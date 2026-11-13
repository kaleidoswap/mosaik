/** The running service: read the book, check settlements on-chain, score, sign. */

import { readFileSync } from 'node:fs';
import { Relay } from './relay.ts';
import type { Chain } from './esplora.ts';
import { isValidEvent, type NostrEvent, type Signer } from './nostr.ts';
import { OFFER_KIND, PROTOCOL_TAG, parseOffer, tag } from './offers.ts';
import { findSettlement, type EsploraTx, type SwapMatch } from './settlement.ts';
import { BUCKET_SECS, WINDOW_SECS, attributeSettlements, buildClusters, scoreMaker, type MakerScore, type SettleState } from './scoring.ts';
import { CATALOG_KIND, REPUTATION_KIND, REPUTATION_TTL_SECS, catalogD, catalogTemplate, reputationTemplate } from './events.ts';
import { parseCatalog } from './catalog.ts';
import type { CheckJob, Store } from './store.ts';
import type { Config } from './config.ts';
import { log } from './log.ts';

export const PROFILE_KIND = 30051;
export const PROFILE_D = 'mosaik-maker';
/** A maker that says `taken` and shows no settlement within this long is counted as unsettled. */
export const SETTLE_DEADLINE_SECS = 48 * 3600;
const MAX_FUTURE_SECS = 600;

export interface CheckResult {
  state: SettleState;
  settlement: SwapMatch | null;
  nextCheck: number | null;
}

/**
 * Look for an order's settlement and decide when to look again.
 * - `txid` tag present: only that transaction can settle it (it must still match the terms).
 * - otherwise: the transactions paying the maker's per-offer receive script.
 * An order last seen `active` (its `taken` update was never seen) is looked at twice after expiry.
 */
export async function checkOrder(job: CheckJob, chain: Chain, now: number): Promise<CheckResult> {
  let txs: EsploraTx[];
  if (job.txidHint) {
    const tx = await chain.tx(job.txidHint);
    txs = tx ? [tx] : [];
  } else if (job.terms.side === 'sell' && job.terms.makerBtcAddress) {
    txs = await chain.addressTxs(job.terms.makerBtcAddress);
  } else if (job.terms.side === 'buy' && job.terms.makerRgbScriptHex) {
    txs = await chain.scriptTxs(job.terms.makerRgbScriptHex);
  } else {
    return { state: 'none', settlement: null, nextCheck: null };
  }
  const m = findSettlement(job.terms, txs);
  if (m?.confirmed) return { state: 'settled', settlement: m, nextCheck: null };
  const backoff = Math.min(3600, 300 * 2 ** Math.min(job.checks, 4));
  // decided by whether it was ever taken, not by its latest status: a maker cannot step out of the
  // deadline by publishing another status afterwards
  if (job.takenAt !== null) {
    const since = job.takenAt;
    if (!m && now - since > SETTLE_DEADLINE_SECS) return { state: 'unsettled', settlement: null, nextCheck: null };
    return { state: 'pending', settlement: m, nextCheck: now + (m ? 600 : backoff) };
  }
  // seen only as active: a missed `taken`, or just an offer that ran out
  if (m) return { state: 'pending', settlement: m, nextCheck: now + 600 };
  return { state: 'none', settlement: null, nextCheck: job.checks < 1 ? now + 3600 : null };
}

export class Indexer {
  readonly relay: Relay;
  private timers: NodeJS.Timeout[] = [];
  private checking = false;
  private store: Store;
  private chain: Chain;
  private signer: Signer;
  private cfg: Config;

  constructor(store: Store, chain: Chain, signer: Signer, cfg: Config) {
    this.store = store;
    this.chain = chain;
    this.signer = signer;
    this.cfg = cfg;
    this.relay = new Relay(cfg.relayUrl);
  }

  /** Fold one relay event in. Returns what it was taken as, for logs and tests. */
  ingest(e: NostrEvent, now = Math.floor(Date.now() / 1000)): 'offer' | 'profile' | 'stale' | 'ignored' {
    if (!isValidEvent(e) || e.created_at > now + MAX_FUTURE_SECS) return 'ignored';
    if (e.kind === OFFER_KIND) {
      const o = parseOffer(e);
      if (!o || o.network !== this.cfg.network) return 'ignored';
      // presence is counted when the indexer receives a fresh event, never from the date the maker
      // wrote: a backdated event cannot fill slots in which the maker was away
      if (Math.abs(now - o.createdAt) <= BUCKET_SECS) this.store.markPresent(o.maker, now);
      const fresh = this.store.applyOffer(o, now);
      const last = Number(this.store.getMeta('last_offer_at') ?? 0);
      if (o.createdAt > last) this.store.setMeta('last_offer_at', String(o.createdAt));
      return fresh ? 'offer' : 'stale';
    }
    if (e.kind === PROFILE_KIND && tag(e, 'd') === PROFILE_D) {
      try {
        if (typeof JSON.parse(e.content) !== 'object') return 'ignored';
      } catch {
        return 'ignored';
      }
      this.store.saveProfile(e);
      return 'profile';
    }
    return 'ignored';
  }

  subscribe(onBackfilled?: () => void) {
    let pending = 2;
    const done = () => {
      if (--pending === 0) onBackfilled?.();
    };
    // one tag filter per REQ: the public relay answers nothing to `#d` combined with `#t`
    this.relay.subscribe({
      id: 'offers',
      filter: () => {
        const last = Number(this.store.getMeta('last_offer_at') ?? 0);
        return { kinds: [OFFER_KIND], '#t': [PROTOCOL_TAG], ...(last ? { since: last - 3600 } : {}), limit: 500 };
      },
      onEvent: (e) => this.ingest(e),
      onEose: done,
    });
    this.relay.subscribe({
      id: 'profiles',
      filter: () => ({ kinds: [PROFILE_KIND], '#d': [PROFILE_D], limit: 500 }),
      onEvent: (e) => this.ingest(e),
      onEose: done,
    });
  }

  async runChecks(now = Math.floor(Date.now() / 1000), limit = 50) {
    if (this.checking) return;
    this.checking = true;
    try {
      for (const job of this.store.dueChecks(now, limit)) {
        try {
          const r = await checkOrder(job, this.chain, now);
          this.store.recordCheck(job.maker, job.orderId, r.state, r.settlement, r.nextCheck);
          if (r.state === 'settled') log.info('settlement verified', { maker: job.maker, order: job.orderId, txid: r.settlement?.txid });
        } catch (err) {
          // Esplora down: try the same order again later, without spending its attempts
          log.warn('settlement check failed', { order: job.orderId, error: String(err) });
          this.store.recordCheck(job.maker, job.orderId, job.settleState, null, now + 300, false);
        }
      }
    } finally {
      this.checking = false;
    }
  }

  scores(now = Math.floor(Date.now() / 1000)): MakerScore[] {
    const orders = attributeSettlements(this.store.scoredOrders());
    const clusters = buildClusters(orders);
    const fromBucket = Math.floor((now - WINDOW_SECS) / BUCKET_SECS);
    const observed = this.store.coverage(fromBucket);
    return this.store.makers().map((m) => {
      const s = scoreMaker(m, orders, clusters, { present: this.store.presenceOf(m, fromBucket), observed }, now, this.cfg.thresholds);
      // first seen over all the maker's offers, not only the traded ones
      return { ...s, firstSeen: this.store.firstSeen(m) };
    });
  }

  /**
   * Sign every maker's score and the curated list, and publish what changed. The relay rate-limits,
   * so an event is re-signed only when it says something new (or a score nears its expiration),
   * and each signed event is sent to the relay once.
   */
  async signAndPublish(now = Math.floor(Date.now() / 1000)) {
    const signed: NostrEvent[] = [];
    for (const s of this.scores(now)) {
      const tpl = reputationTemplate(s, this.cfg.network, now);
      const prev = this.store.published(REPUTATION_KIND, s.maker)[0];
      if (!prev || prev.pubkey !== this.signer.pubkey || scoreChanged(prev, tpl, now)) signed.push(this.signer.sign(tpl));
    }
    try {
      const catalog = parseCatalog(JSON.parse(readFileSync(this.cfg.catalogPath, 'utf8')));
      if (catalog.network !== this.cfg.network) throw new Error(`catalog is for ${catalog.network}`);
      const prev = this.store.published(CATALOG_KIND, catalogD(catalog.network))[0];
      // a list is re-signed only when it changed, so its created_at says when it last did
      const tpl = catalogTemplate(catalog, now);
      if (!prev || prev.content !== tpl.content || prev.pubkey !== this.signer.pubkey) signed.push(this.signer.sign(tpl));
    } catch (err) {
      log.error('curated list not signed', { error: String(err) });
    }
    for (const e of signed) this.store.savePublished(e);
    let sent = 0;
    if (this.cfg.publish) {
      // everything signed and not yet accepted by the relay, including a list signed while publishing was off
      const pending = [...this.store.published(REPUTATION_KIND), ...this.store.published(CATALOG_KIND)].filter(
        (e) => e.pubkey === this.signer.pubkey && this.store.getMeta(`relayed:${e.kind}:${tag(e, 'd')}`) !== e.id,
      );
      for (const e of pending) {
        const r = await this.relay.publish(e).catch((err) => ({ ok: false, message: String(err) }));
        if (r.ok) {
          this.store.setMeta(`relayed:${e.kind}:${tag(e, 'd')}`, e.id);
          sent++;
        } else log.warn('publish refused', { kind: e.kind, d: tag(e, 'd'), message: r.message });
      }
    }
    log.info('signing run', { signed: signed.length, published: sent, publishing: this.cfg.publish });
    return signed;
  }

  start() {
    this.relay.onState = (live) => log.info(live ? 'subscribed' : 'not subscribed');
    this.subscribe();
    this.relay.start();
    const every = (secs: number, f: () => void) => this.timers.push(setInterval(f, secs * 1000));
    // coverage: buckets in which the indexer could have seen a maker's heartbeat
    every(30, () => {
      if (this.relay.live) this.store.markCovered(Math.floor(Date.now() / 1000));
    });
    every(60, () => void this.runChecks());
    every(this.cfg.scoreEverySecs, () => void this.signAndPublish().catch((e) => log.error('sign failed', { error: String(e) })));
    every(3600, () => this.store.prune(Math.floor((Date.now() / 1000 - WINDOW_SECS) / BUCKET_SECS) - 1));
  }

  stop() {
    for (const t of this.timers) clearInterval(t);
    this.relay.stop();
  }
}


/** Tags whose drift alone is not worth a new event. */
const VOLATILE = new Set(['expiration', 'presence']);
const PRESENCE_STEP = 0.01;

/**
 * Whether a fresh score says something the published one does not: any count, the badge, a
 * presence move of a point or more, or the old event being past half its lifetime.
 */
export function scoreChanged(prev: NostrEvent, next: { tags: string[][] }, now: number): boolean {
  if (now - prev.created_at > REPUTATION_TTL_SECS / 2) return true;
  const firm = (tags: string[][]) => JSON.stringify(tags.filter((t) => !VOLATILE.has(t[0]!)));
  if (firm(prev.tags) !== firm(next.tags)) return true;
  const p = (tags: string[][]) => tags.find((t) => t[0] === 'presence')?.[1];
  const a = p(prev.tags);
  const b = p(next.tags);
  if (a === undefined || b === undefined) return a !== b;
  return Math.abs(Number(a) - Number(b)) >= PRESENCE_STEP;
}

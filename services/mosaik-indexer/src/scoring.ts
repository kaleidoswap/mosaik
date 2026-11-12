/**
 * Maker reputation from what anyone can check: the public book and the chain.
 *
 * Pure functions over the indexer's records, so the rules are testable without a relay or Esplora.
 * Every rule is described in services/mosaik-indexer/README.md ("Scoring rules").
 */

import type { Side } from './offers.ts';
import type { SwapMatch } from './settlement.ts';

export const DAY = 86_400;
export const WINDOW_SECS = 30 * DAY;
/** p2pd republishes each open offer every 60 s; five minutes without one means the maker is away. */
export const BUCKET_SECS = 300;

export type SettleState = 'none' | 'pending' | 'settled' | 'unsettled';

export interface OrderRecord {
  maker: string;
  orderId: string;
  side: Side;
  assetId: string;
  amount: number;
  priceSat: number;
  /** `created_at` of the first event of the order: the maker's own clock. */
  firstSeen: number;
  lastSeen: number;
  /** When the indexer first received the order: its own clock. */
  receivedAt: number;
  /** When a `taken` status was first seen: the maker accepted a taker's request. */
  takenAt: number | null;
  /** `taken`, then `cancelled`: the negotiation did not finish (p2pd's `release`). */
  aborted: boolean;
  settleState: SettleState;
  settlement: SwapMatch | null;
}

export interface Thresholds {
  minSettled: number;
  minCounterparties: number;
  minPresence: number;
  maxAbortRate: number;
}

/** Provisional: the badge thresholds are an open decision in the design doc. */
export const DEFAULT_THRESHOLDS: Thresholds = { minSettled: 3, minCounterparties: 2, minPresence: 0.9, maxAbortRate: 0.2 };

export interface SettledTrade {
  orderId: string;
  txid: string;
  height: number | null;
  at: number;
  side: Side;
  assetId: string;
  amount: number;
  priceSat: number;
  /** Paid by the maker itself, as far as the chain shows. */
  self: boolean;
  /** The taker's inputs could not be told from the maker's. */
  unattributed: boolean;
}

export interface MakerScore {
  maker: string;
  window: { from: number; to: number };
  firstSeen: number | null;
  presence: { ratio: number | null; presentBuckets: number; observedBuckets: number; bucketSecs: number };
  negotiations: { taken: number; settled: number; aborted: number; unsettled: number; pending: number; abortRate: number | null };
  volume: {
    /** BTC leg of settled trades with a distinct counterparty (self-trades and unattributed ones excluded). */
    sat: number;
    trades: number;
    counterparties: number;
    selfTrades: number;
    unattributedTrades: number;
    byAsset: Record<string, { trades: number; amount: number; sat: number }>;
  };
  verified: boolean;
  thresholds: Thresholds;
  settlements: SettledTrade[];
}

/**
 * One settlement, one maker. Offers are public, so anyone can republish another maker's terms
 * (receive script included) under their own key and point at that maker's swap. A transaction is
 * credited only to the order whose offer the indexer received first, and only if that offer was
 * dated before the transaction confirmed. Two claims received at the same time (both backfilled
 * at start-up, say) are left to nobody. Orders that lose drop out of scoring altogether: they are
 * neither trades nor aborts, and their scripts are not clustered.
 */
export function attributeSettlements(orders: OrderRecord[]): OrderRecord[] {
  const claims = new Map<string, OrderRecord[]>();
  for (const o of orders) {
    if (o.settleState !== 'settled' || !o.settlement) continue;
    const list = claims.get(o.settlement.txid) ?? [];
    list.push(o);
    claims.set(o.settlement.txid, list);
  }
  const losers = new Set<OrderRecord>();
  for (const list of claims.values()) {
    const plausible = list.filter((o) => o.settlement!.blockTime === null || o.firstSeen <= o.settlement!.blockTime);
    for (const o of list) if (!plausible.includes(o)) losers.add(o);
    if (plausible.length <= 1) continue;
    const first = Math.min(...plausible.map((o) => o.receivedAt));
    const winners = plausible.filter((o) => o.receivedAt === first);
    const keep = winners.length === 1 ? winners[0] : null;
    // the same maker listing the terms twice is still one trade: keep its earliest order only
    for (const o of plausible) if (o !== keep) losers.add(o);
  }
  return orders.filter((o) => !losers.has(o));
}

/** Union-find over makers and scripts: inputs one transaction side signed share an owner. */
export class Clusters {
  private parent = new Map<string, string>();
  find(x: string): string {
    let p = this.parent.get(x) ?? x;
    if (p === x) return x;
    p = this.find(p);
    this.parent.set(x, p);
    return p;
  }
  union(a: string, b: string) {
    const ra = this.find(a);
    const rb = this.find(b);
    if (ra !== rb) this.parent.set(rb, ra);
  }
}

/**
 * Owners of coins, across every settled trade of every maker:
 * - a maker owns the inputs it signed, its change and the script it received on;
 * - a taker's inputs in one transaction share an owner.
 * Taker change and receive scripts are left out: the taker chooses them and could point them at a
 * maker to smear it. A coin the maker received and later spends as a taker joins the two clusters.
 */
export function buildClusters(orders: OrderRecord[]): Clusters {
  const c = new Clusters();
  for (const o of orders) {
    const s = o.settleState === 'settled' ? o.settlement : null;
    if (!s) continue;
    const m = `m:${o.maker}`;
    c.union(m, `s:${s.makerReceiveScript}`);
    if (s.makerChangeScript) c.union(m, `s:${s.makerChangeScript}`);
    for (const sc of s.makerInputScripts ?? []) c.union(m, `s:${sc}`);
    const [first, ...rest] = s.takerInputScripts ?? [];
    if (first) for (const sc of rest) c.union(`s:${first}`, `s:${sc}`);
  }
  return c;
}

export interface PresenceInput {
  /** Buckets (unix secs / BUCKET_SECS) in which the maker published a kind-30053 event. */
  present: Iterable<number>;
  /** Buckets in which the indexer was subscribed and could have seen one. */
  observed: Iterable<number>;
}

export function presence(input: PresenceInput, from: number, to: number) {
  const lo = Math.floor(from / BUCKET_SECS);
  const hi = Math.floor(to / BUCKET_SECS);
  const observed = new Set([...input.observed].filter((b) => b >= lo && b <= hi));
  let present = 0;
  for (const b of new Set(input.present)) if (observed.has(b)) present++;
  return {
    ratio: observed.size ? round4(present / observed.size) : null,
    presentBuckets: present,
    observedBuckets: observed.size,
    bucketSecs: BUCKET_SECS,
  };
}

const round4 = (x: number) => Math.round(x * 10_000) / 10_000;

/** When an order's trade happened: confirmation time once settled, else when it was taken. */
const tradeTime = (o: OrderRecord) => o.settlement?.blockTime ?? o.takenAt ?? o.lastSeen;

export function scoreMaker(
  maker: string,
  orders: OrderRecord[],
  clusters: Clusters,
  pres: PresenceInput,
  now: number,
  thresholds: Thresholds = DEFAULT_THRESHOLDS,
): MakerScore {
  const from = now - WINDOW_SECS;
  const mine = orders.filter((o) => o.maker === maker);
  const inWindow = mine.filter((o) => (o.takenAt !== null || o.settleState === 'settled') && tradeTime(o) >= from && tradeTime(o) <= now);

  const settledOrders = inWindow.filter((o) => o.settleState === 'settled' && o.settlement?.confirmed);
  const aborted = inWindow.filter((o) => o.aborted && o.settleState !== 'settled').length;
  const unsettled = inWindow.filter((o) => !o.aborted && o.settleState === 'unsettled').length;
  const pending = inWindow.filter((o) => !o.aborted && o.settleState === 'pending').length;
  const resolved = settledOrders.length + aborted + unsettled;

  const self = clusters.find(`m:${maker}`);
  const counterparties = new Set<string>();
  const byAsset: MakerScore['volume']['byAsset'] = {};
  let sat = 0;
  let trades = 0;
  let selfTrades = 0;
  let unattributedTrades = 0;
  const settlements: SettledTrade[] = [];
  for (const o of settledOrders) {
    const s = o.settlement!;
    const first = s.takerInputScripts?.[0];
    const unattributed = !first;
    const owner = first ? clusters.find(`s:${first}`) : null;
    const isSelf = owner === self;
    settlements.push({
      orderId: o.orderId,
      txid: s.txid,
      height: s.height,
      at: tradeTime(o),
      side: o.side,
      assetId: o.assetId,
      amount: o.amount,
      priceSat: o.priceSat,
      self: isSelf,
      unattributed,
    });
    if (unattributed) {
      unattributedTrades++;
      continue;
    }
    if (isSelf) {
      selfTrades++;
      continue;
    }
    counterparties.add(owner!);
    trades++;
    sat += o.priceSat;
    const a = (byAsset[o.assetId] ??= { trades: 0, amount: 0, sat: 0 });
    a.trades++;
    a.amount += o.amount;
    a.sat += o.priceSat;
  }
  settlements.sort((a, b) => b.at - a.at);

  const p = presence(pres, from, now);
  const abortRate = resolved ? round4((aborted + unsettled) / resolved) : null;
  const verified =
    trades >= thresholds.minSettled &&
    counterparties.size >= thresholds.minCounterparties &&
    p.ratio !== null &&
    p.ratio >= thresholds.minPresence &&
    (abortRate ?? 0) <= thresholds.maxAbortRate;

  return {
    maker,
    window: { from, to: now },
    firstSeen: mine.length ? Math.min(...mine.map((o) => o.firstSeen)) : null,
    presence: p,
    negotiations: {
      taken: inWindow.length,
      settled: settledOrders.length,
      aborted,
      unsettled,
      pending,
      abortRate,
    },
    volume: { sat, trades, counterparties: counterparties.size, selfTrades, unattributedTrades, byAsset },
    verified,
    thresholds,
    settlements,
  };
}

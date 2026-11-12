/**
 * The two events the indexer signs. The format is specified in docs/PROTOCOL.md (kinds 30054 and
 * 30055); this file is the reference encoder and decoder. The decoders take only nostr-tools and
 * are meant to be copied into clients (the webapp) as they are.
 */

import { isValidEvent, type NostrEvent, type Template } from './nostr.ts';
import { parseCatalog, type Catalog } from './catalog.ts';
import type { MakerScore } from './scoring.ts';

export const REPUTATION_KIND = 30054;
export const REPUTATION_TAG = 'mosaik-reputation-v1';
export const CATALOG_KIND = 30055;
export const CATALOG_TAG = 'mosaik-catalog-v1';
export const catalogD = (network: string) => `mosaik-catalog:${network}`;

/** How long a score stays valid on relays (NIP-40). The indexer republishes far more often. */
export const REPUTATION_TTL_SECS = 2 * 86_400;
/** At most this many settlements ride in the content, newest first. */
export const MAX_SETTLEMENTS = 50;

const ratio = (x: number | null) => (x === null ? undefined : x.toFixed(4));

export function reputationTemplate(score: MakerScore, network: string, now: number): Template {
  const n = score.negotiations;
  const v = score.volume;
  const tags: string[][] = [
    ['d', score.maker],
    ['p', score.maker],
    ['t', REPUTATION_TAG],
    ['network', network],
    ['window', String(score.window.to - score.window.from)],
    ['settled', String(v.trades)],
    ['volume_sat', String(v.sat)],
    ['counterparties', String(v.counterparties)],
    ['self_trades', String(v.selfTrades)],
    ['negotiations', String(n.taken)],
    ['aborted', String(n.aborted + n.unsettled)],
    ['verified', score.verified ? '1' : '0'],
    ['expiration', String(now + REPUTATION_TTL_SECS)],
  ];
  const presence = ratio(score.presence.ratio);
  if (presence !== undefined) tags.push(['presence', presence]);
  const abortRate = ratio(n.abortRate);
  if (abortRate !== undefined) tags.push(['abort_rate', abortRate]);
  const content = { v: 1, network, computedAt: now, ...score, settlements: score.settlements.slice(0, MAX_SETTLEMENTS) };
  return { kind: REPUTATION_KIND, created_at: now, tags, content: JSON.stringify(content) };
}

export function catalogTemplate(catalog: Catalog, now: number): Template {
  const tags: string[][] = [
    ['d', catalogD(catalog.network)],
    ['t', CATALOG_TAG],
    ['network', catalog.network],
    ['version', catalog.version],
  ];
  for (const t of catalog.tokens) {
    tags.push(['asset', t.contract_id, t.ticker]);
    for (const p of (t.issuer_makers as string[] | undefined) ?? []) tags.push(['issuer_maker', t.contract_id, p]);
  }
  return { kind: CATALOG_KIND, created_at: now, tags, content: JSON.stringify(catalog) };
}

const tag = (e: NostrEvent, name: string) => e.tags.find((t) => t[0] === name)?.[1];

export type Verified<T> = { ok: true; value: T } | { ok: false; error: string };

/** Summary fields of a reputation event, after checking its signer, signature and shape. */
export interface ReputationSummary {
  maker: string;
  network: string;
  computedAt: number;
  expiresAt: number;
  settled: number;
  volumeSat: number;
  counterparties: number;
  selfTrades: number;
  negotiations: number;
  aborted: number;
  abortRate: number | null;
  presence: number | null;
  verified: boolean;
  /** The full score, as the indexer computed it. */
  score: MakerScore;
}

const num = (s: string | undefined) => (s !== undefined && /^\d{1,16}$/.test(s) ? Number(s) : undefined);
const frac = (s: string | undefined) => (s !== undefined && /^(0|1)\.\d{4}$/.test(s) ? Number(s) : null);

export function verifyReputationEvent(e: NostrEvent, indexerPubkey: string, now = Math.floor(Date.now() / 1000)): Verified<ReputationSummary> {
  if (e.kind !== REPUTATION_KIND) return { ok: false, error: 'not kind 30054' };
  if (e.pubkey !== indexerPubkey) return { ok: false, error: 'not signed by the indexer key' };
  if (!isValidEvent(e)) return { ok: false, error: 'bad id or signature' };
  if (tag(e, 't') !== REPUTATION_TAG) return { ok: false, error: 'missing t=mosaik-reputation-v1' };
  const maker = tag(e, 'd');
  if (!maker || !/^[0-9a-f]{64}$/.test(maker) || tag(e, 'p') !== maker) return { ok: false, error: 'd and p must both be the maker pubkey' };
  const expiresAt = num(tag(e, 'expiration'));
  if (expiresAt === undefined || expiresAt <= now) return { ok: false, error: 'expired' };
  const network = tag(e, 'network');
  const fields = ['settled', 'volume_sat', 'counterparties', 'self_trades', 'negotiations', 'aborted'].map((k) => num(tag(e, k)));
  const verified = tag(e, 'verified');
  if (!network || fields.some((f) => f === undefined) || (verified !== '0' && verified !== '1')) return { ok: false, error: 'malformed tags' };
  let score: MakerScore & { v?: number; computedAt?: number };
  try {
    score = JSON.parse(e.content);
  } catch {
    return { ok: false, error: 'content is not JSON' };
  }
  if (score.v !== 1 || score.maker !== maker) return { ok: false, error: 'content does not match the tags' };
  const [settled, volumeSat, counterparties, selfTrades, negotiations, aborted] = fields as number[];
  if (score.volume?.trades !== settled || score.volume?.sat !== volumeSat || score.verified !== (verified === '1'))
    return { ok: false, error: 'content does not match the tags' };
  return {
    ok: true,
    value: {
      maker,
      network,
      computedAt: score.computedAt ?? e.created_at,
      expiresAt,
      settled: settled!,
      volumeSat: volumeSat!,
      counterparties: counterparties!,
      selfTrades: selfTrades!,
      negotiations: negotiations!,
      aborted: aborted!,
      abortRate: frac(tag(e, 'abort_rate')),
      presence: frac(tag(e, 'presence')),
      verified: verified === '1',
      score,
    },
  };
}

export function verifyCatalogEvent(e: NostrEvent, listPubkey: string, network: string): Verified<Catalog> {
  if (e.kind !== CATALOG_KIND) return { ok: false, error: 'not kind 30055' };
  if (e.pubkey !== listPubkey) return { ok: false, error: 'not signed by the list key' };
  if (!isValidEvent(e)) return { ok: false, error: 'bad id or signature' };
  if (tag(e, 't') !== CATALOG_TAG || tag(e, 'd') !== catalogD(network) || tag(e, 'network') !== network)
    return { ok: false, error: 'wrong t, d or network tag' };
  let catalog: Catalog;
  try {
    catalog = parseCatalog(JSON.parse(e.content));
  } catch (err) {
    return { ok: false, error: (err as Error).message };
  }
  if (catalog.network !== network || catalog.version !== tag(e, 'version')) return { ok: false, error: 'content does not match the tags' };
  const tagged = e.tags.filter((t) => t[0] === 'asset').map((t) => `${t[1]} ${t[2]}`).sort();
  const listed = catalog.tokens.map((t) => `${t.contract_id} ${t.ticker}`).sort();
  if (tagged.length !== listed.length || tagged.some((x, i) => x !== listed[i])) return { ok: false, error: 'asset tags do not match the content' };
  return { ok: true, value: catalog };
}

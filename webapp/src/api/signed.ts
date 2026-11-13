/**
 * The two events Kaleido signs for the market, read from the relay and checked here before use:
 *
 * - kind 30055, the curated asset list (the `mosaik-catalog/v1` JSON, signed by the list key);
 * - kind 30054, one reputation score per maker (signed by the Mosaik indexer key).
 *
 * The formats are in docs/PROTOCOL.md. The checks mirror the reference verifiers in
 * services/mosaik-indexer/src/events.ts. An event that fails any check is dropped, never shown.
 */

import { useQuery } from '@tanstack/react-query';
import { SimplePool, verifyEvent, type Event } from 'nostr-tools';

export const CATALOG_KIND = 30055;
export const REPUTATION_KIND = 30054;

/**
 * Mutinynet development key of the Mosaik indexer, which today signs both events. Which key signs
 * them in production, and where it is kept, is an open decision: set the env vars when it is made.
 */
export const DEV_KALEIDO_KEY = '0edf1c099dd6896fd607776a7b137648446d8e03856e4801caa296c6263c1937';
const HEX64 = /^[0-9a-f]{64}$/;
const keyFromEnv = (v: unknown) => (typeof v === 'string' && HEX64.test(v) ? v : DEV_KALEIDO_KEY);
export const LIST_PUBKEY = keyFromEnv(import.meta.env.VITE_KALEIDO_LIST_PUBKEY);
export const INDEXER_PUBKEY = keyFromEnv(import.meta.env.VITE_MOSAIK_INDEXER_PUBKEY);
const RELAY = import.meta.env.VITE_RGB_RELAY_URL ?? 'wss://relay.kaleidoswap.com';

const tag = (e: Event, name: string) => e.tags.find((t) => t[0] === name)?.[1];

/** nostr-tools remembers a verdict on the object; check a plain copy so only the fields decide. */
export function validSignature(e: Event): boolean {
  try {
    const { id, pubkey, created_at, kind, tags, content, sig } = e;
    return verifyEvent({ id, pubkey, created_at, kind, tags, content, sig } as Event);
  } catch {
    return false;
  }
}

export type Checked<T> = { ok: true; value: T } | { ok: false; error: string };

export interface SignedList {
  /** The catalog JSON as signed; parse it with `parseCatalog`. */
  content: unknown;
  eventId: string;
  signedAt: number;
}

export function checkCatalogEvent(e: Event, pubkey: string, network: string): Checked<SignedList> {
  if (e.kind !== CATALOG_KIND) return { ok: false, error: 'not a curated list' };
  if (e.pubkey !== pubkey) return { ok: false, error: 'not signed by the Kaleido list key' };
  if (!validSignature(e)) return { ok: false, error: 'bad signature' };
  if (tag(e, 't') !== 'mosaik-catalog-v1' || tag(e, 'd') !== `mosaik-catalog:${network}` || tag(e, 'network') !== network)
    return { ok: false, error: 'list is for another network or format' };
  let content: { network?: unknown; version?: unknown; tokens?: { contract_id?: unknown; ticker?: unknown }[] };
  try {
    content = JSON.parse(e.content);
  } catch {
    return { ok: false, error: 'list content is not JSON' };
  }
  if (content.network !== network || content.version !== tag(e, 'version') || !Array.isArray(content.tokens))
    return { ok: false, error: 'list content does not match its tags' };
  const tagged = e.tags.filter((t) => t[0] === 'asset').map((t) => `${t[1]} ${t[2]}`).sort();
  const listed = content.tokens.map((t) => `${String(t.contract_id)} ${String(t.ticker)}`).sort();
  if (tagged.length !== listed.length || tagged.some((x, i) => x !== listed[i])) return { ok: false, error: 'asset tags do not match the list' };
  return { ok: true, value: { content, eventId: e.id, signedAt: e.created_at } };
}

export interface AssetVolume {
  trades: number;
  amount: number;
  sat: number;
  /** Distinct within the asset; absent from scores signed before it was added. */
  counterparties?: number;
}

/** A maker's score, as the indexer measured it from the relay and the chain. */
export interface MakerScore {
  maker: string;
  computedAt: number;
  expiresAt: number;
  /** Settled trades with a distinct counterparty (self-trades and unattributed ones excluded). */
  settled: number;
  volumeSat: number;
  counterparties: number;
  selfTrades: number;
  negotiations: number;
  aborted: number;
  abortRate: number | null;
  presence: number | null;
  byAsset: Record<string, AssetVolume>;
}

const count = (s: string | undefined) => (s !== undefined && /^\d{1,16}$/.test(s) ? Number(s) : undefined);
const ratio = (s: string | undefined) => (s !== undefined && /^(0|1)\.\d{4}$/.test(s) ? Number(s) : null);

export function checkReputationEvent(e: Event, pubkey: string, network: string, now: number): Checked<MakerScore> {
  if (e.kind !== REPUTATION_KIND) return { ok: false, error: 'not a reputation score' };
  if (e.pubkey !== pubkey) return { ok: false, error: 'not signed by the indexer key' };
  if (!validSignature(e)) return { ok: false, error: 'bad signature' };
  const maker = tag(e, 'd');
  if (tag(e, 't') !== 'mosaik-reputation-v1' || tag(e, 'network') !== network) return { ok: false, error: 'another network or format' };
  if (!maker || !HEX64.test(maker) || tag(e, 'p') !== maker) return { ok: false, error: 'malformed maker' };
  const expiresAt = count(tag(e, 'expiration'));
  if (expiresAt === undefined || expiresAt <= now) return { ok: false, error: 'expired' };
  const n = ['settled', 'volume_sat', 'counterparties', 'self_trades', 'negotiations', 'aborted'].map((k) => count(tag(e, k)));
  if (n.some((x) => x === undefined)) return { ok: false, error: 'malformed tags' };
  const [settled, volumeSat, counterparties, selfTrades, negotiations, aborted] = n as number[];
  let content: { v?: unknown; maker?: unknown; computedAt?: unknown; volume?: { trades?: unknown; sat?: unknown; byAsset?: unknown } };
  try {
    content = JSON.parse(e.content);
  } catch {
    return { ok: false, error: 'content is not JSON' };
  }
  if (content.v !== 1 || content.maker !== maker || content.volume?.trades !== settled || content.volume?.sat !== volumeSat)
    return { ok: false, error: 'content does not match the tags' };
  const byAsset: Record<string, AssetVolume> = {};
  const raw = content.volume?.byAsset;
  if (raw && typeof raw === 'object') {
    for (const [asset, v] of Object.entries(raw as Record<string, Record<string, unknown>>)) {
      const ok = (x: unknown) => typeof x === 'number' && Number.isSafeInteger(x) && x >= 0;
      if (!/^rgb:[A-Za-z0-9~_:-]{8,120}$/.test(asset) || !ok(v.trades) || !ok(v.amount) || !ok(v.sat)) continue;
      byAsset[asset] = { trades: v.trades as number, amount: v.amount as number, sat: v.sat as number, ...(ok(v.counterparties) ? { counterparties: v.counterparties as number } : {}) };
    }
  }
  return {
    ok: true,
    value: {
      maker,
      computedAt: typeof content.computedAt === 'number' ? content.computedAt : e.created_at,
      expiresAt,
      settled: settled!,
      volumeSat: volumeSat!,
      counterparties: counterparties!,
      selfTrades: selfTrades!,
      negotiations: negotiations!,
      aborted: aborted!,
      abortRate: ratio(tag(e, 'abort_rate')),
      presence: ratio(tag(e, 'presence')),
      byAsset,
    },
  };
}

/** Newest checked score per maker; anything that fails a check is left out. */
export function scoresFrom(events: Event[], pubkey: string, network: string, now: number): Map<string, MakerScore> {
  const out = new Map<string, { s: MakerScore; at: number }>();
  for (const e of events) {
    const r = checkReputationEvent(e, pubkey, network, now);
    if (!r.ok) continue;
    const prev = out.get(r.value.maker);
    if (!prev || prev.at < e.created_at) out.set(r.value.maker, { s: r.value, at: e.created_at });
  }
  return new Map([...out].map(([k, v]) => [k, v.s]));
}

/** The newest list that passes every check, or why none did. */
export function newestList(events: Event[], pubkey: string, network: string): Checked<SignedList> {
  let best: SignedList | undefined;
  let error = 'no curated list on the relay';
  for (const e of events) {
    const r = checkCatalogEvent(e, pubkey, network);
    if (!r.ok) error = r.error;
    else if (!best || best.signedAt < r.value.signedAt) best = r.value;
  }
  return best ? { ok: true, value: best } : { ok: false, error };
}

let pool: SimplePool | undefined;
async function query(filter: Parameters<SimplePool['querySync']>[1]): Promise<Event[]> {
  pool ??= new SimplePool();
  return pool.querySync([RELAY], filter, { maxWait: 6000 });
}

export async function fetchSignedList(network: string): Promise<Checked<SignedList>> {
  // authors plus one tag: the public relay answers nothing when two tag filters are combined
  const events = await query({ kinds: [CATALOG_KIND], authors: [LIST_PUBKEY], '#d': [`mosaik-catalog:${network}`] });
  return newestList(events, LIST_PUBKEY, network);
}

export function useMakerScores(network: string | undefined) {
  return useQuery({
    queryKey: ['maker-scores', network, INDEXER_PUBKEY],
    enabled: !!network,
    staleTime: 5 * 60_000,
    refetchInterval: 10 * 60_000,
    retry: 1,
    queryFn: async () => scoresFrom(await query({ kinds: [REPUTATION_KIND], authors: [INDEXER_PUBKEY] }), INDEXER_PUBKEY, network!, Math.floor(Date.now() / 1000)),
  });
}

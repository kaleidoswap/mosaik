/**
 * Listing tiers and maker marks. Nothing here approves an asset or a maker: the book stays open to
 * anyone. These are labels on what can be checked (a signed list, the registry, measured trades).
 * The asset id is the only identity: every lookup below is by contract id, never by ticker.
 */

import type { Catalog } from '../../api/catalog';
import type { Listed } from '../../api/registry';
import type { MakerScore } from '../../api/signed';

export type ListingTier = 'curated' | 'registry' | 'unlisted' | 'unknown';

export interface Listing {
  tier: ListingTier;
  label: string;
  detail: string;
}

/**
 * - curated: in the Kaleido list, and that list's signature checked out;
 * - registry: listed in the RGBMap index (the issuer declared it there);
 * - unlisted: neither (the market still shows, with warnings);
 * - unknown: not curated, and the registry could not be read.
 */
export function listingOf(assetId: string, catalog: Catalog | undefined, registry: Listed[] | undefined): Listing {
  if (catalog?.signed && catalog.assets.some((a) => a.contractId === assetId))
    return { tier: 'curated', label: 'Kaleido curated', detail: 'In the curated list signed by the Kaleido key. A featured market, not an endorsement of the asset.' };
  if (registry?.some((l) => l.contractId === assetId))
    return { tier: 'registry', label: 'RGBMap listed', detail: 'Listed in the RGBMap registry. The listing is the issuer’s declaration; check the asset id.' };
  if (registry === undefined)
    return { tier: 'unknown', label: 'Listing unknown', detail: 'The registry could not be read, so the listing status is unknown. That does not mean unlisted.' };
  return { tier: 'unlisted', label: 'Unlisted', detail: 'Not in the Kaleido curated list or the RGBMap registry. Anyone can publish an offer for any asset; check the asset id before trading.' };
}

/**
 * When a maker counts as verified. An open decision in the design doc: these defaults match the
 * indexer's provisional ones and each can be set at build time. The page decides from the signed
 * numbers with its own thresholds; it does not take the indexer's own `verified` tag.
 */
export interface Thresholds {
  minSettled: number;
  minCounterparties: number;
  minPresence: number;
  maxAbortRate: number;
}

const envNum = (v: unknown, d: number) => {
  const n = typeof v === 'string' && v.trim() !== '' ? Number(v) : NaN;
  return Number.isFinite(n) ? n : d;
};

export const VERIFIED_THRESHOLDS: Thresholds = {
  minSettled: envNum(import.meta.env.VITE_VERIFIED_MIN_SETTLED, 3),
  minCounterparties: envNum(import.meta.env.VITE_VERIFIED_MIN_COUNTERPARTIES, 2),
  minPresence: envNum(import.meta.env.VITE_VERIFIED_MIN_PRESENCE, 0.9),
  maxAbortRate: envNum(import.meta.env.VITE_VERIFIED_MAX_ABORT_RATE, 0.2),
};

export function isVerifiedMaker(s: MakerScore | undefined, t: Thresholds = VERIFIED_THRESHOLDS): boolean {
  if (!s || s.presence === null) return false;
  return s.settled >= t.minSettled && s.counterparties >= t.minCounterparties && s.presence >= t.minPresence && (s.abortRate ?? 0) <= t.maxAbortRate;
}

export interface MakerMark {
  verified: boolean;
  /** The curated list names this maker key as the issuer's (or acting for it) for this asset. */
  issuer: boolean;
  score?: MakerScore;
}

/** Marks only from a signed list: an issuer label from an unsigned file would be a claim, not a fact. */
export function makerMark(maker: string, assetId: string, catalog: Catalog | undefined, scores: Map<string, MakerScore> | undefined, t?: Thresholds): MakerMark {
  const score = scores?.get(maker);
  const asset = catalog?.signed ? catalog.assets.find((a) => a.contractId === assetId) : undefined;
  return { verified: isVerifiedMaker(score, t), issuer: !!asset?.issuerMakers.includes(maker), score };
}

export interface MarketMakerRow {
  maker: string;
  mark: MakerMark;
  /** Orders this maker has open on the market now. */
  open: number;
  trades: number;
  sat: number;
  /** Distinct counterparties within this asset; undefined when the score predates the field. */
  counterparties?: number;
}

/**
 * Makers of one market: those with open orders, and those with settled volume in it. Volume is the
 * indexer's, measured on-chain over 30 days, self-trades excluded; counterparties are distinct
 * within the asset. Volumes add up across makers; counterparties do not (one taker can trade with
 * several makers), so there is no market-wide counterparty total.
 */
export function marketMakers(assetId: string, openMakers: string[], catalog: Catalog | undefined, scores: Map<string, MakerScore> | undefined, t?: Thresholds) {
  const open = new Map<string, number>();
  for (const m of openMakers) open.set(m, (open.get(m) ?? 0) + 1);
  const makers = new Set([...open.keys(), ...[...(scores?.values() ?? [])].filter((s) => s.byAsset[assetId]).map((s) => s.maker)]);
  const rows: MarketMakerRow[] = [...makers].map((maker) => {
    const mark = makerMark(maker, assetId, catalog, scores, t);
    const v = mark.score?.byAsset[assetId];
    return { maker, mark, open: open.get(maker) ?? 0, trades: v?.trades ?? 0, sat: v?.sat ?? 0, counterparties: v?.counterparties };
  });
  rows.sort((a, b) => b.sat - a.sat || b.open - a.open || a.maker.localeCompare(b.maker));
  return { rows, trades: rows.reduce((n, r) => n + r.trades, 0), sat: rows.reduce((n, r) => n + r.sat, 0) };
}

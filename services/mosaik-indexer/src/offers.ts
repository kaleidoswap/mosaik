/**
 * Kind 30053 (`t=rgb-psbt-v1`) offers as the indexer reads them.
 *
 * The book facts come from the public tags, as in `webapp/src/api/rgbp2p.ts`. The indexer also
 * needs what only the content carries: the maker's per-offer receive script and the fee terms,
 * because together they fix the layout of the swap transaction on-chain (see `settlement.ts`).
 */

import type { NostrEvent } from './nostr.ts';

export const OFFER_KIND = 30053;
export const PROTOCOL_TAG = 'rgb-psbt-v1';
export const MUTINYNET = 'signetcustom';

export type Side = 'sell' | 'buy';
export type OfferStatus = 'active' | 'taken' | 'cancelled';

export interface ParsedOffer {
  maker: string;
  orderId: string;
  status: OfferStatus;
  side: Side;
  network: string;
  assetId: string;
  /** RGB leg, in the asset's smallest unit. */
  amount: number;
  /** BTC leg, in sats. */
  priceSat: number;
  expiry: number;
  createdAt: number;
  eventId: string;
  /** Where the maker receives: a BTC address (sell) or an RGB witness script (buy). */
  makerBtcAddress: string | null;
  makerRgbScriptHex: string | null;
  rgbOutputSat: number;
  networkFeeSat: number;
  platformFeeSat: number;
  feeScriptHex: string | null;
  /** A `txid` tag, if the maker published one (proposed p2pd change; see docs/PROTOCOL.md). */
  txidHint: string | null;
  ticker: string | null;
  precision: number;
}

export const tag = (e: Pick<NostrEvent, 'tags'>, name: string): string | undefined =>
  e.tags.find((t) => t[0] === name)?.[1];

const whole = (s: unknown): number | undefined => {
  if (typeof s === 'number') return Number.isSafeInteger(s) && s >= 0 ? s : undefined;
  if (typeof s !== 'string' || !/^\d{1,16}$/.test(s)) return undefined;
  return Number(s);
};

const hex = (s: unknown, min = 2, max = 200): string | null =>
  typeof s === 'string' && s.length >= min && s.length <= max && s.length % 2 === 0 && /^[0-9a-f]+$/.test(s) ? s : null;

const ASSET_RE = /^rgb:[A-Za-z0-9~_:-]{8,120}$/;
const ADDRESS_RE = /^(tb1|bcrt1)[02-9ac-hj-np-z]{8,90}$/;
const TXID_RE = /^[0-9a-f]{64}$/;

/**
 * An event as an offer, or null when it is not a well-formed one. Unlike the webapp, every status
 * is kept and expiry is not checked: the history is the point.
 */
export function parseOffer(e: NostrEvent): ParsedOffer | null {
  if (e.kind !== OFFER_KIND || tag(e, 't') !== PROTOCOL_TAG) return null;
  const status = tag(e, 'status');
  if (status !== 'active' && status !== 'taken' && status !== 'cancelled') return null;
  const side = tag(e, 'side');
  if (side !== 'sell' && side !== 'buy') return null;
  const orderId = tag(e, 'd');
  const assetId = tag(e, 'asset');
  const network = tag(e, 'network');
  const amount = whole(tag(e, 'amount'));
  const priceSat = whole(tag(e, 'price'));
  const expiry = whole(tag(e, 'expiry'));
  if (!orderId || orderId.length > 128 || !assetId || !ASSET_RE.test(assetId) || !network) return null;
  if (!amount || !priceSat || !expiry) return null;

  let content: { offer?: Record<string, unknown>; asset?: { ticker?: unknown; precision?: unknown } };
  try {
    content = JSON.parse(e.content);
  } catch {
    return null;
  }
  const o = content.offer;
  if (!o || typeof o !== 'object') return null;
  // the content is authoritative for the swap; it must agree with the tags we index by
  if (o.swapId !== orderId) return null;
  const gives = o.makerGives as { kind?: unknown; assetId?: unknown; amount?: unknown } | undefined;
  const receives = o.makerReceives as { kind?: unknown; assetId?: unknown; amount?: unknown } | undefined;
  const rgbLeg = side === 'sell' ? gives : receives;
  const btcLeg = side === 'sell' ? receives : gives;
  if (rgbLeg?.kind !== 'Rgb' || btcLeg?.kind !== 'Btc') return null;
  if (rgbLeg.assetId !== assetId || whole(rgbLeg.amount) !== amount || whole(btcLeg.amount) !== priceSat) return null;

  const makerBtcAddress = typeof o.makerBtcAddress === 'string' && ADDRESS_RE.test(o.makerBtcAddress) ? o.makerBtcAddress : null;
  const makerRgbScriptHex = hex(o.makerRgbScriptPubkeyHex);
  if (side === 'sell' && !makerBtcAddress) return null;
  if (side === 'buy' && !makerRgbScriptHex) return null;
  const rgbOutputSat = whole(o.rgbOutputSat);
  const networkFeeSat = whole(o.networkFeeSat);
  const platformFeeSat = whole(o.platformFeeSat) ?? 0;
  if (rgbOutputSat === undefined || networkFeeSat === undefined) return null;
  const feeScriptHex = hex(o.feeRecipientScriptPubkeyHex);
  if (platformFeeSat > 0 && !feeScriptHex) return null;

  const txid = tag(e, 'txid');
  const ticker = typeof content.asset?.ticker === 'string' ? content.asset.ticker.slice(0, 12) : null;
  const p = content.asset?.precision;
  const precision = typeof p === 'number' && Number.isInteger(p) && p >= 0 && p <= 18 ? p : 0;

  return {
    maker: e.pubkey,
    orderId,
    status,
    side,
    network,
    assetId,
    amount,
    priceSat,
    expiry,
    createdAt: e.created_at,
    eventId: e.id,
    makerBtcAddress: side === 'sell' ? makerBtcAddress : null,
    makerRgbScriptHex: side === 'buy' ? makerRgbScriptHex : null,
    rgbOutputSat,
    networkFeeSat,
    platformFeeSat,
    feeScriptHex: platformFeeSat > 0 ? feeScriptHex : null,
    txidHint: txid && TXID_RE.test(txid) ? txid : null,
    ticker,
    precision,
  };
}

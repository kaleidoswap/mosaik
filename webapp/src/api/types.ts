/**
 * Types that mirror the mosaik-api JSON shape (see crates/mosaik-api).
 *
 * Hand-rolled for now. When mosaik-api grows utoipa annotations (Phase 6) we
 * generate this file from openapi.json via `openapi-typescript` and stop
 * editing it directly. The runtime shape is authoritative; if it ever
 * disagrees with this file, the API wins.
 */

export type Backend = 'liquid' | 'banco';

export interface WalletSnapshot {
  address: string;
  lbtc_sats: number;
  assets: Record<string, number>;
}

export interface Maker {
  label: string;
  name: string;
}

export interface NostrMeta {
  kind: number;
  event_id: string;
  pubkey: string;
  order_id: string;
  created_at: number;
  relay: string;
}

/** An offer as returned by /api/state — Liquid-shaped today; Phase 2 banco
 *  variant will arrive once mosaik-api routes through mosaik-banco too. */
export interface OfferView {
  index: number;
  outpoint: string;
  amount_a: number;
  amount_b: number;
  lock_name: string;
  want_name: string;
  timeout: number;
  maker_address: string;
  maker: string;
  maker_name: string;
  covenant_address: string | null;
  cmr: string | null;
  nostr: NostrMeta;
}

export interface Trade {
  pair: string;
  lock_name: string;
  want_name: string;
  amount_a: number;
  amount_b: number;
  rate: number;
  txid: string;
  block: number;
  maker_name: string;
}

export interface ApiState {
  block_count: number;
  assets: string[];
  makers: Maker[];
  wallets: Record<string, WalletSnapshot>;
  offers: OfferView[];
  trades: Trade[];
  relay: string;
}

/** POST /api/make-offer body. */
export interface MakeOfferRequest {
  amount_a: number;
  amount_b: number;
  lock: string;
  want: string;
  timeout?: number;
  maker?: string;
}

export interface MakeOfferResponse {
  ok: true;
  index: number;
  outpoint: string;
  maker_address: string;
  maker: string;
  event_id: string;
}

export type Cheat = 'none' | 'underpay' | 'wrong_recipient' | 'wrong_index';

export interface TakeOfferRequest {
  index: number;
  taker?: string;
  cheat?: Cheat;
}

export type TakeOfferResponse =
  | { ok: true; txid: string }
  | { ok: true; rejected: true; reason: string }
  | { ok: false; covenant_bug: true; txid: string; reason: string };

export interface ApiError {
  error: string;
}

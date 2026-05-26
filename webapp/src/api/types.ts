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

/** An offer as returned by /api/state. Both Liquid and banco offers project
 *  into this shape; the optional fields below are backend-specific. */
export interface OfferView {
  backend?: Backend;
  index: number;
  outpoint: string;
  amount_a: number;
  amount_b: number;
  lock_name: string;
  want_name: string;
  /** Liquid: refund block-height. Banco: unix-seconds cancel_delay. */
  timeout: number;
  maker_address: string;
  maker: string;
  maker_name: string;
  covenant_address: string | null;
  cmr: string | null;
  /** Banco-only: the swap covenant scriptPubKey. */
  swap_pk_script?: string;
  /** Banco-only: whether the offer accepts partial fills. */
  partial_fill?: boolean;
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

/** POST /api/make-offer body. `backend` defaults to `liquid` server-side. */
export interface MakeOfferRequest {
  amount_a: number;
  amount_b: number;
  lock: string;
  want: string;
  timeout?: number;
  maker?: string;
  backend?: Backend;
  /** banco-only: accept partial fills. */
  partial_fill?: boolean;
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

// ─── Cross-chain swap (mosaik-bridge) ───────────────────────────────────────
//
// Mirrors the kind-30052 envelope shape — see docs/PROTOCOL.md and
// crates/mosaik-bridge/src/envelope.rs. The preimage itself is intentionally
// not on the wire; only its hash.

export interface LegRef {
  backend: 'liquid' | 'banco' | string;
  outpoint: string;
  event_id?: string;
}

export interface SwapTimelocks {
  a_refund: number;
  b_refund: number;
  safety_margin: number;
}

export type SwapStateLabel =
  | 'drafted'
  | 'half_published'
  | 'both_published'
  | 'revealed'
  | 'settled'
  | 'refunded';

export type RefundReason =
  | 'never_published'
  | 'partner_never_published'
  | 'no_taker_arrived'
  | 'maker_missed_claim_window';

/** Serde-tagged on `state`; `refunded` carries an extra `reason`. */
export type SwapState =
  | { state: Exclude<SwapStateLabel, 'refunded'> }
  | { state: 'refunded'; reason: RefundReason };

export interface SwapEnvelope {
  swap_id: string;
  preimage_hash: string;
  leg_a: LegRef;
  leg_b: LegRef;
  timelocks: SwapTimelocks;
  state: SwapState;
}

export type SwapAdvanceAction =
  | 'publish_leg_a'
  | 'publish_leg_b'
  | 'observe_preimage'
  | 'complete';

export interface DraftSwapRequest {
  swap_id?: string;
  a_refund?: number;
  b_refund?: number;
  safety_margin?: number;
}

// ─── Maker profile (kind-30051) ─────────────────────────────────────────────

export interface MakerPair {
  backend: Backend | string;
  lock: string;
  want: string;
}

export interface MakerProfile {
  /** Convenience field added by the API; not part of the on-relay event. */
  label?: string;
  d_tag: string;
  name: string;
  about?: string;
  contact?: string;
  pairs: MakerPair[];
  fill_latency_secs?: number;
  min_cancel_delay?: number;
  badges: string[];
}

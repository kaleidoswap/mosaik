# Mosaik — Nostr Protocol

Mosaik uses Nostr as the **discovery layer** for its covenant-DEX
platform. The relay is content-restricted to three event kinds; the
chain is always the source of truth.

> **Authoritative encoder:** `mosaik-relay` crate (see
> `crates/mosaik-relay/src/lib.rs`). The shapes below mirror the Rust
> types; treat them as the spec.

## Event kinds

| Kind  | Event           | Addressable | Retention |
|-------|-----------------|:-:|---|
| 30050 | Offer (Tessera / banco) | ✓ | short (default 24h, capped by `expiry`) |
| 30051 | Maker profile           | ✓ | long (default 7d) |
| 30052 | Cross-chain swap envelope | ✓ | medium (mirrors `b_refund`) |
| 30053 | P2P on-chain RGB↔BTC swap offer | ✓ | short (the offer's own expiry, max 1d) |
| 30054 | Maker reputation (signed by the Mosaik indexer) | ✓ | NIP-40 `expiration`, 2d after signing |
| 30055 | Curated asset list (signed by the list key) | ✓ | long (until replaced) |
| 1059  | NIP-59 gift wrap (private swap negotiation) | — | short |
| 5     | Deletion (NIP-09)       | — | indefinite |

All of them are whitelisted in `services/mosaik-relay-server/config.toml`.
Anything else is rejected at admission.

### Who publishes what

| Trigger | Source | Kind |
|---|---|---|
| Maker funds a Liquid Tessera | `POST /api/make-offer` (default `backend=liquid`) or `mosaik make-offer` CLI | 30050 |
| Maker funds a banco offer    | `POST /api/make-offer` with `backend=banco`, or `mosaik make-offer --backend banco` | 30050 |
| Maker updates their profile  | `POST /api/maker-profiles` | 30051 |
| Swap is drafted              | `POST /api/swaps/draft` | 30052 |
| Swap advances state          | `POST /api/swaps/:id/advance` | 30052 (replaces previous) |
| Swap is refunded             | `POST /api/swaps/:id/refund` | 30052 (replaces previous) |
| Offer cancel / swap cancel   | NIP-09 kind-5 deletion (clients only — on-chain is the real cancel-of-record) | 5 |

`mosaik-api` signs every outbound event with a **stable per-process
identity** (`Keys`, logged at boot). That makes NIP-33 addressable
replacement work cleanly: a swap that advances `drafted → settled`
shows up as one row in the relay (latest state wins), not five rows
from five throwaway pubkeys. Production deployments should seed the
identity from a persisted nsec instead of generating fresh per boot.

`MOSAIK_RELAY_URL` configures the publish target (default
`ws://127.0.0.1:7777`). The connection is best-effort; if the relay
isn't up at boot, nostr-sdk's pool retries transparently and publishes
queue. The API serves regardless.

## Kind 30050 — offer

A published, funded covenant offer. The event `content` is the full
offer as JSON; tags carry the filterable fields so relays / clients can
query by asset, backend, or order id without parsing the body.

### `content` (JSON)

```jsonc
{
  "order_id": "mosaik-mm-a-7",            // matches the `d` tag
  "offer": {
    "outpoint": "<txid>:<vout>",          // Liquid outpoint OR Arkade funding
    "asset_a":  "<asset id or 'BTC'>",    // what the maker locks
    "amount_a": 1000000,                  // raw units
    "kind": {                             // backend-tagged union
      "backend": "liquid",                // or "banco"

      // ── when backend == "liquid": ──
      "asset_b":        "<32-byte hex, internal order>",
      "amount_b":       500000,
      "maker_spk_hash": "<32-byte hex>",
      "timeout":        500               // Liquid block height

      // ── when backend == "banco": ──
      // "offer_hex":      "<TLV-encoded banco offer>",
      // "swap_pk_script": "<hex>",
      // "want_asset":     "<asset id>" | null,
      // "want_amount":    500000,
      // "cancel_delay":   1779737020,    // unix seconds
      // "partial_fill":   true | false
    },
    "maker_address": "<address the counter-payment must reach>"
  },
  "expiry": 9999999999,                   // unix seconds; stale after this
  "status": "active" | "cancelled"
}
```

The discriminator `kind.backend` is serialised via
`#[serde(tag = "backend", rename_all = "lowercase")]` on
`OfferKind`. New backends extend the enum without breaking older
clients (they can still parse and display the envelope; the backend
field tells them whether they understand the body).

### Tags

| Tag | Value | Purpose |
|---|---|---|
| `d` | `order_id` | Addressable identifier — re-publishing under the same `d` tag replaces the offer. |
| `status` | `active` \| `cancelled` | Lifecycle. |
| `backend` | `liquid` \| `banco` | Settlement backend label. |
| `asset_from` | offer's `asset_a` | What the maker sells. |
| `asset_to` | for Liquid: hex of `asset_b`; for banco: `want_asset` or `"BTC"`. | What the maker wants. |
| `covenant_utxo` | `offer.outpoint` | Funding reference (Liquid outpoint or Arkade funding txid:vout). |
| `expiry` | unix seconds | Mirrors the body field so relays can filter without parsing. |

### Verification by takers

Tags are advisory; content is authoritative; **the chain is final**. A
correct taker MUST re-derive the covenant address (Liquid Tessera) or
re-read the funding tx (Arkade banco) and confirm the spendable
state on-chain **before** submitting a fill. A stale or forged offer
event simply fails verification.

## Kind 30051 — maker profile

Self-attested reputation surface. One profile per maker pubkey,
addressable so a maker can update by re-publishing under the same `d`
tag (convention: `"mosaik-maker"`).

### `content` (JSON)

```jsonc
{
  "d_tag":           "mosaik-maker",
  "name":            "Helix MM",
  "about":           "Liquid + Arkade liquidity, 24/5",   // optional
  "contact":         "helix@example.com",                  // optional
  "pairs": [
    { "backend": "liquid", "lock": "BTC",  "want": "USDT" },
    { "backend": "banco",  "lock": "BTC",  "want": "USDT" }
  ],
  "fill_latency_secs": 45,                                 // optional
  "min_cancel_delay":  120,                                // optional
  "badges": ["audited"]                                    // optional
}
```

### Tags

| Tag | Value | Purpose |
|---|---|---|
| `d` | `d_tag` (typically `mosaik-maker`) | Addressable identifier. |
| `name` | profile `name` | Surface in clients. |
| `contact` | profile `contact` | Optional. |
| `pair` | one tag per pair, value `<backend>:<lock>/<want>` | Lets relays / clients filter to "makers active in BTC/USDT on banco". |
| `backend` | one tag per distinct backend the maker handles | Lets clients filter by venue. |
| `badge` | one tag per badge | Self-attested; future phases may cross-check. |

### Reputation, today vs. tomorrow

All fields are **self-attested**. Mosaik's first principle is
on-chain verification of the covenant, not socially-attested
reputation. The profile carries enough hints to **route** a taker (who
to ask first when several makers post the same pair), not to **trust**
them in any meaningful sense.

A later phase may cross-check `badges` against signed attestations
from a separate Nostr layer (kind to be assigned). Until then, treat
badges the same way you treat the description on an exchange's About
page.

## Kind 30052 — cross-chain swap envelope

A bridge swap pairs **two single-leg offers on two different backends**
under a shared SHA-256 preimage commitment. The envelope at kind 30052
is the relay-side announcement; the encoder lives in `mosaik-bridge`
(see `crates/mosaik-bridge/src/relay.rs`) so the relay crate stays free
of bridge-specific types. See `docs/CROSS_CHAIN_SWAP.md` for the
atomicity proof, the timelock-asymmetry math, and the state machine.

### `content` (JSON)

```jsonc
{
  "swap_id":       "mosaik-swap-001",        // matches the `d` tag
  "preimage_hash": "<32-byte hex>",          // SHA256(preimage); never the preimage
  "leg_a": {                                  // longer-refund leg
    "backend":  "liquid",
    "outpoint": "<txid>:<vout>",              // empty until funded
    "event_id": "<kind-30050 event id>"       // optional
  },
  "leg_b": {                                  // shorter-refund leg
    "backend":  "banco",
    "outpoint": "<txid>:<vout>",
    "event_id": "<kind-30050 event id>"
  },
  "timelocks": {
    "a_refund":      1779741208,              // unix seconds
    "b_refund":      1779737608,
    "safety_margin": 1800                     // a_refund - b_refund must be >=
  },
  "state": {
    "state": "drafted" | "half_published" | "both_published"
           | "revealed" | "settled" | "refunded",
    "reason": "never_published" | "partner_never_published"
            | "no_taker_arrived" | "maker_missed_claim_window"   // only when state == refunded
  },
  "leg_a_offer": { ... },                     // optional full snapshot of the kind-30050
  "leg_b_offer": { ... }                      // body; lets clients skip the lookup
}
```

Validation enforced both at parse time and at publish time
(`SwapEnvelope::validate`):

- `leg_a.backend != leg_b.backend` (two distinct chains are the point).
- `b_refund + safety_margin <= a_refund` (the asymmetry constraint).
- `leg_a.outpoint != leg_b.outpoint` once both legs are funded.

### Tags

| Tag | Value | Purpose |
|---|---|---|
| `d` | `swap_id` | Addressable identifier; re-publishing advances state. |
| `state` | the state's snake-case label | Lifecycle, mirrors the body. |
| `backend` | one tag per leg backend | Lets clients filter on "swaps involving liquid". |
| `pair` | `<backend_a>:<outpoint_a>↔<backend_b>:<outpoint_b>` | Groups swaps by venue pair. |
| `hashlock` | `preimage_hash` hex | Surfaces commitment reuse across announcements. |
| `a_refund` / `b_refund` | unix seconds | Filterability for near-expiry watch jobs. |

### Lifecycle

The maker re-publishes under the same `swap_id` to advance the state
field. Clients should treat the most recent kind-30052 event per
`(pubkey, swap_id)` as authoritative — relays already enforce that via
NIP-33 replacement semantics for addressable events.

A **kind 5 deletion** of a kind-30052 event is *not* a cancellation of
the on-chain legs — those refund via their own covenants. Kind 5 here
is a courtesy to clients so they drop the announcement from view; the
single-leg `Offer` events the envelope references have their own
kind-5 cancels.

### Primitive gaps

The kind-30052 envelope is fully exercisable today (parse, validate,
publish, sign, verify, advance state via the mock orchestrator). The
**on-chain enforcement** of the preimage-bound covenant on each leg
depends on two primitive extensions documented in
`CROSS_CHAIN_SWAP.md` §"Primitive gaps" — until both land, the
`mosaik bridge-*` CLI surfaces clean error messages pointing at the
spec rather than pretending to settle.

### RealOrchestrator runway

`crates/mosaik-bridge/src/real.rs` already wires the swap state
machine against real backend clients (`ElementsRpc` for Liquid,
`BancoClient` for banco). Its `draft` and `refund` paths work today;
the `publish_leg_*` / `await_preimage` / `complete` methods return
`BridgeError::PrimitiveGap` with the spec URL in the message. The
day either primitive lands, the corresponding `PrimitiveGap` site
becomes a concrete call — the trait shape, the envelope, and the
error vocabulary stay unchanged.

## Kind 30053 — P2P on-chain RGB↔BTC swap offer

Published by the `rgb-p2p-swaps` crate (`kaleidoswap/rgb-onchain-swaps`, `p2p/`). Unlike a
Tessera offer this is an **intent, not a funded covenant**: the swap is one Bitcoin transaction
built by rgb-lib's async on-chain swap (offer → request → proposal → completion), the maker signs
last and is the only broadcaster, and the chain is the only source of truth.

- `d` tag: the rgb-lib swap id (also `content.offer.swapId`; a mismatch is rejected).
- Tags: `t=rgb-psbt-v1`, `status` (`active|taken|cancelled`), `network`, `side` (`sell`: the maker
  sells the RGB asset), `asset`, `amount`, `price` (sats, whole BTC leg), `expiry`, NIP-40 `expiration`.
- `content`: `{ "offer": <rgb-lib OnchainSwapOffer>, "status": …, "asset": { ticker, name, precision } | null }`.
  `asset` is self-attested display data; the asset id is the only identity.
- An expiry is required. Updates and status changes reuse the `d` tag; cancellation is a
  `cancelled` replacement plus a NIP-09 deletion.
- The platform fee is part of the rgb-lib offer (`platformFeeSat`, `feeRecipientScriptPubkeyHex`)
  and is paid inside the swap transaction by the taker.

The swap messages (`request`, `proposal`, `completion`, `broadcast`, `abort`) are JSON carried in
NIP-17 private messages (kind 1059 gift wraps), so the relay never sees PSBTs.

### What is public about a trade

The order's `status` goes `active` → `taken` when the maker accepts a request and builds the PSBT
(p2pd `on_request`), *before* anything is signed. After that:

- the negotiation finishes: the maker broadcasts, and the order stays `taken` (nothing more is
  published; the event lapses at its NIP-40 expiration);
- it does not (taker abort, proposal timeout, rejected completion, maker restart): the maker
  publishes `cancelled` for that `d`, deletes it (NIP-09) and lists the same terms under a new `d`.

**The swap txid is not in any public event.** It travels only in the private `broadcast` message.
It can still be found from public data: the offer content carries a fresh per-order receive script
(`makerBtcAddress` for `side=sell`, `makerRgbScriptPubkeyHex` for `side=buy`) and the fee terms,
and rgb-lib builds every swap transaction with the same layout (see
`services/mosaik-indexer/src/settlement.ts`). A confirmed transaction that pays that script with
that layout, and whose fee is exactly `networkFeeSat`, is the order's settlement. The Mosaik indexer
verifies fills this way.

**Proposed p2pd change (not implemented):** after `broadcast_swap_completion` succeeds in
`Maker::drive_completion`, republish the order (status still `taken`, so existing clients are
unaffected) with one extra tag `["txid", "<64 hex>"]`. One `republish` call plus the tag in
`P2pOffer::to_event_builder`. The indexer already reads the tag; it fetches that transaction and
checks it against the same layout, so a wrong txid never counts.

## Kind 30054 — maker reputation

Published by the Mosaik indexer (`services/mosaik-indexer/`), one event per maker pubkey, replaced
on every scoring run (default every 10 minutes). The numbers are measured from public data only:
kind-30053 events on the relay and transactions on Esplora. The maker's own profile (kind 30051) is
never an input.

Only events signed by the published indexer key count. Clients pin that key (see the indexer README
for the dev key and the open decision on the production key).

### Tags

| Tag | Value | Notes |
|---|---|---|
| `d` | maker pubkey (64 hex) | One score per maker. |
| `p` | maker pubkey (64 hex) | Indexed: `{"kinds":[30054],"authors":[<indexer>],"#p":[<maker>]}` fetches one maker. |
| `t` | `mosaik-reputation-v1` | Schema version. |
| `network` | `signetcustom` | Mutinynet only for now. |
| `window` | `2592000` | Seconds the numbers cover (30 days, ending at `created_at`). |
| `settled` | integer | Settled trades with a distinct counterparty (self-trades and unattributed trades excluded). |
| `volume_sat` | integer | Sum of the BTC leg (`price`) of those trades. |
| `counterparties` | integer | Distinct counterparties among them. |
| `self_trades` | integer | Settled trades whose taker the chain links to the maker. |
| `negotiations` | integer | Orders seen `taken`, or found settled, in the window. |
| `aborted` | integer | Negotiations that did not finish: `taken` then `cancelled` (or `active`), or `taken` with no settlement within 48 h. |
| `abort_rate` | `0.0000`–`1.0000` | `aborted / (aborted + every confirmed settlement)`, i.e. over resolved negotiations. Absent when there are none. |
| `presence` | `0.0000`–`1.0000` | Share of 5-minute slots, while the indexer was watching, in which it received a fresh 30053 event from the maker (recorded at receive time; backdated events do not count). Absent when the indexer watched no slot. |
| `verified` | `1` \| `0` | All badge thresholds met (thresholds are in the content; provisional). |
| `expiration` | unix seconds | NIP-40, `created_at + 172800`: a score the indexer stops refreshing disappears. |

`settled` in the tags counts only trades with a distinct counterparty; the content's
`negotiations.settled` counts every confirmed settlement.

### `content` (JSON)

```jsonc
{
  "v": 1,
  "network": "signetcustom",
  "computedAt": 1794520000,
  "maker": "<64 hex>",
  "window": { "from": 1791928000, "to": 1794520000 },
  "firstSeen": 1794400000,                   // first kind-30053 event of this maker the indexer saw
  "presence": { "ratio": 0.9731, "presentBuckets": 1401, "observedBuckets": 1440, "bucketSecs": 300 },
  "negotiations": { "taken": 6, "settled": 4, "aborted": 1, "unsettled": 0, "pending": 1, "abortRate": 0.2 },
  "volume": {
    "sat": 120000, "trades": 3, "counterparties": 2, "selfTrades": 1, "unattributedTrades": 0,
    "byAsset": { "rgb:…": { "trades": 3, "amount": 300, "sat": 120000, "counterparties": 2 } }   // amount in base units; counterparties distinct within the asset
  },
  "verified": true,
  "thresholds": { "minSettled": 3, "minCounterparties": 2, "minPresence": 0.9, "maxAbortRate": 0.2 },
  "settlements": [                           // newest first, at most 50
    { "orderId": "…", "txid": "<64 hex>", "height": 3584542, "at": 1794510000, "side": "sell",
      "assetId": "rgb:…", "amount": 50, "priceSat": 27148, "self": false, "unattributed": false }
  ]
}
```

`aborted` in the tags is `negotiations.aborted + negotiations.unsettled`. A settlement transaction is credited to
one order only: the one whose offer the indexer received first, dated before the transaction
confirmed; copies of another maker's offer earn nothing. Every `txid` in
`settlements` can be checked against Esplora by anyone.

### Verifying (clients)

1. `kind == 30054`, `pubkey ==` the pinned indexer key, valid id and signature.
2. `t == mosaik-reputation-v1`, `d == p ==` the maker, `expiration` in the future.
3. Tag numbers equal the content (`settled == volume.trades`, `volume_sat == volume.sat`,
   `verified == content.verified`).

`verifyReputationEvent` in `services/mosaik-indexer/src/events.ts` is the reference and depends
only on nostr-tools.

## Kind 30055 — curated asset list

The Kaleido curated tier, as a signed list. The content is the curated catalog, format
`mosaik-catalog/v1` (today `webapp/public/.well-known/rgb/tokens.json`), so a client verifies the
signature and then reads the same JSON it reads from the well-known file. Re-signed only when the
catalog changes, so `created_at` is when the list last changed.

### Tags

| Tag | Value | Notes |
|---|---|---|
| `d` | `mosaik-catalog:<network>` | One list per network: `mosaik-catalog:signetcustom`. |
| `t` | `mosaik-catalog-v1` | |
| `network` | `signetcustom` | Must equal `content.network`. |
| `version` | catalog `version` | Must equal `content.version`. |
| `asset` | `<contract_id>`, `<ticker>` | One per listed token, same set as `content.tokens`. |
| `issuer_maker` | `<contract_id>`, `<maker pubkey>` | Optional, one per pair: a maker key run by (or for) the asset's issuer, for the issuer-is-maker label. |

### `content`

The catalog JSON (`mosaik-catalog/v1`): `format`, `version`, `network`, `network_id`, `provider`,
and `tokens[]` with `contract_id`, `ticker`, `name`, `precision`, plus optional fields
(`description`, `issued_supply`, `demo`, …). One optional field is added by this list:

- `tokens[].issuer_makers`: array of maker pubkeys (64 hex) known to be the issuer or acting for it.

Rules the signer enforces before signing (`parseCatalog`): contract ids unique, tickers unique
(case-insensitive, so the list never merges two assets under one name), precision 0–18.

### Verifying (clients)

`kind == 30055`, `pubkey ==` the pinned list key, valid signature, `d`/`t`/`network`/`version` as
above, and the `asset` tags equal the content's tokens. Reference: `verifyCatalogEvent` in
`services/mosaik-indexer/src/events.ts`. Today the indexer key also signs the list; which key
signs it in production is an open decision.

## Kind 5 — deletion (NIP-09)

Standard NIP-09. A maker cancels a published offer by emitting a
kind-5 event tagging the original offer's event id. Our relay accepts
kind 5 alongside the two Mosaik kinds; the cancel is enforced by
clients dropping the original from view. The on-chain covenant is the
real cancel-of-record (`reclaim` / banco `cancel`); kind 5 is a
courtesy to clients so they don't display a stale offer.

## Versioning

The schemas above are **v1**. Breaking changes bump the schema version
via a new `v` tag on the event:

```
["v", "1"]
```

A relay running a newer schema validator may reject `v=0` events; a
relay running an older validator should ignore unknown tags. Field
additions inside `content` that don't change existing field meanings
do **not** bump the version (the JSON is forward-compatible by
construction).

## Federation

A Mosaik relay can federate by **subscribing** to peer Mosaik relays
and republishing the kinds it cares about. `nostr-rs-relay` doesn't
implement active pulling itself; see
[`services/mosaik-relay-server/federation/`](../services/mosaik-relay-server/federation/)
for the recommended sidecar pattern and a reference implementation.

The relay is otherwise content-agnostic: any standard Nostr client
that knows about kind 30050/30051 can read and write to it.

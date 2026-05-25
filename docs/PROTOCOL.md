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
| 5     | Deletion (NIP-09)       | — | indefinite |

All three are whitelisted in `services/mosaik-relay-server/config.toml`.
Anything else is rejected at admission.

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

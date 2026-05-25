# Plan: Mosaik on Arkade via banco (replacing the VHTLC backend)

## Context

Mosaik shipped two settlement backends:

- **Liquid** — Tessera: a single UTXO governed by a keyless Simplicity
  covenant. Maker can publish and go offline; anyone can fulfil.
- **Arkade (VHTLC)** — two preimage-linked VHTLCs with asymmetric timelocks.
  Atomic, but **interactive** and the maker must be online to claim.

The VHTLC backend was chosen because we believed Arkade had no covenant
primitive. That turned out to be wrong: **`@arkade-os/banco`** implements a
covenant-based non-interactive swap on top of Arkade Script (`FINDASSETGROUPBYASSETID`,
`INSPECTOUTASSETLOOKUP` opcodes), which is structurally **the same shape as
Tessera** — single covenant VTXO, maker offline, taker fulfils in one tx.

This plan pivots the Arkade backend from VHTLCs to banco, runs the stack on
**Mutinynet**, and unifies Liquid + Arkade(banco) behind one Mosaik interface.

## Scope decisions

1. **Replace** `mosaik-arkade` (VHTLC) with `mosaik-banco`. The VHTLC code is
   thrown away — banco is strictly better for our use case and there's no
   reason to maintain both Arkade variants.
2. **Network**: Mutinynet (custom signet, ~30s blocks). Mainnet is a future step.
3. **Same `OfferKind` enum** in `mosaik-core` — banco slots in as a third
   variant; the existing `Offer`/`MakeOffer`/`TakeOffer` shape carries it.
4. **No Rust banco SDK exists** — we'll run a small `banco-proxy` TS
   microservice (same pattern as `ark-proxy`), called over HTTP from Rust.

## The stack to run on Mutinynet

No turnkey compose exists. We assemble it from published images + a build of
`bancod` and a new `banco-proxy`:

```
  bitcoind (signet/mutinynet)   ←  needs --signet --signetchallenge=... or use mutinynet image
    │
    ↓ RPC
  nbxplorer  (nicolasdorier/nbxplorer:2.6.7)        port 32838
    │
    ↓
  arkd-wallet  (ghcr.io/arkade-os/arkd-wallet)      port 6060
    ↓                                                ARKD_WALLET_NETWORK=mutinynet
  arkd        (ghcr.io/arkade-os/arkd)              ports 7070 (gRPC) / 7071 (admin)
    │                                                ARKD_ESPLORA_URL=https://mutinynet.com/api
    ↓
  introspector (ghcr.io/arklabshq/introspector:v0.0.1)  port 7073
    │                                                    INTROSPECTOR_ARKD_URL=arkd:7070
    ↓
  bancod       (built locally from arkade-os/bancod)     ports 7070 (gRPC) / 7071 (HTTP+UI)
    │                                                    optional — auto-taker bot
    ↓
  banco-proxy  (NEW — TS microservice, this repo)        port 3001
    │                                                    wraps @arkade-os/banco Maker/Taker
    ↓
  mosaik       (Rust CLI / webapp)
```

**To create:** `mosaik/docker/banco-mutinynet/`
- `compose.yaml` wiring nbxplorer + arkd-wallet + arkd + introspector + banco-proxy.
  Choose: either embed a mutinynet bitcoind container or point nbxplorer at the
  public mutinynet node (`https://mutinynet.com`). Public RPC is rate-limited;
  for development run our own bitcoind signet with mutinynet's challenge.
- `.env.example` — `ARKD_WALLET_NETWORK=mutinynet`, esplora URL, NBX creds,
  introspector secret, wallet seeds.
- `README.md` — bring-up sequence (init wallets, fund maker/taker on mutinynet
  faucet, sanity checks).

**Dockerize `bancod`:** the repo has a `Dockerfile` but no published image.
- File our own `mosaik/docker/banco-mutinynet/bancod.Dockerfile` that pulls
  `arkade-os/bancod` at a pinned commit and builds. Optionally PR an image
  publish workflow back upstream once we're stable.

## `banco-proxy` (new TS microservice)

Mirror what `ark-proxy` does for the SDK. Live at
`/Users/walter/Lavoro/Kaleidoswap/banco-proxy/` (new repo, not in mosaik).

Endpoints, all JWT-authed like ark-proxy:

```
POST /api/v1/auth/token           { walletId } → { token }
GET  /api/v1/banco/identity       → { walletPubkey, arkServerPubkey, introspectorPubkey }
POST /api/v1/banco/maker/offer    { wantAmount, wantAsset?, cancelDelay?, exitTimelock?,
                                     ratioNum?, ratioDen? }
                                  → { offerHex, swapPkScript, packetHex, fundingTxid? }
POST /api/v1/banco/maker/cancel   { offerHex } → { txid }
GET  /api/v1/banco/maker/offers?swapPkScript=…  → [{ txid, vout, value, assets, spendable }]
POST /api/v1/banco/taker/fulfill  { offerHex, fillAmount? }   → { txid }
POST /api/v1/banco/taker/fulfill-by-txid { txid, fillAmount? } → { txid }
GET  /api/v1/banco/offers/stream  (SSE) — wraps arkd's tx stream, emits new banco offers
```

**Pinning concern:** `@arkade-os/banco` v0.0.1 depends on a personal fork
`github:louisinger/wallet-sdk#arkade-script-final`. Pin both at a known-working
commit in `banco-proxy`'s `package.json`; revisit when banco hits a real release.

## `mosaik-banco` (replaces `mosaik-arkade`)

Crate at `mosaik/crates/mosaik-banco/`. Drops the VHTLC client, modelled on the
same shape so the CLI integration stays small.

```rust
pub struct BancoClient { /* ureq HTTP to banco-proxy */ }
impl BancoClient {
    pub fn identity(&self) -> Result<Identity>;
    pub fn create_offer(&self, p: &OfferParams) -> Result<BuiltOffer>;
    pub fn fulfill(&self, offer_hex: &str, fill: Option<u64>) -> Result<String>;
    pub fn fulfill_by_txid(&self, txid: &str, fill: Option<u64>) -> Result<String>;
    pub fn cancel(&self, offer_hex: &str) -> Result<String>;
    pub fn list_maker_offers(&self, swap_pk_script: &str) -> Result<Vec<MakerOffer>>;
}

pub struct BancoMaker { client: BancoClient }
impl BancoMaker {
    pub fn make_offer(&self, terms: …) -> Result<Offer>;   // no preimage, no leg-B dance
    pub fn cancel(&self, offer: &Offer) -> Result<String>;
}

pub struct BancoTaker { client: BancoClient }
impl BancoTaker {
    pub fn take_offer(&self, offer: &Offer) -> Result<String>;   // single-call fulfilment
}
```

Drop `Preimage`, `SwapDelays`, `wait_for_preimage`, `claim`, `ArkadeSettle` —
unnecessary. The taker call is one HTTP roundtrip; the trait shape goes back
to `MakeOffer`/`TakeOffer`/`ReclaimOffer` (cancel ≈ reclaim).

In `mosaik-core`: replace `VhtlcOffer` with `BancoOffer`:

```rust
#[serde(tag = "backend", rename_all = "lowercase")]
pub enum OfferKind {
    Liquid(Tessera),
    Banco(BancoOffer),
}

pub struct BancoOffer {
    pub offer_hex: String,         // the TLV-encoded banco offer
    pub swap_pk_script: String,    // covenant VTXO scriptPubKey
    pub funding_txid: String,      // arkd tx that funded + embedded the offer packet
    pub want_asset: Option<String>, // None = BTC
    pub want_amount: u64,
    pub cancel_delay: u64,         // unix ts
    pub partial_fill: bool,
}
```

## CLI (back to three commands)

Liquid keeps the existing flags; Arkade(banco) loses the multi-step dance:

```
mosaik make-offer --backend banco --amount-a … --asset-a … --amount-b … --asset-b BTC|<id>
mosaik take-offer --backend banco --offer offer.json
mosaik reclaim    --backend banco --offer offer.json   # cancel after cancel_delay
```

Delete `arkade-settle` and `swap-status` — no longer needed.

## Discovery: arkd's tx stream IS the orderbook

A banco offer is published as an Arkade extension packet (type `0x03`) in the
funding tx's OP_RETURN. `bancod` already subscribes to arkd's tx stream to
auto-fulfil; we expose the same firehose to clients via
`banco-proxy /banco/offers/stream` (SSE).

Implication: the Nostr relay (`mosaik-relay`) becomes **optional** on the
Arkade side — `browse-offers` can also pull from banco-proxy. We keep Nostr
because:
- It carries richer metadata (price band, expiry, maker handle, signed reviews).
- It unifies Liquid + Arkade in one stream.
- It works across multiple arkd instances without on-chain enumeration.

So the relay model becomes: **chain-of-truth on each backend, plus an
optional global Nostr aggregator for discovery and metadata.**

## Verification

1. `cd mosaik/docker/banco-mutinynet && docker compose up` brings up the full
   stack on mutinynet.
2. `scripts/banco-bootstrap.sh` — initialises arkd-wallet, creates `maker`
   and `taker` wallets in banco-proxy, prints faucet addresses.
3. Fund both from https://mutinynet.com/faucet.
4. `scripts/banco-e2e.sh` (replaces `arkade-e2e.sh`): make-offer → take-offer
   in two CLI calls; assert both final balances.
5. Cancel path: make-offer, wait past `cancel_delay`, reclaim, assert refund.
6. Regression: existing Liquid test suite untouched and passing.

## Open risks

1. **banco SDK churn**: v0.0.1, fork dependency. Pin hard, expect to rebase.
2. **No published bancod image**: we build it; upstream PR for a release workflow.
3. **Introspector trust model**: the introspector is a liveness cosigner —
   can't steal, but can censor. Run our own (the docker image is public).
4. **Mutinynet bitcoind**: public RPC is rate-limited; running our own
   signet+mutinynet bitcoind in compose adds operational weight. Acceptable
   for dev, revisit for prod.
5. **Asset issuance on Arkade**: docs are thin (`pkg/ark-lib/asset/`). For
   asset↔asset swaps we need to know how the maker mints/holds the
   want-asset. Spike this before promising asset-pair UX.

## Critical files to create

- `mosaik/docker/banco-mutinynet/{compose.yaml, .env.example, README.md, bancod.Dockerfile}`
- `mosaik/crates/mosaik-banco/{Cargo.toml, src/lib.rs, src/client.rs}`
- `mosaik/scripts/{banco-bootstrap.sh, banco-e2e.sh}`
- New repo (sibling of mosaik): `banco-proxy/` — TS microservice

## Critical files to modify

- `mosaik/Cargo.toml` — swap `mosaik-arkade` → `mosaik-banco` in members
- `mosaik/crates/mosaik-core/src/lib.rs` — `VhtlcOffer` → `BancoOffer`,
  `OfferKind::Arkade` → `OfferKind::Banco`
- `mosaik/crates/mosaik-relay/src/lib.rs` — backend tag values
- `mosaik/crates/mosaik-cli/src/main.rs` — Arkade subcommands collapse back
  into the three canonical ones
- Delete `mosaik/crates/mosaik-arkade/`, `mosaik/scripts/arkade-e2e.sh`

---

# Where this evolves

The interesting thing about pivoting to banco is that Mosaik becomes
**one product spanning two real covenant DEXes** — same UX, same orderbook,
same maker/taker mental model, different chains. From there:

### Near-term (months)

- **Cross-backend price aggregation.** Nostr relay carries offers from both
  Liquid Tesseras and Arkade banco UTXOs. A taker sees one merged book and
  picks the venue with the best fill. Same `Offer`, same web UI.
- **Asset support on Arkade.** Once Arkade asset issuance stabilises, the
  same Tessera asset/asset trades work on Arkade — USDT↔BTC, stable↔stable.
  Mutinynet is a fine playground for this before mainnet.
- **A `bancod` watcher as Mosaik's own market-maker.** The bancod daemon
  already auto-fulfils offers in configured price ranges. Run one alongside
  Mosaik and the project ships with a reference market-maker bot.

### Medium-term (quarters)

- **Cross-chain atomic swap: Liquid-Tessera ↔ Arkade-banco.** Both are
  keyless covenants on UTXO-style chains. With a shared preimage commitment
  on both sides (Liquid Simplicity easily expresses HASH160(preimage)
  checks; banco can be extended with a preimage tap leaf or use the existing
  `pkg/preimage` plugin) you get atomic L-BTC ↔ Arkade-BTC without an LSP,
  without an oracle. This is the real product — a covenant-only bridge.
- **Permissionless market-maker registry on Nostr.** Makers publish signed
  maker profiles (kind 30051?) with their supported pairs, fill speeds,
  cancel-delays, reputation attestations. Takers route on reputation, not
  just price. Mosaik becomes an aggregator like 1inch but for covenant L2s.
- **Webapp as the user surface, CLI as the trust anchor.** The existing
  vanilla-JS webapp grows into a hosted-but-self-custodial frontend; the
  Rust CLI remains the "verify everything yourself" path. Tessera/banco
  source + CMR/script-hex shown next to every order.

### Longer-term (year+)

- **Bitcoin L1 covenant DEX once CTV/CSFS/CAT ship.** The Mosaik abstraction
  already speaks "single-UTXO covenant offer." When Bitcoin gets enough
  introspection, a third backend slots into `OfferKind::Bitcoin(Tessera)`
  without changing the relay, the UI, or the CLI. Mosaik becomes the
  reference covenant-DEX UX across every UTXO chain that gains the
  primitive — Liquid first, Arkade today, Bitcoin when it lands.
- **RGB integration via banco-style covenants.** If RGB ever standardises
  covenant-equivalent enforcement at the asset-transition layer (AluVM is
  the path), the same maker-publishes-then-disappears model works for RGB
  assets — and KaleidoSwap's HTLC product gains a non-interactive sibling.
- **A "covenant DEX standard".** The kind-30050 Nostr offer event, the
  `Offer`/`OfferKind` data model, the "taker re-derives the address before
  filling" verification flow — these generalise. Publish them as a small spec
  and you've defined what a credibly-neutral self-custodial spot DEX looks
  like across L2s.

The pivot to banco is the moment Mosaik stops being a Simplicity demo and
starts being a covenant-DEX platform.

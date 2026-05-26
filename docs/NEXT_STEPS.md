# Mosaik — Next Steps

**Companion to** `docs/PLATFORM_PLAN.md` (architecture, phases 1-8 — landed)
and `docs/CROSS_CHAIN_SWAP.md` (the Phase-8 design + primitive gaps).

The eight architecture phases are committed on `feat/covenant-dex-platform`.
This document is the *execution* backlog from here: what to build, in what
order, and what each step unblocks.

## Where we are (as of branch tip `295ca00`)

| | |
|---|---|
| Rust workspace | 6 crates · 18 test suites · fmt + clippy(-D warnings) + test all clean |
| Services | mosaik-api · mosaik-webapp · mosaik-relay-server · banco-proxy · banco-proxy/federation |
| Docker | base + Arkade-Mutinynet overlays validate; mosaik-api image builds |
| Banco SDK | wired against real `@arkade-os/banco`; daemon starts; install self-patches upstream's broken ESM |
| Relay protocol | kinds 30050 (offer) / 30051 (maker profile) / 30052 (cross-chain swap envelope) all encoded + tested |
| Cross-chain bridge | full state machine, types, CLI subcommands, kind-30052 codec; on-chain primitives still TODO |
| CI / release | `ci.yaml` runs hard gates · `release.yaml` ships images + binaries on `v*` tag — neither has fired yet |

Branch is 13 commits ahead of `main`, not yet pushed.

## Three tracks

The remaining work splits cleanly into **deploy & validate**, **product UX**,
and **cryptographic primitives**. Each track can move independently;
recommended order at the end of this doc.

---

### Track 1 — Deploy & validate

What's built is correct as far as we've exercised it locally; the next
unknowns surface only when the platform meets a real chain.

#### 1.1 Push branch + PR (½ day)
- `git push origin feat/covenant-dex-platform`
- Open a PR against `main` so `ci.yaml` actually fires (first run will
  surface anything the local mirror missed — Linux-specific paths,
  cache keys, action versions).
- Address whatever the CI surfaces; don't merge until green.

#### 1.2 Tag `v0.2.0-platform` (½ day)
- After merge: `git tag -a v0.2.0-platform -m "platform-release: covenant DEX abstraction"`
- Push; `release.yaml` fires.
- Validates the multi-arch ghcr push (4 images × {amd64, arm64}) and the
  CLI cross-compile (linux-x86_64, macos-aarch64) end-to-end.
- Promotes the draft Release to a public one.

#### 1.3 Real Mutinynet bring-up (1-2 days)
- `cp docker/banco-mutinynet/.env.example docker/banco-mutinynet/.env`
  with real BIP-39 mnemonics + introspector secret key.
- `make build-images && make dev-arkade`.
- Watch each service come up (bitcoind sync → nbxplorer → arkd-wallet →
  arkd → introspector → banco-proxy → mosaik-api → mosaik-webapp).
- Fund a maker + taker wallet via <https://mutinynet.com/faucet>.
- Run `mosaik make-offer --backend banco …` end-to-end; surface every
  operational issue into Issues.

#### 1.4 Nightly e2e workflow (1 day)
- `.github/workflows/e2e-nightly.yaml` cron-fires `00 04 * * *`.
- Bring up the Liquid regtest stack inside the runner.
- Run `scripts/regtest.sh` + a make/take/reclaim cycle through
  `mosaik make-offer / take-offer / reclaim`.
- Assert the trade tape advances and balances move.
- Failure → notify Telegram / Slack.

#### 1.5 Open Issues for everything below (½ day)
- Pull each remaining item out of commit messages into a tracked Issue.
- One label per track (`track/deploy`, `track/ux`, `track/primitive`).
- Link to the relevant design doc section.

---

### Track 2 — Product UX

The protocol surface is in place; the SPA hasn't caught up. Each item
here is a visible-in-the-UI improvement.

#### 2.1 `/api/swaps/*` endpoints (1-2 days) — **unblocks all bridge UI**
- New module in `crates/mosaik-api`: `swap_handlers.rs`.
- Endpoints:
  - `POST /api/swaps/draft` → returns `SwapEnvelope` + (privately) the preimage
  - `GET  /api/swaps` → list in-memory swaps (mirrors offers state pattern)
  - `GET  /api/swaps/:id` → single envelope + current state
  - `POST /api/swaps/:id/advance` → step the MockOrchestrator forward
    (publish-leg-a / publish-leg-b / observe-preimage / complete)
  - `POST /api/swaps/:id/refund` → `RefundReason` body
- All backed by `mosaik_bridge::MockOrchestrator` so the SPA can show
  the **full state-machine walk on mock data** without primitive gaps.
- Replace the orchestrator with the real one once Gap A + B land — the
  HTTP surface doesn't change.

#### 2.2 SwapEnvelope panel in the SPA (1-2 days, depends on 2.1)
- `webapp/src/components/SwapBoard.tsx`: list of in-flight swaps with
  state pills, leg refs, hashlock, timelock countdown.
- `webapp/src/components/SwapDraftForm.tsx`: "draft a cross-chain swap"
  form that calls `POST /api/swaps/draft`.
- Step-the-machine buttons gated on current state.
- This is the most distinctive thing about the platform; today only
  the CLI sees it.

#### 2.3 MakerProfile chip + editor (1 day)
- `MakerProfile` chip rendered next to each offer in `OrderBook.tsx`
  (name + backend pills + fill-latency badge).
- `MakerProfileEditor.tsx` panel: form posting to a new
  `POST /api/maker-profile` (mosaik-api signs and publishes kind 30051).
- Backed by an in-memory profile cache keyed by maker pubkey.

#### 2.4 Banco offers through the API (1-2 days)
- Today: `mosaik make-offer --backend banco` works in the CLI; the
  webapp's `MakeOfferForm` only handles Liquid.
- Add a `backend` field to `POST /api/make-offer`, route to
  `mosaik_banco::BancoMaker::make_offer` when `banco`.
- Same for `/api/take-offer`, `/api/reclaim`.
- Add a configurable `BANCO_PROXY_URL` env to mosaik-api.
- `MakeOfferForm`'s banco mode becomes live; the "use the CLI" hint
  comes out.

#### 2.5 Live Nostr subscription (1 day)
- Replace the SPA's 2-second `/api/state` polling with a WebSocket
  subscription to `mosaik-relay-server` for kinds 30050 / 30051 / 30052.
- `webapp/src/api/relay.ts`: wraps `nostr-tools` SimplePool.
- TanStack Query stays for non-realtime data (wallets, balances).

---

### Track 3 — Cryptographic primitives

This is the on-chain work that turns the bridge from mock to real.

#### 3.1 Gap A — `tessera-hashlock.simf` (1-2 days) — **contained, no external deps**
- Add a third SETTLE branch to `crates/tessera/contracts/tessera.simf`:
  ```
  settle_with_hashlock(vout, preimage):
      require sha256(preimage) == HASHLOCK
      // plus the existing output_amount / output_asset / output_script_hash checks
  ```
- New 32-byte template parameter `HASHLOCK` alongside `asset_b`,
  `amount_b`, `maker_spk_hash`, `timeout`.
- Rust `Tessera` struct gains `Option<[u8;32]>` `hashlock` (None =
  legacy non-bridge offer; covenant binds to a known-null hash so the
  branch is unsatisfiable for legacy).
- Extend the `hal-simplicity` test harness with a hashlock case;
  add `tests/hashlock.rs` covering the happy + wrong-preimage paths.
- Independently useful: hashlocked single-chain Liquid swaps gain a
  preimage gate even without the cross-chain story.

#### 3.2 Gap B — banco preimage-gated fulfilment (3-5 days, gated on Gap A
or a stable upstream banco)
- Upstream: extend `@arkade-os/banco`'s covenant builder with a
  preimage tap leaf. Either local fork + PR, or extend after a stable
  upstream release.
- `services/banco-proxy`: surface `preimageHash` on
  `POST /banco/maker/offer` and pass through.
- New `GET /banco/offers/:txid/preimage` endpoint that extracts the
  revealed preimage from a fulfilment spend (the bridge orchestrator
  watches this).
- `crates/mosaik-banco`: matching client methods.

#### 3.3 Wire the real orchestrator (½ day, gated on 3.1 + 3.2)
- `crates/mosaik-bridge`: new `RealOrchestrator` implementing the
  `BridgeOrchestrator` trait against `mosaik-core` (Liquid) and
  `mosaik-banco` (Arkade).
- Replace the `PrimitiveGap` errors at the documented sites with the
  real on-chain calls.
- `mosaik bridge-make --dry-run=false` becomes the first real
  cross-chain covenant swap on Mosaik.

#### 3.4 First public cross-chain swap on Mutinynet (1 day, gated on 3.3)
- Two real wallets, two backends, single shared preimage, two on-chain
  settlements visible on `mutinynet.com/api` and the arkd indexer.
- Write up + post.

---

## Recommended order

| Order | Item | Why first |
|---:|---|---|
| 1 | **1.1 Push + PR** | Tiny; surfaces CI bugs while context is fresh |
| 2 | **1.3 Mutinynet bring-up** | Highest information value; tells us what real-world stack issues exist before more code lands |
| 3 | **2.1 `/api/swaps/*`** | Unblocks all bridge UX (2.2) and demonstrates the platform's distinctive primitive even before 3.1 / 3.2 land |
| 4 | **2.2 SwapEnvelope panel** | Makes the moat visible in the UI |
| 5 | **3.1 Gap A** | The largest piece of contained real work; no external blockers |
| 6 | **2.4 Banco offers through API** | Closes the UI ↔ CLI feature parity gap |
| 7 | **1.4 Nightly e2e** | After the surface stabilises so the cron isn't fighting churn |
| 8 | **3.2 → 3.3 → 3.4** | The cross-chain swap goes live |

### Quick-win shortlist (one focused week)

If a single week of focused work is on the table, the sequence
**1.1 → 2.1 → 2.2 → 3.1** lands:
- a green CI on every PR,
- a webapp that visualises the full cross-chain swap state machine
  (on mock orchestrator),
- the first half of the real-bridge cryptographic primitive.

That's the most-visible-progress-per-day path.

## Out of scope (deliberate deferrals)

- **bancod taker bot integration.** Reference market-maker; only
  meaningful once 3.3 lands.
- **Federated relay deployment.** Sidecar exists; running peers is an
  ops question, not a build one.
- **Maker reputation cross-attestations.** Phase 7's `badges` field is
  self-attested today; signed attestations need their own kind +
  taxonomy.
- **mosaik-api OpenAPI codegen.** Worth doing once the surface
  stabilises post-2.4; not before.
- **RGB / RLN backend.** A coherent fourth `OfferKind` variant lands
  only if there's a covenant primitive on that stack — see the earlier
  discussion in this repo's history.

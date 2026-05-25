# Mosaik — Industrialization Plan

**Branch:** `feat/industrialize-covenant-dex`
**Companion docs:** `docs/BANCO_INTEGRATION_PLAN.md` (backend pivot details)

## Context

Mosaik began as a Simplicity-covenant hackathon DEX on Liquid: one Rust
workspace, one binary (`mosaik`), one vanilla-HTML webapp baked into the
binary, one settlement backend. The product thesis has since broadened: the
`OfferKind` abstraction now spans **multiple keyless-covenant L2s** (Liquid
today; Arkade via banco next; Bitcoin L1 once it gains covenant opcodes).

This plan industrialises Mosaik into a small, multi-service **covenant-DEX
platform**: real services with their own images, a Vite SPA frontend, a
purpose-built Nostr relay as part of the project (not an external dependency),
the banco backend on Mutinynet, and a docker-compose that brings the whole
thing up reproducibly.

## Goals

- **Pluggable backends.** Liquid + Arkade(banco) wired behind the same
  interface today; Bitcoin L1 / RGB slottable later without surgery.
- **Real services, not a binary with sub-modes.** Webapp, API, relay, and
  banco-proxy are separately deployable containers.
- **One-command bring-up** for each network (regtest, mutinynet, mainnet)
  via docker compose.
- **Federated discovery** via a Mosaik-tuned Nostr relay that also runs
  inside the stack — clients don't depend on third-party relays for
  baseline functionality.
- **Frontend as a real product** — a Vite SPA delivered as an `nginx:alpine`
  image, talking to the mosaik-api over JSON-REST + a Nostr WS subscription
  for live order updates.
- **Industrial hygiene** — CI on every PR, ghcr-published images on tag,
  pinned upstream versions, integration tests against a live mutinynet stack
  in nightly CI.

## Target architecture

```
                          ┌──────────────────────────────┐
                          │ mosaik-webapp (Vite SPA)     │ nginx:alpine
                          │  React + TS, served static   │ :80
                          └─────────────┬────────────────┘
                                        │ JSON-REST + WS
                          ┌─────────────▼────────────────┐
                          │ mosaik-api (Rust, axum)      │ :8080
                          │  thin HTTP over the workspace│
                          │  crates; long-running daemon │
                          └──┬────────────┬────────────┬─┘
                             │            │            │
              ┌──────────────▼──┐  ┌──────▼──────┐  ┌──▼──────────────┐
              │ mosaik-relay    │  │ Liquid path │  │ Arkade path     │
              │ (nostr,         │  │             │  │                 │
              │  curated)       │  │ elementsd   │  │ banco-proxy(TS) │
              │ :7777 WS        │  │ + LWK       │  │   ↓             │
              └─────────────────┘  └─────────────┘  │ arkd / arkd-wlt │
                                                    │ introspector    │
                                                    │ bancod (optional│
                                                    │ MM bot)         │
                                                    │ nbxplorer       │
                                                    │ bitcoind        │
                                                    │  (signet/muti)  │
                                                    └─────────────────┘
```

Every box on the row "mosaik-*" is a service authored by us; every box below
is a pinned upstream image. The relay is **inside** the project so the demo
ships with discovery out of the box and so we can curate the offer schema.

## Monorepo layout (target)

The Rust workspace stays in this repo. Services that need a different
toolchain (TS, frontend) live as sibling top-level dirs:

```
mosaik/
├── crates/
│   ├── tessera/              # Liquid Simplicity covenant (unchanged)
│   ├── mosaik-core/          # Offer, OfferKind enum, traits
│   ├── mosaik-banco/         # NEW — Arkade banco backend (ureq → banco-proxy)
│   ├── mosaik-relay/         # Nostr offer codec + relay client
│   ├── mosaik-api/           # NEW — axum HTTP API binary (was: server.rs in mosaik-cli)
│   └── mosaik-cli/           # CLI binary, slimmer (no embedded webapp)
├── services/
│   ├── banco-proxy/          # NEW — TS microservice wrapping @arkade-os/banco
│   └── mosaik-relay-server/  # NEW — Nostr relay (wraps nostr-rs-relay) with our policy plugin
├── webapp/                   # REPLACE — Vite + React + TS SPA
│   ├── src/
│   ├── index.html
│   ├── package.json
│   └── Dockerfile            # multi-stage: node:22 → nginx:alpine
├── docker/
│   ├── regtest-liquid/       # existing scripts/regtest.sh → compose
│   ├── mutinynet-arkade/     # full Arkade banco stack
│   ├── shared/               # nginx config, healthcheck snippets
│   └── compose.yaml          # top-level: mosaik-api + webapp + relay + per-net overlays
├── scripts/                  # bootstrap, e2e, faucet helpers
├── docs/
│   ├── ARCHITECTURE.md       # NEW — service map + sequence diagrams
│   ├── BANCO_INTEGRATION_PLAN.md
│   ├── INDUSTRIALIZATION_PLAN.md
│   ├── PROTOCOL.md           # NEW — Nostr kind-30050 spec + OfferKind schema
│   └── existing hackathon docs
└── .github/workflows/        # NEW — CI per service
```

## Services in detail

### 1. `mosaik-api` (new Rust crate, replaces the inline `server.rs`)

- **Why split:** the CLI is a developer tool; the webapp needs a stable
  long-running API. Mixing them in one binary makes deploy weird and forces
  the CLI to pull tokio.
- **Framework:** `axum` + `tokio` + `tower-http` (CORS, traces). Reuses the
  workspace crates.
- **Endpoints:** REST over `Offer`/`OfferKind`. Stable shape across backends:
  - `GET  /api/offers?backend=&pair=` — aggregates Liquid + banco
  - `POST /api/offers` — `{ backend, …terms }` → makes an offer on the named backend
  - `POST /api/offers/:id/take` — taker fills it
  - `POST /api/offers/:id/reclaim` — maker cancels
  - `GET  /api/offers/:id/contract` — covenant/script source + on-chain refs
  - `GET  /api/assets`, `/api/pairs`
  - `WS   /api/events` — re-broadcasts the relay's offer stream + on-chain events
- **Health:** `/healthz` (liveness), `/readyz` (downstreams reachable).
- **Container:** `ghcr.io/kaleidoswap/mosaik-api:<tag>`. Multi-stage
  `rust:1.83-bookworm` → `debian:bookworm-slim`. ~80MB.
- **OpenAPI:** `utoipa` to auto-generate `openapi.json`; the webapp consumes
  it for TS types via `openapi-typescript`.

### 2. `mosaik-webapp` (NEW — Vite + React + TS)

- **Why Vite:** fast dev, tiny static output, ecosystem standard. React
  for component familiarity (Svelte/Solid are fine alternatives but React
  matches every other KaleidoSwap frontend).
- **Stack:** Vite 5 + React 18 + TypeScript + Tailwind (matches the dark
  Mosaik aesthetic) + TanStack Query (server state) + Zustand (local UI
  state). Nostr subscription via `nostr-tools`.
- **API client:** generated from `mosaik-api`'s OpenAPI.
- **Build:** multi-stage Dockerfile — `node:22-alpine` build → `nginx:alpine`
  serve. Static, no SSR. Bundled offer-schema TS types pinned to the api
  version it ships against.
- **Image:** `ghcr.io/kaleidoswap/mosaik-webapp:<tag>`.
- **Migration:** the current vanilla HTML lives at `webapp/index.html` baked
  into the CLI via `include_str!`. We port its panels (order book, depth,
  trade tape, contract inspector, wallet switcher) into React components.
  The visual language (KaleidoSwap dark theme, panel grid) carries over.

### 3. `services/mosaik-relay-server` (NEW — Nostr relay)

A purpose-built Nostr relay shipped as part of Mosaik. Two options; we pick
B for industrial use:

- **A.** Keep `mosaik_relay::run_local_relay` (built on `nostr-relay-builder`)
  as the in-process demo relay. Fine for tests; not for production.
- **B.** Run `scsibug/nostr-rs-relay` (mature, sqlite-backed, dockerized)
  with a **policy plugin** that:
    1. Restricts accepted kinds to `30050` (offers), `30051` (maker
       profiles), `5` (deletion). Everything else rejected.
    2. Schema-validates the offer JSON in `content` against the same
       `OfferKind` JSON Schema we export from `mosaik-core`.
    3. Optional `chain-check` mode: re-derives the covenant address /
       VHTLC script / banco offer-hex and verifies the funding UTXO exists
       on the relevant chain. Rejects stale offers at admission time.
    4. Federation: pulls from a configured set of peer Mosaik relays.
- **Image:** `ghcr.io/kaleidoswap/mosaik-relay-server:<tag>` — base
  `scsibug/nostr-rs-relay` + our policy WASM/Rust plugin built into the image.
- **Default port:** `7777` WS. Config via `/etc/mosaik-relay/config.toml`
  mounted from compose.

### 4. `services/banco-proxy` (NEW — TS microservice)

Specified fully in `BANCO_INTEGRATION_PLAN.md`. Highlights:
- TS/Node 22; mirrors `ark-proxy`'s JWT + REST shape.
- Wraps `@arkade-os/banco` `Maker` / `Taker`.
- SSE endpoint relays arkd's tx stream filtered to banco offer packets.
- Pin `@arkade-os/banco` + its `wallet-sdk` fork to a known-good commit.
- Image: `ghcr.io/kaleidoswap/banco-proxy:<tag>`.

### 5. Upstream services we orchestrate (not authored)

| Service | Image | Pinned |
|---|---|---|
| elementsd | `blockstream/elementsd:23.2.7` | regtest stack |
| bitcoind (mutinynet) | `mutinynet/bitcoin:25.0` (or self-build w/ signet challenge) | mutinynet stack |
| nbxplorer | `nicolasdorier/nbxplorer:2.6.7` | mutinynet stack |
| arkd-wallet | `ghcr.io/arkade-os/arkd-wallet:<latest-tag>` | mutinynet stack |
| arkd | `ghcr.io/arkade-os/arkd:<latest-tag>` | mutinynet stack |
| introspector | `ghcr.io/arklabshq/introspector:v0.0.1` | mutinynet stack |
| bancod (optional MM) | build from `arkade-os/bancod` at pinned commit | mutinynet stack |

All pinned in a single `docker/.versions.env` so upgrades are one PR.

## Docker compose layout

Three composes, layered:

- `docker/compose.yaml` — base: `mosaik-api`, `mosaik-webapp`,
  `mosaik-relay-server`. Always needed.
- `docker/compose.liquid.yaml` — overlay: `elementsd` regtest + funded wallets.
- `docker/compose.arkade-mutinynet.yaml` — overlay: bitcoind(signet/mutinynet)
  + nbxplorer + arkd-wallet + arkd + introspector + banco-proxy.

Bring-up patterns:
```
# Liquid-only dev
docker compose -f docker/compose.yaml -f docker/compose.liquid.yaml up

# Full Liquid + Arkade-mutinynet
docker compose -f docker/compose.yaml \
               -f docker/compose.liquid.yaml \
               -f docker/compose.arkade-mutinynet.yaml up
```

A `Makefile` at repo root wraps the common combos (`make dev-liquid`,
`make dev-full`, `make e2e`).

## Protocol surface (versioned, public)

`docs/PROTOCOL.md` documents:

- The **Nostr offer event** (kind 30050) schema, with the JSON Schema for
  the `Offer` content. Versioned via a `v` tag (`v=1`). The relay rejects
  unknown versions.
- The **`OfferKind` discriminator** — `liquid` | `banco` | (future)
  `bitcoin` | `rln`. New backends bump the schema with a new variant.
- The **kind-30051 maker profile** — supported pairs, fill rate, cancel
  delay, contact URI. Lets the webapp surface maker reputation.

The schema is exported from `mosaik-core` (a `schemars` derive on the
public types) so the relay's policy plugin and the webapp's TS types
both pin against the same source.

## CI / Release

`.github/workflows/`:

- `ci.yaml` (on PR + push):
  - `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`.
  - `pnpm -r lint && pnpm -r test` for `services/*` and `webapp/`.
  - Build all Dockerfiles (no push).
- `release.yaml` (on tag `v*`):
  - Build and push every image to `ghcr.io/kaleidoswap/*:${TAG}` and
    `:latest`.
  - Publish `mosaik-cli` binaries (linux + macOS) as a GitHub Release.
- `e2e-nightly.yaml` (cron):
  - Spin the full mutinynet stack inside a runner; run
    `scripts/e2e-banco.sh` and `scripts/e2e-liquid.sh`; assert both pass.
  - Surface failures to Slack/Telegram.

## Environment & secrets

A single `.env.example` per stack; secrets injected via the runtime (1Password
Connect / GitHub Actions secrets / docker secrets in prod). No checked-in
keys. Mutinynet wallet seeds are dev-only and clearly marked `regtest-only`.

## Migration plan (phased)

### Phase 0 — branch hygiene (this PR)
- [x] Branch `feat/industrialize-covenant-dex` cut from `main`.
- [x] `docs/BANCO_INTEGRATION_PLAN.md` carried over.
- [x] This document.

### Phase 1 — split the API out of the CLI (1–2 days)
- Create `crates/mosaik-api` with axum, move `mosaik-cli/src/server.rs` logic in.
- Add `utoipa` annotations; emit `openapi.json` at build time.
- Dockerfile + image. CLI shrinks (no `tiny_http`, no `tokio` runtime).
- Webapp still served from the API (legacy HTML) until Phase 4.

### Phase 2 — banco backend (3–5 days, gated on Phase 1)
- Build `crates/mosaik-banco` per `BANCO_INTEGRATION_PLAN.md`.
- Build `services/banco-proxy` TS microservice.
- Wire `OfferKind::Banco(BancoOffer)` in `mosaik-core` (drops VHTLC).
- New CLI: `--backend liquid|banco` on the three canonical commands.

### Phase 3 — Mutinynet stack (2–3 days)
- `docker/compose.arkade-mutinynet.yaml` with the upstream services pinned.
- `bancod.Dockerfile` build from pinned commit.
- `scripts/mutinynet-bootstrap.sh` (wallet init, faucet pointer, sanity).
- `scripts/e2e-banco.sh` runs end-to-end against the stack.

### Phase 4 — Nostr relay-as-a-service (2–3 days)
- `services/mosaik-relay-server` based on `nostr-rs-relay` + policy plugin.
- Schema export from `mosaik-core` via `schemars`.
- Relay added to `docker/compose.yaml`; API talks to it locally instead of
  a hardcoded external URL.

### Phase 5 — Vite SPA (5–7 days)
- `webapp/` rewritten as Vite + React + TS.
- Components: order book, depth chart, trade tape, contract inspector,
  wallet switcher, **backend selector** (Liquid / Arkade).
- TanStack Query against `mosaik-api`; Nostr subscription against
  `mosaik-relay-server`.
- Dockerfile (node:22 → nginx:alpine).
- Old vanilla HTML deleted; CLI no longer embeds it.

### Phase 6 — CI / release / e2e (2–3 days)
- GH Actions: ci, release-on-tag, nightly e2e.
- Pinned `docker/.versions.env`.
- First tagged release: `v0.2.0-platform`.

### Phase 7 — federated relays + maker profiles (later)
- Kind-30051 maker profile events.
- Relay-to-relay federation config.
- Webapp surfaces maker reputation.

### Phase 8 — cross-chain Liquid-Tessera ↔ Arkade-banco (the prize)
- Shared preimage commitment on both covenants.
- New `OfferKind::CrossChain { liquid_leg, banco_leg }`.
- API + CLI flow that publishes both legs atomically.
- The first credibly-neutral self-custodial L2-to-L2 covenant bridge.

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| `@arkade-os/banco` alpha + fork dependency churn | Pin commits; rebase weekly; CI canary |
| Mutinynet RPC rate limits | Run our own signet bitcoind in compose |
| Introspector trust (liveness cosigner) | Run our own; eventually multi-introspector |
| Webapp rewrite ships regressions vs vanilla HTML | Keep old HTML behind a `?legacy=1` query param for one release |
| OpenAPI / TS-types drift | Schema is the source of truth, generated both sides on build |
| Relay storage growth | sqlite + retention policy (drop kind 30050 after `cancel_delay` + grace) |
| Multi-service local dev complexity | `make dev-*` targets; healthchecks gate `depends_on` |

## Naming & ownership

- npm scope `@kaleidoswap/` for the TS services (banco-proxy, future
  webapp shared libs).
- Docker images under `ghcr.io/kaleidoswap/`.
- Rust crates keep `mosaik-*` naming (we may rename `mosaik-core` to
  `mosaik-protocol` later if the platform framing sticks).

## Definition of done for this branch

The branch is mergeable when:
1. Phases 1–6 are landed.
2. `make dev-full` brings the whole stack up locally; webapp renders
   offers from both backends; e2e scripts pass.
3. CI green; first tagged platform release built and pushed.
4. README updated to describe the platform shape (not the hackathon binary).

---

## What we're really building

The covenant-DEX-platform framing is what the rest of this work is in
service of. Three claims:

1. **Mosaik's `OfferKind` is the right abstraction.** A funded UTXO
   governed by a keyless rule + a counter-payment commitment is
   chain-agnostic. Liquid Simplicity expresses it via introspection;
   Arkade Script expresses it via banco; Bitcoin will eventually express
   it via CTV/CSFS. The same `Offer` JSON, the same Nostr event, the same
   `take-offer` button work for all three.

2. **The Nostr relay is the orderbook layer.** Once the protocol is
   versioned (kind 30050 v=1) and the schema is in `mosaik-core`, anyone
   can run a relay, anyone can write a different frontend, anyone can run
   a different matching/MM bot. Mosaik becomes a *protocol with reference
   implementations*, not a product.

3. **The killer feature is the cross-chain covenant bridge.** Phase 8 is
   the moment Mosaik becomes uniquely interesting — atomic Liquid ↔ Arkade
   swaps with no LSP, no oracle, no trusted intermediary, just two
   covenants sharing a preimage. KaleidoSwap's HTLC product is the
   interactive cousin; Mosaik's covenant bridge is the non-interactive,
   maker-can-go-offline sibling. That's the moat.

Everything in Phases 1–7 exists to make Phase 8 plausible. The
industrialisation is the foundation, the cross-chain swap is the building.

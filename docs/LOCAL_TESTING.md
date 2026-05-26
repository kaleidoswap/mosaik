# Mosaik — Local Testing Playbook

Canonical recipe for exercising every layer of the platform on a single
machine, plus the results of the run that captured this doc.

## Tooling required

| | min version | check |
|---|---|---|
| Rust | 1.85 | `cargo --version` |
| Node | 22.12 | `node --version` |
| pnpm | 10 | `pnpm --version` |
| Docker | 24+ | `docker --version` |
| `hal-simplicity` | (Liquid covenants) | `which hal-simplicity` |
| `elementsd` / `elements-cli` | for Liquid regtest | `which elementsd` |

The Liquid regtest path also needs an Elements binary on PATH (or
`ELEMENTSD_EXEC` / `ELEMENTS_CLI_EXEC` set). Bring up the regtest with:

```bash
./scripts/regtest.sh up
```

## The five-layer check

### 1. Rust workspace
```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
```
Expect: 18 `test result: ok` lines (mosaik-core 4, mosaik-banco 1,
mosaik-bridge 24+1 doctest, mosaik-relay 6+1 integration, tessera 8+5,
mosaik-api 0, mosaik-cli 0).

### 2. Liquid covenant flow (mosaik-api against live elementsd regtest)

```bash
# Build once.
cargo build --release --bin mosaik-api --bin mosaik

# Run mosaik-api on a free port (default 8080 may collide).
./target/release/mosaik-api --port 18080 &

# Fund + issue test assets.
curl -s -X POST http://127.0.0.1:18080/api/fund -d '{"wallet":"user"}'  -H content-type:application/json
curl -s -X POST http://127.0.0.1:18080/api/fund -d '{"wallet":"mm-a"}'  -H content-type:application/json

# Maker posts a covenant offer.
curl -s -X POST http://127.0.0.1:18080/api/make-offer \
  -H content-type:application/json \
  -d '{"maker":"mm-a","lock":"BTC","want":"USDT","amount_a":1000000,"amount_b":500000,"timeout":1000}'

# Inspect the covenant (CMR, source, address, Nostr metadata).
curl -s -X POST http://127.0.0.1:18080/api/contract -d '{"index":0}' -H content-type:application/json

# Taker fulfils.
curl -s -X POST http://127.0.0.1:18080/api/take-offer -d '{"index":0,"taker":"user"}' -H content-type:application/json

# Confirm the trade tape advanced.
curl -s http://127.0.0.1:18080/api/state | jq '.trades'
```

**Covenant rejection drill** — proves the Simplicity covenant is
actually enforcing:

```bash
# Post a fresh offer (index 0 if the previous one was filled).
curl -s -X POST http://127.0.0.1:18080/api/make-offer \
  -H content-type:application/json \
  -d '{"maker":"mm-a","lock":"BTC","want":"USDT","amount_a":500000,"amount_b":250000,"timeout":1000}'

# Each cheat below must come back with `rejected: true`:
for CHEAT in underpay wrong_recipient wrong_index; do
  curl -s -X POST http://127.0.0.1:18080/api/take-offer \
    -H content-type:application/json \
    -d "{\"index\":0,\"taker\":\"user\",\"cheat\":\"$CHEAT\"}"
done

# Honest take of the same offer should succeed.
curl -s -X POST http://127.0.0.1:18080/api/take-offer \
  -d '{"index":0,"taker":"user"}' -H content-type:application/json
```

### 3. Bridge CLI (mosaik-bridge state machine)

```bash
./target/release/mosaik bridge-make --swap-id local-test-01 --out /tmp/swap.json
./target/release/mosaik bridge-status --envelope /tmp/swap.json
./target/release/mosaik bridge-take --envelope /tmp/swap.json    # surfaces Gap B
./target/release/mosaik bridge-claim --envelope /tmp/swap.json \
  --preimage <hex from the make-offer output>                    # surfaces Gap A
```

`bridge-make` succeeds and writes a well-formed envelope; the
take/claim paths return clean `PrimitiveGap` errors pointing at
`docs/CROSS_CHAIN_SWAP.md` (expected — the on-chain primitives are
follow-up work).

### 4. Vite SPA against the live API

```bash
cd webapp
VITE_API_URL=http://127.0.0.1:18080 pnpm install
VITE_API_URL=http://127.0.0.1:18080 pnpm run dev
# Open http://localhost:5173
```

Confirm:
- Header shows the running chain height.
- Order book shows posted offers.
- Wallet switcher cycles through user / mm-{a,b,c}.
- "Make offer" form posts (against funded wallets).
- "Take" button on a foreign offer fulfils via the covenant.

Vite proxies `/api/*` to `VITE_API_URL`; real RPC errors (e.g.
"Insufficient funds") propagate to the UI verbatim.

### 5. Docker images + relay roundtrip

```bash
make build-images   # builds mosaik-api, mosaik-webapp, banco-proxy, mosaik-relay-server
```

Each first-party image:

| | size | status |
|---|---|---|
| `mosaik-api:dev` | ~80MB | Rust binary on debian-slim |
| `mosaik-webapp:dev` | ~80MB | Vite SPA on nginx-alpine |
| `banco-proxy:dev` | ~480MB | TS + @arkade-os/banco on bookworm-slim (native deps) |
| `mosaik-relay-server:dev` | ~230MB | nostr-rs-relay base + our config |

**Relay smoke** — confirm the curated relay accepts a kind-30050
event, returns it on a subscription, and rejects everything else:

```bash
docker run --rm -d --name relay -p 17777:7777 mosaik-relay-server:dev

# Build a sample offer envelope (the OfferKind enum on this branch
# requires byte-array asset_b / maker_spk_hash):
python3 - <<'PY' > /tmp/offer.json
import json
print(json.dumps({
  "outpoint": "deadbeef:0",
  "asset_a": "BTC",
  "amount_a": 2_000_000,
  "kind": {
    "backend": "liquid",
    "asset_b":         [0x11]*32,
    "amount_b":        1_000_000,
    "maker_spk_hash":  [0x22]*32,
    "timeout":         1500
  },
  "maker_address": "ert1qexample"
}))
PY

./target/release/mosaik publish-offer --offer /tmp/offer.json \
  --relay ws://127.0.0.1:17777 --order-id smoke-001 --ttl 600
./target/release/mosaik browse-offers --relay ws://127.0.0.1:17777

docker stop relay
```

**banco-proxy smoke** — auth token + healthz against the container.
Real banco calls additionally need a live arkd + a `BANCO_WALLET_SEED_<ID>`
env var; the proxy answers seed-missing errors cleanly when not set:

```bash
docker run --rm -d --name banco -p 13001:3001 banco-proxy:dev
curl -s http://127.0.0.1:13001/healthz
curl -s -X POST http://127.0.0.1:13001/api/v1/auth/token \
  -H content-type:application/json -d '{"walletId":"maker"}'
docker stop banco
```

## Results from this run

Captured 2026-05-26, against branch `feat/covenant-dex-platform`
tip `295ca00`, on an arm64 macOS host with an `elementsd` regtest
already running at `:7040`.

| Layer | Result |
|---|---|
| 1. Rust workspace | ✅ 18 suites pass; fmt + clippy clean |
| 2.a Liquid make / take / contract / trade | ✅ end-to-end via `mosaik-api`; trade tape shows two settled trades (`L-BTC/USDT 1000000→500000 blk 660`, `…500000→250000 blk 662`) |
| 2.b Covenant rejects 3 cheats | ✅ all three (`underpay`, `wrong_recipient`, `wrong_index`) returned `rejected: true` with `hal-simplicity: Jet failed during execution`; the honest follow-up take succeeded |
| 3. Bridge CLI | ✅ `bridge-make` drafts a valid envelope; `bridge-take`/`bridge-claim` surface clean `PrimitiveGap` errors pointing at the design doc |
| 4. Vite SPA | ✅ dev server up; `<title>Mosaik — covenant DEX</title>` renders; `/api/state` proxies HTTP 200; real RPC errors propagate to the UI |
| 5.a `mosaik-relay-server:dev` | ✅ image builds, container starts, sqlite migrations to v18, publish + browse roundtrip works (event id returned, browse retrieves) |
| 5.b `banco-proxy:dev` | ✅ image builds (after the fixes below), container starts, `/healthz` + `/auth/token` serve |
| 5.c `mosaik-webapp:dev` | ✅ image builds (82MB) |
| 5.d `mosaik-api:dev` | ⚠️ transient `docker.io` registry EOF pulling the rust base — retry. Local `cargo build --release --bin mosaik-api` works perfectly |

## Fixes the run captured

These landed in the same branch during the run:

1. **`services/banco-proxy/Dockerfile`** — switched from `node:22-alpine`
   to `node:22-bookworm-slim`. Alpine's musl + missing Python /
   build-essential made `@arkade-os/banco`'s native dev deps (`canvas`,
   `better-sqlite3`) fail to build. Added `ca-certificates`, `git`,
   `python3`, `build-essential`, `pkg-config`, `libcairo2-dev`,
   `libpango1.0-dev`, `libjpeg-dev`, `libgif-dev`, `librsvg2-dev` in
   the builder; runtime keeps only the non-`-dev` variants.
2. **Same Dockerfile** — replaced `corepack enable && corepack prepare
   pnpm@10.25.0 --activate` with `npm install -g pnpm@10.25.0`.
   Corepack's bundled signing keys reject newer pnpm releases on the
   Node 22 base today.
3. **Same Dockerfile** — `COPY scripts ./scripts` must precede `pnpm
   install`, since the `postinstall` hook (`patch-banco-esm.mjs`)
   runs during the initial install.

## Cleanup

```bash
# stop everything this playbook starts
kill %1 %2 2>/dev/null              # mosaik-api + vite dev server
docker stop relay banco 2>/dev/null
rm -f /tmp/swap.json /tmp/offer.json /tmp/{api,proxy,vite}.log
```

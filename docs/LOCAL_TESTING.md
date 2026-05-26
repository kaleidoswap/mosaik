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

## The eight-layer check

The platform's shape has grown since this playbook was first written
(swap state machine, maker profiles, banco backend dispatch, live
relay subscription, API → relay publishes). Each layer below is a
focused thing to confirm; run them in order, each one builds on the
last.

### 1. Rust workspace

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
```

Expect: **18** `test result: ok` lines, including:

| crate | tests |
|---|---:|
| mosaik-core | 4 |
| mosaik-banco | 1 |
| mosaik-bridge | 30 unit + 1 doctest |
| mosaik-relay | 6 + 1 integration |
| tessera | 8 execution + 5 keyless |
| mosaik-api / mosaik-cli | 0 (covered by smoke below) |

### 2. Liquid covenant flow (mosaik-api against live elementsd regtest)

```bash
cargo build --release --bin mosaik-api --bin mosaik
./target/release/mosaik-api --port 18080 &

# Fund + issue test assets.
curl -s -X POST http://127.0.0.1:18080/api/fund -d '{"wallet":"user"}' -H content-type:application/json
curl -s -X POST http://127.0.0.1:18080/api/fund -d '{"wallet":"mm-a"}' -H content-type:application/json

# Maker posts an offer (defaults to backend=liquid).
curl -s -X POST http://127.0.0.1:18080/api/make-offer \
  -H content-type:application/json \
  -d '{"maker":"mm-a","lock":"BTC","want":"USDT","amount_a":1000000,"amount_b":500000,"timeout":1000}'

# Inspect the Simplicity covenant (CMR, source, address, Nostr metadata).
curl -s -X POST http://127.0.0.1:18080/api/contract -d '{"index":0}' -H content-type:application/json

# Taker fulfils.
curl -s -X POST http://127.0.0.1:18080/api/take-offer -d '{"index":0,"taker":"user"}' -H content-type:application/json

curl -s http://127.0.0.1:18080/api/state | jq '.trades'
```

**Covenant rejection drill** — proves the Simplicity covenant enforces:

```bash
curl -s -X POST http://127.0.0.1:18080/api/make-offer \
  -H content-type:application/json \
  -d '{"maker":"mm-a","lock":"BTC","want":"USDT","amount_a":500000,"amount_b":250000,"timeout":1000}'

for CHEAT in underpay wrong_recipient wrong_index; do
  curl -s -X POST http://127.0.0.1:18080/api/take-offer \
    -H content-type:application/json \
    -d "{\"index\":0,\"taker\":\"user\",\"cheat\":\"$CHEAT\"}"
done
# Each cheat MUST return `rejected: true`. Then the honest follow-up
# settles for real:
curl -s -X POST http://127.0.0.1:18080/api/take-offer \
  -d '{"index":0,"taker":"user"}' -H content-type:application/json
```

### 3. Banco backend dispatch (mosaik-api → banco-proxy)

```bash
# Bring the proxy up (image built by `make build-banco-proxy`).
docker run --rm -d --name banco -p 13001:3001 banco-proxy:dev
sleep 2

# Restart mosaik-api pointed at the proxy.
BANCO_PROXY_URL=http://127.0.0.1:13001 \
  ./target/release/mosaik-api --port 18081 &

# A banco make-offer with no wallet seed configured returns the *full*
# error chain (anyhow's `{:#}`), naming the env var the proxy expects:
curl -s -X POST http://127.0.0.1:18081/api/make-offer \
  -H content-type:application/json \
  -d '{"backend":"banco","maker":"mm-a","lock":"BTC","want":"USDT",
       "amount_a":1500000,"amount_b":750000,"cancel_delay":1900000000}'
# Expect: "POST /banco/maker/offer: banco-proxy returned HTTP 500:
#          no wallet seed configured for walletId='mm-a' …"

docker stop banco
```

When the proxy *has* `BANCO_WALLET_SEED_MM_A` set and arkd is reachable,
the same call lands a real banco offer (upstream `@arkade-os/banco` is
still alpha; see `services/banco-proxy/README.md` §"Upstream caveat").

### 4. Maker profiles (kind-30051)

```bash
curl -s http://127.0.0.1:18080/api/maker-profiles | jq '.profiles | length'
# Expect: 4 (seeded at boot: user, mm-a, mm-b, mm-c).

# Replace one in memory.
curl -s -X POST http://127.0.0.1:18080/api/maker-profiles \
  -H content-type:application/json \
  -d '{"label":"user","profile":{"d_tag":"mosaik-user","name":"Walter",
        "pairs":[{"backend":"liquid","lock":"BTC","want":"USDT"}],
        "fill_latency_secs":15,"badges":["self-attested"]}}'
```

### 5. Cross-chain bridge (mosaik-bridge state machine)

Two paths — the CLI walks the typed state machine on the bridge crate
directly; the API drives the in-memory MockOrchestrator alongside.

```bash
# CLI — mock end-to-end.
./target/release/mosaik bridge-make --swap-id local-test-01 --out /tmp/swap.json
./target/release/mosaik bridge-status --envelope /tmp/swap.json
./target/release/mosaik bridge-take --envelope /tmp/swap.json    # → Gap B
./target/release/mosaik bridge-claim --envelope /tmp/swap.json \
  --preimage <hex from the make-offer output>                    # → Gap A

# API — same machine, exposed over HTTP.
curl -s -X POST http://127.0.0.1:18080/api/swaps/draft \
  -H content-type:application/json -d '{"swap_id":"api-test"}' | jq
for ACTION in publish_leg_a publish_leg_b observe_preimage complete; do
  curl -s -X POST "http://127.0.0.1:18080/api/swaps/api-test/advance" \
    -H content-type:application/json -d "{\"action\":\"$ACTION\"}" \
    | jq -r '.state.state'
done
# Expect: half_published, both_published, revealed, settled.
```

`mosaik-bridge`'s `RealOrchestrator` (`crates/mosaik-bridge/src/real.rs`)
is wired against `mosaik-core` + `mosaik-banco` and ready for Gap A / B
to land — its `draft` and `refund` paths work today against real
backends; the SHA-256-bound paths return `PrimitiveGap` errors pointing
at `docs/CROSS_CHAIN_SWAP.md`.

### 6. Vite SPA against the live API

```bash
cd webapp
VITE_API_URL=http://127.0.0.1:18080 pnpm install
VITE_API_URL=http://127.0.0.1:18080 \
  VITE_RELAY_URL=ws://127.0.0.1:17777 \
  pnpm run dev
# Open http://localhost:5173
```

Confirm:

- Header: chain height pill + **live-relay status chip** (mint dot when
  the relay subscription is open) + active-wallet switcher + backend chip.
- Order book: posted offers with backend badge per row, **MakerProfileChip**
  next to each maker name (badges + ⏱ fill latency).
- "Make offer" form posts (against funded wallets); switching to banco
  enables the cancel-delay + partial-fill fields.
- "Take" button on a foreign offer fulfils via the covenant.
- **SwapBoard** panel: draft a swap, click through the state-machine
  buttons (publish leg A → publish leg B → observe preimage → complete).
  State pill colour transitions: zinc → cyan → cyan → amber → mint.
- **RelayFeed** panel: events streaming in from the relay as the API
  publishes them.

### 7. Docker images + relay container roundtrip

```bash
make build-images   # mosaik-api, mosaik-webapp, banco-proxy, mosaik-relay-server
```

| | size | notes |
|---|---:|---|
| `mosaik-api:dev` | ~80 MB | rust:1.85 builder → debian:bookworm-slim |
| `mosaik-webapp:dev` | ~80 MB | Vite SPA on nginx-alpine |
| `banco-proxy:dev` | ~480 MB | TS + @arkade-os/banco on bookworm-slim (native deps) |
| `mosaik-relay-server:dev` | ~230 MB | nostr-rs-relay 0.9 + our config |

**Relay smoke** — confirm the curated relay accepts kind-30050 + rejects
everything else:

```bash
docker run --rm -d --name relay -p 17777:7777 mosaik-relay-server:dev
sleep 2

# Construct an OfferKind envelope (byte-array asset_b / maker_spk_hash).
python3 - <<'PY' > /tmp/offer.json
import json
print(json.dumps({
  "outpoint": "deadbeef:0", "asset_a": "BTC", "amount_a": 2_000_000,
  "kind": {"backend":"liquid","asset_b":[0x11]*32,"amount_b":1_000_000,
           "maker_spk_hash":[0x22]*32,"timeout":1500},
  "maker_address": "ert1qexample"
}))
PY
./target/release/mosaik publish-offer --offer /tmp/offer.json \
  --relay ws://127.0.0.1:17777 --order-id smoke-001 --ttl 600
./target/release/mosaik browse-offers --relay ws://127.0.0.1:17777

docker stop relay
```

### 8. mosaik-api → relay publishes (the loop closes)

The API publishes a Nostr event for every successful write
(`/api/make-offer` → kind 30050, `/api/maker-profiles` → kind 30051,
`/api/swaps/{draft,advance,refund}` → kind 30052). Verify by querying
the relay's sqlite directly:

```bash
docker run --rm -d --name relay -p 17777:7777 mosaik-relay-server:dev
sleep 2

MOSAIK_RELAY_URL=ws://127.0.0.1:17777 \
  ./target/release/mosaik-api --port 18080 &
sleep 2

# Drive each kind of write once.
curl -s -X POST http://127.0.0.1:18080/api/fund     -d '{"wallet":"mm-a"}' -H content-type:application/json > /dev/null
curl -s -X POST http://127.0.0.1:18080/api/make-offer -d '{"maker":"mm-a","lock":"BTC","want":"USDT","amount_a":1000000,"amount_b":500000,"timeout":2000}' -H content-type:application/json > /dev/null
curl -s -X POST http://127.0.0.1:18080/api/maker-profiles -d '{"label":"mm-c","profile":{"d_tag":"tessera-lp","name":"Tessera LP v2","pairs":[],"badges":["audited","federated"]}}' -H content-type:application/json > /dev/null
curl -s -X POST http://127.0.0.1:18080/api/swaps/draft -d '{"swap_id":"publish-test"}' -H content-type:application/json > /dev/null
curl -s -X POST http://127.0.0.1:18080/api/swaps/publish-test/advance -d '{"action":"publish_leg_a"}' -H content-type:application/json > /dev/null
sleep 1

# Count rows per kind in the relay's sqlite.
docker exec relay sqlite3 /usr/src/app/db/nostr.db "SELECT kind, COUNT(*) FROM event GROUP BY kind;"
# Expect (NIP-33 addressable replacement is what makes 30052 a 1):
#   30050|1       (one offer)
#   30051|1       (the new profile)
#   30052|1       (draft + advance replace under the same swap_id+pubkey+kind)

docker stop relay
```

The SPA's `useRelayFeed` hook subscribes to the same relay and will
render every event live — no polling. Drive any of the above
endpoints while the SPA is open and watch the RelayFeed panel
populate within a frame.

## Results from this run

Captured against branch `feat/covenant-dex-platform` tip (post Phase
2.5 + relay-publish wiring), on an arm64 macOS host with an existing
`elementsd` regtest at `:7040`.

| Layer | Result |
|---|---|
| 1. Rust workspace | ✅ 18 suites pass (mosaik-bridge has 30 unit + 1 doctest after RealOrchestrator); fmt + clippy clean |
| 2.a Liquid make / take / contract / trade | ✅ end-to-end via `mosaik-api`; trades land in successive blocks |
| 2.b Covenant rejects 3 cheats | ✅ all three (`underpay`, `wrong_recipient`, `wrong_index`) return `rejected: true` with `hal-simplicity: Jet failed during execution`; honest follow-up succeeds |
| 3. Banco backend dispatch | ✅ no-seed call surfaces the full anyhow chain pointing at the missing env var |
| 4. Maker profiles | ✅ 4 seeded profiles, POST replaces in memory + publishes kind-30051 |
| 5. Bridge state machine | ✅ CLI walks; API drives the same machine over HTTP; on-chain `PrimitiveGap` errors mention `docs/CROSS_CHAIN_SWAP.md` |
| 6. Vite SPA | ✅ dev server up, all panels render, RelayFeed populates from API writes |
| 7.a `mosaik-relay-server:dev` | ✅ image builds, container starts, publish + browse roundtrip works |
| 7.b `banco-proxy:dev` | ✅ image builds (~480 MB after the bookworm move); container serves `/healthz` + `/auth/token` |
| 7.c `mosaik-webapp:dev` | ✅ image builds (~80 MB) |
| 7.d `mosaik-api:dev` | ⚠️ transient docker.io registry EOF in one run; local release binary works |
| 8. API → relay publishes | ✅ counts visible in relay sqlite: `30050\|1`, `30051\|1`, `30052\|1` (latter is one row because NIP-33 addressable replacement does its job across draft + advance under the same swap_id) |

## Cleanup

```bash
# stop everything this playbook starts
kill %1 %2 %3 2>/dev/null              # mosaik-api(s) + vite dev server
docker stop relay banco 2>/dev/null
rm -f /tmp/swap.json /tmp/offer.json /tmp/{api,proxy,vite}.log
```

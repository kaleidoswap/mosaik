# Mosaik — developer guide for Claude Code

## What this project is

Mosaik is a trustless DEX on the Liquid Network where every order is a
**Tessera** — a Liquid UTXO governed by a Simplicity covenant. A Tessera
releases its locked asset only to a transaction that pays the maker exactly the
agreed counter-asset. Settlement is enforced by the Liquid node itself; no
server, escrow, or matching engine is needed.

Built for the Blockstream Simplicity Bootcamp & Hackathon, Turin, May 2026.

---

## Repository layout

```
mosaik/
├── crates/
│   ├── tessera/              # Tessera covenant: SimplicityHL source + Rust compiler wrapper
│   │   ├── contracts/tessera.simf   # the covenant (SimplicityHL)
│   │   └── src/lib.rs               # Tessera struct, compile(), settle_witness(), refund_witness()
│   ├── mosaik-core/          # offer lifecycle, PSET construction, Elements RPC helpers
│   ├── mosaik-relay/         # Nostr orderbook — publish/fetch Tessera offers
│   └── mosaik-cli/           # `mosaik` binary: CLI commands + browser wallet HTTP server
│       ├── src/main.rs        # clap commands
│       └── src/server.rs      # tiny_http server, /api/* handlers
├── webapp/index.html         # single-file browser wallet UI (served by mosaik serve-wallet)
├── scripts/
│   ├── regtest.sh            # local Elements regtest harness
│   └── testnet.sh            # Liquid testnet demo (codespace approach, no local node)
└── docs/DESIGN.md            # protocol and covenant spec
```

---

## Two environments

### Regtest (local, full-featured)

Requires a Simplicity-capable `elementsd` (from smplx or simplicity-codespace).

```sh
export ELEMENTSD_EXEC="$PWD/tools/elementsd"
./scripts/regtest.sh up           # start node, create wallets, fund treasury
cargo run -p mosaik-cli -- serve-wallet --port 8080
open http://127.0.0.1:8080
./scripts/regtest.sh down         # stop the node
```

The UI issues two test assets (USDT, EURx) on first Fund. It mines a block
after every settlement/reclaim so the taker's balance updates immediately.

Wallet names on the node: `mosaik` (treasury), `mosaik-maker`, `mosaik-taker`.
RPC: `http://127.0.0.1:7040`, user/pass `user`/`pass`.

### Liquid testnet (no local node)

Follows the Blockstream simplicity-codespace pattern. No `elementsd` needed.

```sh
./scripts/testnet.sh make-offer   # compile covenant, faucet-fund, save offer.json
./scripts/testnet.sh take-offer   # build PSET with hal-simplicity, broadcast via Esplora
```

Key design decisions in `testnet.sh`:
- `amount_b_sats` defaults to 99 500 sats — leaves 500 sats for fee when the
  faucet sends its standard 100 000-sat drop, so the PSET balances exactly.
- If the faucet sends more than `amount_b + 500`, a second change output is
  added (also to the maker address) so the PSET always balances.
- `hal-simplicity simplicity pset update-input` requires a Taproot scriptPubKey
  (`5120…`) — the script uses vout 0's SPK from the Esplora response.

---

## CLI commands (mosaik binary)

| Command | Description |
|---|---|
| `make-offer` | Fund a covenant UTXO on regtest, print offer JSON |
| `take-offer` | Fill an offer on regtest (node executes the covenant) |
| `reclaim` | Maker reclaims an expired offer via the REFUND path |
| `tessera-json` | Compile a Tessera, output `{cmr, program}` as JSON (base64 program) |
| `settle-json` | Build the SETTLE witness, output `{program, witness}` as JSON |
| `settle-witness` | Print the SETTLE witness in hex (human-readable form) |
| `compile-tessera` | Print CMR and covenant address |
| `show-tessera` | Print the rendered SimplicityHL source for given parameters |
| `testnet-constants` | Print the hardcoded demo maker keys as JSON |
| `serve-wallet` | Serve the browser wallet UI on `127.0.0.1:<port>` |
| `serve-relay` | Run a local Nostr relay for the orderbook |
| `publish-offer` | Publish an offer JSON to a Nostr relay |
| `browse-offers` | List Tessera offers from a Nostr relay |

---

## Key crates

### `tessera`

`Tessera` struct holds the five covenant parameters. `render()` substitutes
them into `contracts/tessera.simf` (tagged with `// TESSERA_PARAM:<NAME>`).
`compile()` calls the Simplicity compiler and returns a `CompiledTessera` with
`cmr_hex()` and `address()` (regtest bech32m). `settle_witness()` and
`refund_witness()` produce the `TesseraWitness` needed to spend the UTXO.

The covenant checks **exact equality** (`jet::eq_64`) for `amount_b`, so the
settlement transaction must pay exactly `amount_b` sats — not more, not less.

### `mosaik-core`

- `ElementsRpc` — thin wrapper around the Elements RPC. Two constructors:
  `::regtest_wallet(name)` (port 7040) and `::wallet(url, name, user, pass)`.
- `MosaikMaker::make_offer` — builds and broadcasts the covenant-funding tx.
- `MosaikTaker::settle` — builds and broadcasts the settlement tx (SETTLE path).
- `MosaikMaker::reclaim` — builds and broadcasts the reclaim tx (REFUND path).
- `DEMO_MAKER_SECRET` / `demo_maker_pk()` — the hardcoded `[7; 32]` test key.

### `mosaik-cli/src/server.rs`

Serves `webapp/index.html` for `GET /` and JSON for `/api/*`. All state is
in-memory (`AppState` behind a `Mutex`). Test assets (USDT, EURx) are issued
from the treasury wallet on first `POST /api/fund/*`. The server mines one
block after every settlement or reclaim to confirm the tx immediately.

---

## Tests

```sh
cargo test                        # all crates
cargo test -p tessera             # covenant compile + execution tests
cargo test -p mosaik-core         # offer lifecycle + settlement tests (needs elementsd)
```

The `tessera` tests are pure (no node). The `mosaik-core` tests hit the local
regtest node — run `./scripts/regtest.sh up` first.

---

## Conventions

- The Tessera covenant lives in `crates/tessera/contracts/tessera.simf`.
  The Rust crate embeds and parameterises it at runtime — do not commit a
  pre-compiled binary.
- Asset IDs appear in two byte orders throughout:
  - **Display order** — what `elementsd` RPC returns, what Esplora shows.
  - **Internal order** — byte-reversed, what the Simplicity `output_asset` jet
    returns. `mosaik-core` reverses display→internal before passing to `Tessera`.
- The NUMS unspendable internal key for the Taproot output is
  `50929b74c1a04954b78b4b6035e97a5e078a5a0f28ec96d547bfee9ace803ac0`
  (same constant used by simplicity-codespace).
- `regtest.sh` wallets: `mosaik` is the treasury; `mosaik-maker` and
  `mosaik-taker` are created by the script and loaded by the server on startup.

# Mosaik

**A DEX on Liquid where every order is a Tessera — a self-enforcing covenant.**

Mosaik is a trustless, permissionless asset exchange on the
[Liquid Network](https://docs.liquid.net). It has no order book, no matching
engine, and no maker server. Each offer is a **Tessera**: a single Liquid coin
whose [Simplicity](https://docs.simplicity-lang.org) covenant enforces the trade
terms. A market is a mosaic of these tesserae.

> On Mosaik, an order isn't a promise a market maker might break —
> it's a covenant the coin itself keeps.

Built for the **Blockstream Simplicity Bootcamp & Hackathon** — Blox Space,
Turin, 16–17 May 2026.

---

## The idea

A maker wants to sell `amount_A` of asset **A** for `amount_B` of asset **B**.

Instead of posting an order to an exchange, the maker locks asset A into a
single Liquid UTXO controlled by a **Tessera** — a Simplicity covenant. That
covenant inspects any transaction trying to spend the coin and allows it
**only if** the transaction pays the maker exactly `amount_B` of asset B.

That one UTXO _is_ the offer:

- **Permissionless** — any taker can fill it by building a transaction that pays
  the maker. The maker does not need to be online.
- **Uncheatable** — the taker cannot take asset A without paying asset B; the
  coin itself rejects any spend that does not. No escrow, no custody.
- **Atomic** — one Elements transaction moves both assets, or neither confirms.
- **No infrastructure** — no order book, no matching engine, no maker daemon.

The maker keeps a refund path: after a timeout it can reclaim the coin with its
own signature.

### Why this needs Simplicity

The whole construction rests on **transaction introspection** — a coin reading
the outputs of the transaction spending it. Bitcoin Script cannot do this.
LiquiDEX-style swaps get atomicity from a `SIGHASH` trick; adaptor-signature
PTLCs work on plain Taproot. A covenant that _enforces the counter-payment_ is
the thing that genuinely requires Simplicity.

### Mosaik & Tessera

- A **Tessera** is one offer — one covenant, one tile.
- **Mosaik** is the market they form — the mosaic.

(A kaleidoscope shows a mosaic — the names sit naturally inside the KaleidoSwap
family.)

---

## How it works

```
        ┌─────────────────────────────────────────────┐
        │  Tessera UTXO  (holds amount_A of asset A)    │
        │                                               │
        │  SETTLE path:  spendable by ANYONE iff the    │
        │     spending tx has an output paying          │
        │     exactly amount_B of asset B to MAKER      │
        │                                               │
        │  REFUND path:  after TIMEOUT, spendable by    │
        │     MAKER's signature                         │
        └─────────────────────────────────────────────┘

  maker                                              taker
    │  1. lock asset A in the Tessera UTXO             │
    │     (terms baked into the tapleaf)               │
    │ ───────────── publish offer (outpoint) ────────► │
    │                                                  │
    │            2. build a tx:                        │
    │               input  = Tessera UTXO + own coins  │
    │               output = amount_B asset B → maker  │
    │               output = amount_A asset A → taker  │
    │               + fee                              │
    │ ◄──────── 3. covenant verifies & tx confirms ─── │
```

The Tessera's parameters (asset B id, amount B, maker script, timeout, maker
pubkey) are committed into the Taproot tapleaf, so they cannot be altered after
the offer is published.

---

## Repository layout

```
mosaik/
├── crates/
│   ├── tessera/              # the Tessera Simplicity covenant
│   │   ├── contracts/        #   SimplicityHL (tessera.simf) source
│   │   └── src/              #   Rust: parameterise + compile the covenant
│   ├── mosaik-core/          # offers, multi-asset PSET construction, Elements RPC
│   ├── mosaik-relay/         # Nostr orderbook — publish / discover Tessera offers
│   └── mosaik-cli/           # `mosaik` CLI + the browser wallet UI server
├── webapp/index.html         # the two-wallet covenant DEX UI (served by mosaik)
├── docs/DESIGN.md            # full protocol + covenant spec
├── docs/hackathon.html       # hackathon plan + 3-dev work division
└── scripts/regtest.sh        # local Elements regtest harness
```

## Status

- [x] Tessera covenant compiles to a CMR; SETTLE and REFUND paths covered by
      execution tests.
- [x] Enforced settlement on a Simplicity-capable `elementsd` — the node
      executes the covenant and rejects any spend that underpays the maker.
- [x] `mosaik` CLI end-to-end: make-offer → take-offer → confirmed.
- [x] Multi-asset swaps — L-BTC/asset, asset/L-BTC and asset/asset, in either
      direction (the covenant never inspects the *locked* asset).
- [x] Nostr orderbook (`mosaik-relay`) and a browser wallet UI with two
      separate maker / taker wallets.
- [x] Adversarial "attack the covenant" path — underpay, wrong recipient,
      hidden maker output — all rejected by the covenant.
- [x] REFUND / reclaim — the maker reclaims an unfilled offer after its
      timeout, signing the Simplicity `sig_all` hash with a BIP-340 key.
- [ ] Recursive partial-fill covenant — fund once, fill many times.

## Install

You need three things: the Rust toolchain, a **Simplicity-capable** `elementsd`,
and `hal-simplicity`.

The quick path — once you have the Simplicity `elementsd` (see below) — is the
helper script. It installs `hal-simplicity`, locates and verifies the node,
ad-hoc signs the binaries on macOS, and builds the workspace:

```sh
./scripts/install-deps.sh
```

Or do it by hand:

```sh
# 1. Rust (the workspace pins its toolchain via rust-toolchain.toml)
curl https://sh.rustup.rs -sSf | sh

# 2. hal-simplicity — assembles the covenant witness for a settlement
cargo install hal-simplicity      # lands in ~/.cargo/bin

# 3. build the workspace
cargo build
```

### Simplicity-capable `elementsd`

Stock Elements can *fund* a covenant address (a normal Taproot payment) but
**cannot validate a covenant spend** — leaf version `0xbe` is non-standard, so a
Tessera settlement is rejected. Spending a covenant needs an `elementsd` built
with the Simplicity consensus rules.

The Mosaik demo was developed against the build bundled with **smplx** (the
Simplicity dev framework — its `simplex` CLI ships a matching `elementsd`,
`elements-cli`, and `electrs`). Either source works:

- **smplx** — install the `simplex` toolchain and use the `elementsd` it
  bundles. See [github.com/BlockstreamResearch/smplx](https://github.com/BlockstreamResearch/smplx).
- **Simplicity codespace** — the Blockstream
  [simplicity-codespace](https://github.com/Blockstream/simplicity-codespace)
  ships a Simplicity-capable `elementsd` and the SimplicityHL tooling.

Put the binaries somewhere stable and point Mosaik at them — the demo expects
them under `tools/` (git-ignored), but any path works via `ELEMENTSD_EXEC`:

```sh
mkdir -p tools
cp /path/from/smplx/{elementsd,elements-cli,electrs,simplex} tools/

# verify it is a Simplicity build (Elements Core v23.3.1, from smplx)
tools/elementsd -version | head -1
```

> **macOS:** a freshly downloaded `elementsd` is unsigned and gets SIGKILL-ed.
> Ad-hoc sign each binary once: `codesign -s - tools/elementsd tools/elements-cli`.

`hal-simplicity` must be on `PATH` (step 2 puts it in `~/.cargo/bin`), or set
`HAL_SIMPLICITY=/path/to/hal-simplicity`.

## Run — regtest (local, full demo)

```sh
# 1. start a local Elements regtest with the Simplicity-capable node
export ELEMENTSD_EXEC="$PWD/tools/elementsd"
./scripts/regtest.sh up           # starts elementsd + funds the treasury wallet

# 2. serve the browser wallet UI
cargo run -p mosaik-cli -- serve-wallet --port 8080

# 3. open the UI
open http://127.0.0.1:8080
```

In the UI:

1. **Fund maker** and **Fund taker** — each wallet gets 1 L-BTC plus 100 of
   each test asset (USDT, EURx).
2. **Make an offer** — pick a lock asset and want asset, set amounts, and fund
   the covenant UTXO.
3. On an offer card: **Take offer** settles it via the covenant; the red
   **Attack the covenant** buttons build fraudulent fills and show the covenant
   rejecting them; **View covenant** shows the compiled SimplicityHL source +
   the Commitment Merkle Root.

Stop everything: `./scripts/regtest.sh down` and `pkill -f 'mosaik serve'`.

### CLI (no browser)

```sh
cargo run -p mosaik-cli -- --help
cargo run -p mosaik-cli -- make-offer --amount-a 1000000 --amount-b 5000000000 \
    --asset-b <usdt-asset-id> --timeout 500 > offer.json
cargo run -p mosaik-cli -- take-offer --offer offer.json
cargo run -p mosaik-cli -- serve-relay --port 7777   # local Nostr orderbook
```

## Run — Liquid testnet (no local node)

Mirrors the [Blockstream simplicity-codespace](https://github.com/Blockstream/simplicity-codespace)
philosophy: **no local `elementsd`, no local wallet**. The public Liquid
testnet faucet funds covenant addresses directly; the public Esplora reads
chain state and broadcasts; `hal-simplicity` builds PSETs; the Mosaik CLI
compiles the Tessera covenant and assembles its witness.

**Prerequisites**

| Tool | Install |
|---|---|
| Rust toolchain | `curl https://sh.rustup.rs -sSf \| sh` |
| `hal-simplicity` | `cargo install hal-simplicity` (lands in `~/.cargo/bin`) |
| `curl`, `python3` | system package manager |

No `elementsd` and no `tools/` directory are needed for the testnet path.

### Option A — Wallet UI on port 8081 (mirrors the regtest demo)

```sh
# 1. Start the testnet UI. Reads from public Esplora; no node required.
./scripts/testnet.sh serve

# 2. Open it in a browser.
open http://127.0.0.1:8081
```

What to do in the UI:

1. **Fund maker / Fund taker** — each button hits the public faucet on the
   shown demo address (`tex1qkkx…` for the maker, `tex1q4f9…` for the taker).
   The faucet sends ~100,000 sats per request and is rate-limited; the toast
   shows the funding txid.
2. **Make an offer** — L-BTC only on testnet. Set the want-amount (default
   `0.000995` L-BTC = 99,500 sats, leaving 500 sats for the settlement fee on
   a 100k-sat faucet drop) and click **Fund covenant & publish offer**. The
   server compiles the Tessera, derives the covenant address, and **hits the
   faucet on the covenant address directly** — no maker UTXO is spent. The
   offer appears with a `faucet pending` pill.
3. **Wait ~1 min** for the faucet tx to confirm on Liquid testnet. The pill
   flips to `ready` and the **Take offer** button enables.
4. **Take offer** — the server builds a settlement PSET (`hal-simplicity
   simplicity pset create / update-input / finalize / extract`), attaches the
   SETTLE witness, and broadcasts via Esplora. The covenant is enforced by
   the network's Simplicity-capable validators.
5. **Attack the covenant** — the red buttons build deliberately-broken
   settlement PSETs (underpay, wrong recipient, hidden maker output) and
   broadcast them. The testnet validators reject each one with the
   covenant's error; the toast surfaces it. A green toast here means the
   covenant did its job.
6. **Reclaim (REFUND)** — enabled once the chain reaches `refund height`.
   Builds the refund tx in-process (`tessera::build_refund_tx` signs with the
   demo maker key) and broadcasts via Esplora.
7. **View covenant** — shows the compiled SimplicityHL source, the CMR, and
   the covenant Taproot address. Purely local; no chain query.

Stop the UI with `pkill -f 'mosaik serve-wallet'`.

### Option B — CLI demo (one Tessera, end-to-end)

This mirrors a codespace `exercises/*/demo.sh` — a single Tessera, funded by
the faucet, settled by the same shell pipeline.

```sh
# Step 1 — compile the covenant, hit the faucet, wait for confirmation, save offer.json
./scripts/testnet.sh make-offer

# Step 2 — build the settlement PSET and broadcast
./scripts/testnet.sh take-offer
```

`make-offer` compiles a Tessera with the hardcoded demo keys, derives the
Taproot covenant address, hits the public Liquid testnet faucet, polls
Esplora until the tx confirms, and writes `offer.json`.

`take-offer` reads `offer.json`, builds the PSET via `hal-simplicity`,
attaches the SETTLE witness produced by `mosaik settle-json`, extracts the
raw tx, and POSTs it to Esplora.

```sh
# Override defaults: amount_b_sats=50000, timeout=1000, offer file path
./scripts/testnet.sh make-offer 50000 1000 my-offer.json
./scripts/testnet.sh take-offer my-offer.json

# Print the hardcoded demo key constants
./scripts/testnet.sh testnet-constants
```

The Simplicity contract source lives at
[`crates/tessera/contracts/tessera.simf`](crates/tessera/contracts/tessera.simf).

### Faucet hints

The Liquid testnet faucet at `https://liquidtestnet.com/faucet` is shared
infrastructure. If a `Fund …` or `Make offer` action errors out, the faucet
is most likely rate-limiting your IP. Visit the URL in a browser to confirm,
or wait a few minutes and retry.

## References

- Simplicity docs — https://docs.simplicity-lang.org
- Liquid docs — https://docs.liquid.net
- Elements — https://github.com/ElementsProject/elements
- Simplicity codespace — https://github.com/Blockstream/simplicity-codespace
- LWK walk-through — https://www.youtube.com/watch?v=C2p4jzplg4c
- Simplicity Dev TG — @simplicity_community

## License

MIT — see [LICENSE](LICENSE).

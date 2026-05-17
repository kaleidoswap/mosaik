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
- [ ] REFUND / reclaim wired with a real maker key.
- [ ] Recursive partial-fill covenant — fund once, fill many times.

## Install

You need three things: the Rust toolchain, a **Simplicity-capable** `elementsd`,
and `hal-simplicity`.

```sh
# 1. Rust (the workspace pins its toolchain via rust-toolchain.toml)
curl https://sh.rustup.rs -sSf | sh

# 2. hal-simplicity — assembles the covenant witness for a settlement
cargo install hal-simplicity      # lands in ~/.cargo/bin

# 3. build the workspace
cargo build
```

**`elementsd` must support Simplicity.** Stock Elements can *fund* a covenant
address (a normal Taproot payment) but cannot *validate a covenant spend* —
spending a Tessera needs the Simplicity-capable node from the
[smplx / Simplicity codespace](https://github.com/Blockstream/simplicity-codespace).

> **macOS:** a freshly downloaded `elementsd` is unsigned and gets SIGKILL-ed.
> Ad-hoc sign it once: `codesign -s - /path/to/elementsd`.

## Run

```sh
# 1. start a local Elements regtest (point ELEMENTSD_EXEC at the Simplicity build)
export ELEMENTSD_EXEC=/path/to/simplicity/elementsd
./scripts/regtest.sh up           # starts elementsd + funds the treasury wallet

# 2. serve the browser wallet UI
cargo run -p mosaik-cli -- serve-wallet --port 8080

# 3. open the UI
open http://127.0.0.1:8080
```

In the UI:

1. **Fund maker** and **Fund taker** — each wallet gets L-BTC plus the two test
   assets (USDT, EURx).
2. **Make an offer** — pick any lock asset and want asset, set the amounts, and
   fund the covenant UTXO.
3. On an offer card: **Take offer** settles it via the covenant; the red
   **Attack the covenant** buttons build fraudulent fills and show the covenant
   rejecting them; **View covenant** shows the compiled SimplicityHL source +
   the Commitment Merkle Root.

`hal-simplicity` must be on `PATH`, or set `HAL_SIMPLICITY=/path/to/hal-simplicity`.

Stop everything with `./scripts/regtest.sh down` and `pkill -f 'mosaik serve'`.

### CLI

The same flow without the browser:

```sh
cargo run -p mosaik-cli -- --help
cargo run -p mosaik-cli -- make-offer --amount-a 1000000 --amount-b 5000000000 \
    --asset-b <usdt-asset-id> --timeout 500 > offer.json
cargo run -p mosaik-cli -- take-offer --offer offer.json
cargo run -p mosaik-cli -- serve-relay --port 7777   # local Nostr orderbook
```

The Simplicity contract itself is developed in
**[Blockstream/simplicity-codespace](https://github.com/Blockstream/simplicity-codespace)** —
see [`crates/tessera/contracts/tessera.simf`](crates/tessera/contracts/tessera.simf).

## References

- Simplicity docs — https://docs.simplicity-lang.org
- Liquid docs — https://docs.liquid.net
- Elements — https://github.com/ElementsProject/elements
- Simplicity codespace — https://github.com/Blockstream/simplicity-codespace
- LWK walk-through — https://www.youtube.com/watch?v=C2p4jzplg4c
- Simplicity Dev TG — @simplicity_community

## License

MIT — see [LICENSE](LICENSE).

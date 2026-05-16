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
│   ├── mosaik-core/          # PSET construction, keys, Liquid plumbing (LWK / rust-elements)
│   └── mosaik-cli/           # demo CLI `mosaik`: make-offer / take-offer / reclaim
├── docs/DESIGN.md            # full protocol + covenant spec
├── docs/hackathon.html       # hackathon plan + 3-dev work division
└── scripts/regtest.sh        # local Elements regtest harness
```

## Roadmap (hackathon)

- [ ] **Day 1** — Tessera covenant compiles in the Simplicity codespace; offer
      UTXO funded and the SETTLE path spent on Elements regtest.
- [ ] **Day 1** — REFUND path verified after timeout.
- [ ] **Day 2** — `mosaik` CLI end-to-end demo: make-offer → take-offer → confirmed.
- [ ] **Stretch** — Tessera flavours: Dutch-auction (price decays with height)
      and oracle-settled offers, from the same covenant codebase.
- [ ] **Stretch** — wire Mosaik into [kaleidoswap-maker](../kaleidoswap-maker)
      as a `pset`-venue settlement path alongside LWK LiquiDEX.

## Getting started

The Simplicity contract is developed in
**[Blockstream/simplicity-codespace](https://github.com/Blockstream/simplicity-codespace)**
(SimplicityHL compiler + tooling preinstalled — run it in-browser or in VS Code).
See [`crates/tessera/contracts/tessera.simf`](crates/tessera/contracts/tessera.simf).

For the Liquid side you need an Elements node:

```sh
# download elementsd from https://github.com/ElementsProject/elements/releases
export ELEMENTSD_EXEC=/path/to/elementsd
./scripts/regtest.sh up        # start a local Elements regtest
cargo run -p mosaik-cli -- --help
```

## References

- Simplicity docs — https://docs.simplicity-lang.org
- Liquid docs — https://docs.liquid.net
- Elements — https://github.com/ElementsProject/elements
- Simplicity codespace — https://github.com/Blockstream/simplicity-codespace
- LWK walk-through — https://www.youtube.com/watch?v=C2p4jzplg4c
- Simplicity Dev TG — @simplicity_community

## License

MIT — see [LICENSE](LICENSE).

# covenant-swap

**A swap offer that lives inside a coin.**

A trustless, permissionless asset swap on the [Liquid Network](https://docs.liquid.net),
where the trade terms are enforced by a [Simplicity](https://docs.simplicity-lang.org)
covenant — not by an order book, a maker server, or a counterparty's good behaviour.

Built for the **Blockstream Simplicity Bootcamp & Hackathon** — Blox Space, Turin,
16–17 May 2026.

---

## The idea

A maker wants to sell `amount_A` of asset **A** for `amount_B` of asset **B**.

Instead of posting an order to an exchange, the maker locks asset A into a single
Liquid UTXO controlled by a **Simplicity covenant**. That covenant inspects any
transaction trying to spend the coin and allows it **only if** the transaction
pays the maker exactly `amount_B` of asset B.

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

---

## How it works

```
        ┌─────────────────────────────────────────────┐
        │  Covenant UTXO  (holds amount_A of asset A)   │
        │                                               │
        │  SETTLE path:  spendable by ANYONE iff the    │
        │     spending tx has an output paying          │
        │     exactly amount_B of asset B to MAKER      │
        │                                               │
        │  REFUND path:  after TIMEOUT, spendable by    │
        │     MAKER's signature                         │
        └─────────────────────────────────────────────┘

  maker                                              taker
    │  1. lock asset A in the covenant UTXO            │
    │     (terms baked into the tapleaf)               │
    │ ───────────── publish offer (outpoint) ────────► │
    │                                                  │
    │            2. build a tx:                        │
    │               input  = covenant UTXO + own coins │
    │               output = amount_B asset B → maker  │
    │               output = amount_A asset A → taker  │
    │               + fee                              │
    │ ◄──────── 3. covenant verifies & tx confirms ─── │
```

The covenant's parameters (asset B id, amount B, maker script, timeout, maker
pubkey) are committed into the Taproot tapleaf, so they cannot be altered after
the offer is published.

---

## Repository layout

```
covenant-swap/
├── crates/
│   ├── covenant/             # the Simplicity covenant
│   │   ├── contracts/        #   SimplicityHL (.simf) source
│   │   └── src/              #   Rust: parameterise + compile the covenant
│   ├── swap-core/            # PSET construction, keys, Liquid plumbing (LWK / rust-elements)
│   └── swap-cli/             # demo CLI: make-offer / take-offer / reclaim
├── docs/DESIGN.md            # full protocol + covenant spec
└── scripts/regtest.sh        # local Elements regtest harness
```

## Roadmap (hackathon)

- [ ] **Day 1** — covenant draft compiles in the Simplicity codespace; offer
      UTXO funded and the SETTLE path spent on Elements regtest.
- [ ] **Day 1** — REFUND path verified after timeout.
- [ ] **Day 2** — `swap-cli` end-to-end demo: make-offer → take-offer → confirmed.
- [ ] **Stretch** — PTLC variant: point-lock the SETTLE path with an adaptor
      signature so two swap legs are unlinkable.
- [ ] **Stretch** — wire into [kaleidoswap-maker](../kaleidoswap-maker) as a
      `pset`-venue settlement path alongside LWK LiquiDEX.

## Getting started

The Simplicity contract is developed in
**[Blockstream/simplicity-codespace](https://github.com/Blockstream/simplicity-codespace)**
(SimplicityHL compiler + tooling preinstalled — run it in-browser or in VS Code).
See [`crates/covenant/contracts/swap.simf`](crates/covenant/contracts/swap.simf).

For the Liquid side you need an Elements node:

```sh
# download elementsd from https://github.com/ElementsProject/elements/releases
export ELEMENTSD_EXEC=/path/to/elementsd
./scripts/regtest.sh up        # start a local Elements regtest
cargo run -p swap-cli -- --help
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

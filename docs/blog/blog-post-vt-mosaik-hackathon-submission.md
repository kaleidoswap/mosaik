---
title: "Mosaik: Our Simplicity Hackathon Submission, a Covenant DEX on Liquid"
meta_description: "KaleidoSwap submitted Mosaik to the Blockstream Simplicity Hackathon: a covenant DEX on Liquid where the order is a coin the chain enforces"
variant: Technical
audience: Developer, Bitcoin-native
goal: Action
---

# Mosaik: Our Simplicity Hackathon Submission, a Covenant DEX on Liquid

We submitted Mosaik to the Blockstream Simplicity Hackathon. It is a covenant DEX on Liquid where the order is a coin, and the coin enforces the trade.

The KaleidoSwap team has spent the past two days at the Blockstream Simplicity Bootcamp and Hackathon in Turin. Our submission is **Mosaik**, a covenant-based Bitcoin DEX on the Liquid Network, now tagged as `v0.1.0` and open source.

The idea is one sentence. On every venue we know, an order is a promise: an exchange promises to hold your funds and settle, a market maker promises a price it can still walk away from. Mosaik removes the promise. An order is a single Liquid coin, and the coin enforces the trade by itself.

This post is the submission writeup: what Mosaik is, how the covenant works, and what you can run today.

## The order is the coin

A maker who wants to sell `amount_A` of asset A for `amount_B` of asset B does not post anything to an order book. The maker locks asset A into a single Liquid coin governed by a Simplicity covenant. We call that coin a **Tessera**, after the Roman token that was redeemed when its two halves were rejoined and fit.

The Tessera is a Taproot output on Liquid. The internal key is a NUMS point, so there is no key-path spend. The only way to move the coin is through a tapleaf, and that tapleaf is a Simplicity program parameterised with the trade terms. Because the terms live in the leaf, they live in the address. There is no maker daemon to keep online, no matching engine, and no escrow.

## Two doors, no keys

The Tessera covenant has exactly two spend paths.

**SETTLE** is spendable by anyone. The covenant reads the transaction trying to spend the coin and permits it only if one output pays the maker exactly `amount_B` of `asset_B` at the maker's script. A fill is one atomic Liquid transaction: the taker takes asset A and the maker is paid asset B, or nothing happens.

**REFUND** is keyless. After a timeout block height, anyone may sweep an unfilled Tessera, and the covenant only accepts a spend that returns the locked asset to the maker. The worst a stranger can do is pay the network fee to hand the maker their own coin back. The maker can stay offline forever.

The covenant carries four parameters in the tapleaf: the asset id the maker wants, the amount, the maker's script pubkey hash, and the timeout. There is no maker public key anywhere in it. Most covenant swap designs still sign something on one path or the other. Mosaik signs nothing. Both doors are pure transaction introspection, and the maker is just an address.

## Why Simplicity

Bitcoin Script cannot inspect the outputs of the transaction spending a coin. A covenant that enforces a trade needs exactly that: it has to read the amount, the asset, and the destination of an output it has never seen. [Simplicity](https://docs.simplicity-lang.org) provides it. The Tessera is written in SimplicityHL, compiles to a 32-byte Commitment Merkle Root, and is executed by a Simplicity-capable `elementsd` under leaf version 0xbe. The node enforces the covenant, not us.

## What we submitted

Mosaik `v0.1.0` is a working artifact, not a slide deck.

- **End-to-end on Liquid regtest.** Fund a wallet, post an offer as a maker, fill it as a taker. Every settlement and reclaim is one confirmed Elements transaction.
- **Multi-asset, both directions.** The covenant never inspects the locked asset, only the maker's counter-payment, so L-BTC for an asset, asset for L-BTC, and asset for asset all work.
- **A Nostr orderbook.** Each offer is published as an addressable Nostr event, kind 30050. Discovery is over Nostr; the chain stays the source of truth, since a taker re-derives the covenant address before filling.
- **An exchange-style browser wallet.** A single-file UI with one trading-pair filter, an active-wallet switcher for testing cross-user buys and sells, and a book seeded by several independent market makers.
- **Adversarial tests.** Mosaik ships three fraudulent fills, each a well-formed, balanced Elements transaction the mempool would accept: underpay the maker, pay the wrong address, hide the maker's output. All three are rejected by the covenant and covered by on-chain regtest integration tests.

The stack is Liquid and Elements at the base, a Simplicity-capable `elementsd` for consensus, SimplicityHL for the covenant, Rust for transaction construction, and Nostr for discovery.

## Where it goes

Mosaik today enforces firm limit orders. Next is partial fills that sweep a grid of covenant UTXOs in one transaction, a recursive vault where a fill re-locks the remainder so a maker funds once and fills many times, a hash-preimage branch that bridges a leg to Lightning and RGB, and covenant settlement over Liquid's confidential amounts.

Simplicity is live on Liquid testnet today, with mainnet activation still ahead. Mosaik is the bet for when it activates: a Bitcoin DEX where settlement, enforcement, and discovery all happen without a server, and the order is a coin that cannot break.

Clone it, run the regtest harness, and post an offer:

```sh
git clone https://github.com/kaleidoswap/mosaik
```

Mosaik is open source at [github.com/kaleidoswap/mosaik](https://github.com/kaleidoswap/mosaik). For our wider work on non-custodial Bitcoin trading across Lightning and RGB, read the [KaleidoSwap docs](https://docs.kaleidoswap.com).

## Sources

- Mosaik source and v0.1.0 release: [github.com/kaleidoswap/mosaik](https://github.com/kaleidoswap/mosaik)
- Simplicity and SimplicityHL: [docs.simplicity-lang.org](https://docs.simplicity-lang.org)
- Liquid Network: [liquid.net](https://liquid.net)
- Blockstream Simplicity Bootcamp and Hackathon, Turin, May 2026

---

*Platform recommendation: publish on the KaleidoSwap blog and cross-post to Simplicity and Liquid developer channels. The depth suits a developer and Bitcoin-native audience.*

---
title: "Covenants vs LiquiDEX: What Simplicity Adds to Trading on Liquid"
meta_description: "How covenants beat pre-signed swaps on Liquid, and the design of a serverless RFQ covenant vault built at the Blockstream Simplicity hackathon"
variant: Technical
audience: Developer, Bitcoin-native
goal: Trust
---

# Covenants vs LiquiDEX: What Simplicity Adds to Trading on Liquid

A look at how trading on Liquid works today, where a covenant beats a pre-signed atomic swap, and the serverless DEX design we are building toward.

The Liquid Network already has working atomic swaps. LiquiDEX settles a trade as a single transaction, and SideSwap puts a usable wallet and order book on top of the same idea. Trading on Liquid is not a missing feature.

So at the Blockstream Simplicity Bootcamp and Hackathon in Turin, the KaleidoSwap team asked a narrower question. If Liquid swaps already work, what does a covenant actually add? The answer shaped a project we call Mosaik, a covenant-based Bitcoin DEX where the offer itself is a coin.

This post covers what we found. How trading on Liquid works today, where a covenant beats a pre-signed swap, and the design we are heading toward: an RFQ covenant vault that gives a market maker a server's ergonomics with no server.

## Trading on Liquid today

Two mechanisms define the current landscape.

**LiquiDEX** is a two-step atomic swap. The maker pre-signs a partial transaction, one input and one output, with a signature hash flag that commits only to its own leg of the trade. The taker adds their input and output, balances the transaction, signs, and broadcasts. The result is a single confirmed transaction that moves both assets or neither. It needs no covenant and it works on mainnet now.

**SideSwap** is a Liquid wallet with trading built in. Its swaps settle as atomic Liquid transactions, the same kind of construction. Around that settlement layer, SideSwap runs a server: the order book is hosted by SideSwap, matching makers to takers happens server-side, and SideSwap often provides its own liquidity. See the [SideSwap documentation](https://docs.sideswap.io) for details.

The pattern across both is the same. **Settlement is trustless, coordination is centralized.** The server is what makes SideSwap usable, and it also lets a maker update or pull a quote instantly. That last point matters more than it looks.

A firm offer, once signed, is a free option for whoever can take it. The taker can wait, watch the market move, and take the offer only if it moved in their favor. The maker is frozen into a stale price. A central server fixes this by letting the maker cancel fast. It is a real fix, bought with a real cost: you trust the server for matching, censorship resistance, fee policy, and visibility into order flow.

## What a covenant changes

A covenant takes a different route. Instead of pre-signing a swap, the maker locks the asset into a coin whose spending rules are a program. The program inspects the transaction trying to spend the coin and permits it only if the maker is paid the agreed asset and amount. We call one such offer a Tessera.

This gives trustless settlement and trustless enforcement, with no server in the middle. It also opens a gap that a pre-signed swap cannot close.

A covenant can do several things LiquiDEX structurally cannot:

- **Partial fills with change.** A LiquiDEX proposal is all-or-nothing and needs a UTXO of the exact size. A covenant can release any amount up to a balance and return the change.
- **Fund once, quote many times.** A LiquiDEX proposal commits one specific UTXO. A stateful covenant holds a pool and serves many offers from a single funding.
- **Cheap cancellation.** A signed LiquiDEX proposal can only be cancelled by spending its UTXO, which costs a transaction. A covenant can encode a timelock expiry and a state nonce, so cancellation is fast or free.
- **Conditional logic.** LiquiDEX expresses one thing, asset A for asset B. A covenant can require an oracle attestation, a hash preimage, a price that decays with block height, or a whitelisted counterparty.
- **New instruments.** Options, automated market makers, and stateful market-making vaults are covenant constructions. A pre-signed swap cannot express any of them.

The deepest difference is about where safety comes from. LiquiDEX is safe because of what a signature commits to, which is a narrow window: one input and one output. A covenant is safe because the coin reads the whole spending transaction. That is the entire reason to build on Simplicity rather than stop at LiquiDEX.

## What the hackathon produced

Mosaik is the working result. The Tessera covenant compiles with the [SimplicityHL](https://docs.simplicity-lang.org) toolchain, both of its spend paths are execution-tested against a transaction environment, and an offer can be funded as a real coin on a Liquid regtest. The covenant is a real, tested contract, not a sketch.

For the advanced version, the reference is Blockstream's own [Simplicity DEX](https://github.com/Blockstream/simplicity-dex). It is an options venue on Liquid: collateral sits in a covenant, the position is split into tradable tokens, settlement is oracle-free and falls out of economic incentives, and contract discovery happens over Nostr. It proves the hardest parts of a covenant market already work.

For spot trading, we converged on a different design.

## The RFQ covenant vault

The maker funds one stateful covenant, a vault, with a pool of liquidity. The vault's state is a single 256-bit value committed into its address, so every state produces a different address.

From that single funding, the maker signs an unlimited number of quotes off-chain, each with a short expiry. A quote is just a signature, so quoting is free and instant. When a taker fills a quote, the transaction pays the maker, hands the bought asset to the taker, and re-forms the vault with the reduced balance. The covenant verifies all three.

The maker keeps optionality the whole time. Quotes expire in seconds, and a state nonce lets the maker cancel every outstanding quote in a single transaction. The free-option problem closes without a central server.

That is the point. The RFQ covenant vault gives a maker the same experience as SideSwap's order book, fund once, quote freely, cancel fast, with discovery over Nostr and enforcement by the coin itself. SideSwap's trading ergonomics, with no SideSwap in the middle.

## Why this matters

Covenants do not make Liquid swaps possible. LiquiDEX and SideSwap already settle trades atomically and safely. What covenants change is everything around the settlement: capital efficiency, cancellation, conditional logic, and the coordination layer.

A pre-signed swap forces a choice. Stay fully peer-to-peer and accept the free-option problem, or add a server and accept the trust. A covenant removes the choice. The coin enforces the maker's terms, a relay carries the quotes, and no server sits between the two parties.

Simplicity is live on Liquid testnet today, with mainnet activation still ahead. So a pre-signed swap is what ships in the immediate term, and the covenant vault is the bet for when Simplicity activates. The direction is clear: a Bitcoin DEX on Liquid that is programmable, capital-efficient, and serverless is the version worth building.

Mosaik is open source. Read the [Simplicity documentation](https://docs.simplicity-lang.org) to follow the language, and the [KaleidoSwap docs](https://docs.kaleidoswap.com) for our wider work on non-custodial Bitcoin trading.

## Sources

- Liquid Network: [liquid.net](https://liquid.net)
- Simplicity and SimplicityHL: [docs.simplicity-lang.org](https://docs.simplicity-lang.org)
- Blockstream Simplicity DEX: [github.com/Blockstream/simplicity-dex](https://github.com/Blockstream/simplicity-dex)
- SideSwap: [sideswap.io](https://sideswap.io)
- Blockstream Simplicity Bootcamp and Hackathon, Turin, May 2026

---

*Platform recommendation: publish on the KaleidoSwap blog and cross-post to developer channels. The technical depth suits a Simplicity and Liquid developer audience.*

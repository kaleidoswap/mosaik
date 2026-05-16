---
title: "A Liquid Swap Without a Server in the Middle"
meta_description: "What we learned building a covenant DEX on Liquid at the Blockstream Simplicity hackathon, and why it removes the server from non-custodial trading"
variant: Accessible
audience: Bitcoin-native, newcomers
goal: Trust
---

# A Liquid Swap Without a Server in the Middle

What we learned at the Blockstream Simplicity hackathon, and why a covenant can replace the company sitting between two traders.

Swapping assets on the Liquid Network already works. You can trade Liquid bitcoin for a stablecoin today, and the trade settles in a single transaction that either completes fully or does not happen at all. No counterparty can run off with your funds mid-trade.

So at the Blockstream Simplicity Bootcamp and Hackathon in Turin, the KaleidoSwap team asked a sharper question. If trading on Liquid already works, what is still missing? We built a project called Mosaik to find out, a non-custodial Bitcoin DEX where the offer to trade is itself a coin.

This is what we found.

## How trading on Liquid works now

Two things shape how people trade on Liquid today.

The first is the **atomic swap**. One trader signs half of a transaction, the other completes it, and the network confirms it as one unit. Both assets move, or neither does. This is the safe core, and it needs no trusted middleman.

The second is the **wallet and order book** built around that core. SideSwap is the best-known example. It is a Liquid wallet with trading built in, and the actual trades settle as those same atomic transactions. But the order book itself, the part that shows you prices and matches you with a counterparty, runs on SideSwap's own server.

So the picture splits in two. The settlement is trustless. The coordination is not. A company runs the order book, the matching, and often the liquidity. That server is what makes the experience smooth, and it is also a party you have to trust.

## The problem with a firm offer

There is a quieter issue too. When a trader posts a firm offer, anyone who can take it holds a free option. They can wait, watch the price move, and accept the offer only if it moved in their favor. The trader who posted it is stuck with a stale price.

A central server papers over this. Because the company controls the order book, a market maker can cancel a stale offer instantly. The fix works. It just costs you a trusted server.

## What a covenant does differently

A covenant is a coin with rules. Instead of signing an offer and hoping to cancel it later, a trader locks the asset into a coin whose own spending conditions are a small program. That program checks the transaction trying to spend the coin, and it allows the trade only if the trader is paid exactly what they asked for.

The coin enforces the deal. No server is needed to keep anyone honest, because the rule lives in the coin itself.

That unlocks things a plain signed swap cannot do:

- **Trade part of an offer.** A signed swap is all-or-nothing. A covenant can let someone take half, with the rest staying available.
- **Fund once, offer many times.** A signed swap ties up one specific coin. A covenant can hold a pool and serve many trades from it.
- **Cancel cheaply.** A covenant can carry an expiry and a built-in cancel switch, so a maker is never frozen into an old price.
- **Add conditions.** A covenant can require a deadline, a secret, a price schedule, or a specific recipient.

This is the heart of what we learned. The safety of a signed swap comes from a narrow promise made by one signature. The safety of a covenant comes from the coin reading the whole transaction. That wider view is what makes a serverless market possible.

## What we built, and where it goes

Mosaik works. The covenant compiles with the Simplicity toolchain, both of its spend paths are tested, and an offer can be funded as a real coin on a Liquid test network. Blockstream's own [Simplicity DEX](https://github.com/Blockstream/simplicity-dex) proves the harder version is possible too, an options venue with no price oracle and discovery handled over the Nostr protocol.

The design we are heading toward is an **RFQ covenant vault**. A market maker funds one covenant once. From it, they sign as many short-lived quotes as they like, and each quote costs nothing because it is just a signature. A trader fills one, the covenant pays the maker, and the vault re-forms itself with the balance reduced. The maker can cancel every outstanding quote at once whenever the market turns.

The result is the experience SideSwap gives a market maker, fund once, quote freely, cancel fast, but with no company in the middle. Quotes travel over an open relay, and the coin enforces every trade.

## Why it matters

Covenants do not make Liquid trading possible. It already is. What they remove is the server.

Until now, a non-custodial trader on Liquid faced a trade-off. Stay fully peer-to-peer and accept a clumsy experience, or use a smooth app and accept a company in the middle. A covenant ends that trade-off. The coin keeps everyone honest, an open relay carries the offers, and no one sits between the two traders.

Simplicity, the language that makes these covenants possible, is live on Liquid's test network today, with full activation still ahead. So the simpler swaps ship first, and the covenant vault is the plan for when Simplicity goes live. The destination is a Bitcoin trading venue on Liquid that is self-custodial, efficient, and free of any server.

Mosaik is open source. To follow the wider work on non-custodial Bitcoin trading, read the [KaleidoSwap documentation](https://docs.kaleidoswap.com).

## Sources

- Liquid Network: [liquid.net](https://liquid.net)
- Simplicity language: [docs.simplicity-lang.org](https://docs.simplicity-lang.org)
- Blockstream Simplicity DEX: [github.com/Blockstream/simplicity-dex](https://github.com/Blockstream/simplicity-dex)
- SideSwap: [sideswap.io](https://sideswap.io)
- Blockstream Simplicity Bootcamp and Hackathon, Turin, May 2026

---

*Platform recommendation: publish on the KaleidoSwap blog and share to Bitcoin-native communities. The framing around self-custody and removing the server suits a general Bitcoin audience.*

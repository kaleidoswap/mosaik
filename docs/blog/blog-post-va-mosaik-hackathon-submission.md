---
title: "We Submitted Mosaik to the Simplicity Hackathon: an Order That Cannot Break"
meta_description: "KaleidoSwap submitted Mosaik to the Blockstream Simplicity Hackathon, a Bitcoin DEX on Liquid where the order is a coin that enforces the trade"
variant: Accessible
audience: Bitcoin-native, General public
goal: Awareness
---

# We Submitted Mosaik to the Simplicity Hackathon: an Order That Cannot Break

A Bitcoin DEX on Liquid where the order is a coin, and the coin keeps its own word.

The KaleidoSwap team spent the past two days at the Blockstream Simplicity Bootcamp and Hackathon in Turin. Our submission is **Mosaik**, and it is now open source as version `0.1.0`.

Every time you trade, you trust something. A central exchange holds your funds and promises to settle. A market maker shows a price and can pull it the moment the market moves. The order itself is just a promise, and a promise is only as good as the party making it.

Mosaik removes that promise. It is a Bitcoin DEX on the Liquid Network where the order is not a message on a server. The order is a coin, and the coin enforces the trade by itself.

## The order is the coin

Say a maker wants to sell one asset for another. Instead of posting an order anywhere, the maker locks the asset into a single Liquid coin with a small program attached. That program is a covenant: a set of rules the coin carries that decide who is allowed to spend it.

We call one of these coins a Tessera. The name is the old Roman word for a token, the kind that was split in two so that rejoining the matching halves proved a bond. A Mosaik order works the same way. The coin is one half. A transaction that pays the maker correctly is the other. They fit, or nothing happens.

Once the maker funds the coin, the maker can walk away. There is no app to keep running, no server, and no middleman holding anything.

## Two ways to spend it

A Tessera can be spent in exactly two ways, and both are open to anyone.

The first is **settlement**. Anyone can take the offer, but only by building a transaction that pays the maker the exact asset and amount the maker asked for. The covenant reads the transaction and checks it. If the payment is short, goes to the wrong place, or is in the wrong asset, the coin refuses to move.

The second is **refund**. If nobody fills the offer, the maker needs the funds back. After a set point in time, anyone at all can sweep the coin, but the covenant only allows a sweep that sends the asset home to the maker. Even a stranger trying to interfere can do nothing worse than pay a small fee to return the maker's money.

Mosaik uses no keys and no signatures, on either path. The covenant is built entirely from the coin reading its own spending transaction. That makes it the simplest covenant we could find that still enforces a real trade.

## Why this needs Simplicity

Bitcoin's normal scripting cannot do this. It can check a signature and a timer, but it cannot look at where a transaction is sending money. A covenant that enforces a trade has to look. Simplicity, a new contract language on Liquid, can. That is the whole reason Mosaik is built on it. A Liquid node runs the covenant and rejects any cheating transaction, so the network enforces the rules.

## What we submitted

Mosaik `v0.1.0` is a working build, not a concept.

- You can fund a wallet, post an offer, and fill it, and every step is a real transaction on a Liquid test network.
- It handles many asset pairs in both directions.
- Offers are shared over Nostr, an open messaging network, so there is no order book server.
- It comes with a browser wallet where you pick a trading pair, switch between several wallets to test trades between users, and watch a live book of offers from different makers.

We also built the attacks ourselves. Mosaik includes three deliberately fraudulent trades: one that underpays the maker, one that pays the wrong address, and one that hides the maker's payment. The covenant rejects all three. That is the demo worth watching: not the lock turning for an honest trade, but the lock holding shut against a dishonest one.

## Where it goes next

Mosaik today handles firm orders. Next comes partial fills, a vault a maker can fund once and sell from many times, and bridges that connect a trade to Lightning and to RGB assets. Simplicity is live on Liquid's test network now, with the main network still ahead, and Mosaik is built for the day it arrives.

The goal is steady and clear. A Bitcoin DEX with no exchange in the middle, no matching engine, and no custody. Just an order that is a coin, and a coin that keeps its word.

Mosaik is open source at [github.com/kaleidoswap/mosaik](https://github.com/kaleidoswap/mosaik). To follow our wider work on non-custodial Bitcoin trading, read the [KaleidoSwap docs](https://docs.kaleidoswap.com).

## Sources

- Mosaik source and v0.1.0 release: [github.com/kaleidoswap/mosaik](https://github.com/kaleidoswap/mosaik)
- Simplicity: [docs.simplicity-lang.org](https://docs.simplicity-lang.org)
- Liquid Network: [liquid.net](https://liquid.net)
- Blockstream Simplicity Bootcamp and Hackathon, Turin, May 2026

---

*Platform recommendation: publish on the KaleidoSwap blog and share on Bitcoin-native social channels. The framing suits a Bitcoin-native and newcomer audience.*

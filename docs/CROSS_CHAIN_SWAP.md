# Mosaik — Cross-chain covenant bridge (Phase 8)

**Goal:** atomic asset swaps **Liquid-Tessera ↔ Arkade-banco**, with
both sides keyless on-chain, both makers able to go offline after
publishing, no LSP, no oracle.

> Liquid covenants (Simplicity introspection) and Arkade banco
> covenants (Arkade-Script introspection) are structurally the same
> primitive — they both enforce "this UTXO can only be spent by a tx
> that satisfies these constraints, anyone may publish that tx." Bind
> two such covenants to a single preimage commitment and you get an
> atomic-swap bridge in the strict sense: there's no version of the
> protocol in which one leg settles and the other doesn't.

The core platform shape (covenant DEX, `OfferKind` enum, kind-30050
relay, banco-proxy, Vite SPA) already exists. Phase 8 adds the
**coordination layer**. The two **on-chain primitive extensions** it
needs land separately (see *Primitive gaps*).

## Atomicity construction

A bridge swap is **two covenants that commit to the same
`H = SHA256(P)`** plus a courier (the relay) that announces them as a
pair.

```
   ┌──────────────────────────────────────────────────────────────┐
   │  Alice (maker)                            Bob (taker)        │
   │  ─────────────                            ─────────────      │
   │   ① picks secret P, computes H = SHA256(P)                   │
   │   ② publishes leg-A:                                         │
   │       Liquid Tessera covenant:                               │
   │       "anyone who pays X of asset_B to Alice                 │
   │        AND reveals P such that SHA256(P)=H                   │
   │        may claim asset_A locked here."                       │
   │      refund_height = T_long                                  │
   │   ③ publishes leg-B:                                         │
   │       Arkade banco covenant:                                 │
   │       "anyone who pays Y of asset_D to Alice                 │
   │        AND reveals P such that SHA256(P)=H                   │
   │        may claim asset_C locked here."                       │
   │      cancel_delay = T_short  (T_short  <  T_long − Δ)        │
   │   ④ Bob fulfils leg-B on Arkade → reveals P on-chain         │
   │   ⑤ Alice reads P from leg-B's spend, claims leg-A on Liquid │
   └──────────────────────────────────────────────────────────────┘
```

**Why atomic.** P is revealed iff some leg is spent. Whichever leg is
spent first reveals P; the second leg can then be claimed in the
window before its refund opens. There's no order in which only one leg
clears: either both happen (taker fulfils, maker claims) or neither
happens (taker walks away, both legs refund).

**Why keyless on-chain.** Both covenants are self-executing. The
counter-payment + preimage are the *witness*; no maker signature
exists or is needed at SETTLE time. Anyone who knows P and is willing
to pay can settle either leg — but only the maker has economic reason
to (they get paid).

## Timelock-asymmetry math

Let `Δ` be the **safety margin** — wall-clock time the slower leg's
spender needs to (a) observe the preimage reveal, (b) build and
broadcast its claim, (c) get it confirmed past any reorg risk.

Constraints:

| Leg | Refund opens at | Reason |
|---|---|---|
| B (shorter) | `T_short` | The taker's leg. Bob locks his counter-asset here; if Alice never claims, Bob must be able to refund. |
| A (longer) | `T_long ≥ T_short + Δ` | Alice's leg. After Bob spends leg-B and reveals P, Alice (or anyone) needs Δ to claim leg-A. |

If `T_long < T_short + Δ`: there's a race window where Bob can refund
leg-B *and* Alice can no longer beat the leg-A refund. **Don't.**

Picking Δ in practice:

- Liquid block time: ~60s. A 6-block safety margin is 360s.
- Mutinynet (signet) block time: ~30s. 6-block ≈ 180s.
- arkd batch round: ~60s.

A safe default for the platform: `Δ = 30 minutes`. Aggressive market
makers may run tighter; the orchestrator validates the constraint at
swap-creation time and refuses asymmetric configs.

## State machine

```
        ┌───────────────┐
        │  Drafted      │  maker picked terms, P generated, nothing on-chain
        └──────┬────────┘
               │ publish leg-A
               ▼
        ┌───────────────┐
        │  HalfPublished│  leg-A funded; waiting for leg-B publication
        └──────┬────────┘
               │ publish leg-B
               ▼
        ┌───────────────┐
        │  BothPublished│  swap is now atomic; relay event live
        └──────┬────────┘
               │ taker fulfils one leg (reveals P)
               ▼
        ┌───────────────┐
        │  Revealed     │  P is public; the other leg is claimable
        └──────┬────────┘
               │ second leg claimed
               ▼
        ┌───────────────┐
        │  Settled      │  terminal: both legs spent via SETTLE
        └───────────────┘

  Failure exits (each above state can move to Refunded if its refund
  deadline passes without the next transition):

        ┌───────────────┐
        │  Refunded     │  terminal: at least one leg refunded
        └───────────────┘
```

The state machine is enforced in `mosaik-bridge` (see
`crates/mosaik-bridge/src/state.rs`). Invalid transitions are
typed-out, not runtime checks.

## Failure modes

| Scenario | Outcome | Why |
|---|---|---|
| Maker publishes leg-A, never leg-B. | Maker reclaims leg-A at `T_long`. No one else has lost anything. | Leg-A's covenant accepts a refund spend that returns the locked asset to the maker; banco's pattern is symmetric. |
| Both legs published; no taker arrives. | Both legs refund (B first at `T_short`, A at `T_long`). | Same as above on each chain independently. |
| Taker fulfils leg-B; Alice's claim of leg-A fails (e.g. she's offline). | After `T_long`, leg-A refunds to Alice. Bob keeps the asset he claimed from leg-A? **No** — Bob claimed leg-B in this scenario. He got Alice's asset_C; Alice didn't get Bob's asset_D. **Alice's loss, by Alice's negligence.** | Alice's failure to claim leg-A within `T_long − T_short` is on Alice. The protocol is atomic up to liveness; liveness is the maker's job. **Mitigation:** the maker's claim is keyless, so anyone (including a watch-tower service) can complete it for a tip. |
| Taker fulfils leg-A first instead of leg-B. | Symmetric: P revealed on leg-A's chain; Alice (or anyone) claims leg-B. | Both legs are interchangeable as the "fulfilment side." |
| Reorg deeper than `Δ` on the slower chain. | Worst case: settlement reverts and a refund opens later. The locked assets eventually return to their original owners. No double-spend gives a counterparty more than they should have. | Both covenants reject any spend that doesn't satisfy them; a reorg can revert a SETTLE but not turn it into a different settlement. |
| Banco covenant cosign by introspector withheld. | Affected leg blocks; refund eventually fires. | Introspector is a liveness cosigner. Run your own; consider a multi-introspector deployment for redundancy. |

## Primitive gaps (the work this design depends on)

Phase 8 ships the **coordination layer**. Two on-chain primitive
extensions remain:

### Gap A — `tessera-hashlock.simf`

Today's `crates/tessera/contracts/tessera.simf` enforces "output 0 pays
the maker exactly X of asset_B." It does **not** require revealing a
preimage. A bridge-capable Tessera variant adds one branch:

```
settle_with_hashlock(vout, preimage):
    require check_lock_height(0)               # no timelock on this branch
    require sha256(preimage) == HASHLOCK       # parameter at compile time
    require output_amount(vout)  == AMOUNT_B
    require output_asset(vout)   == ASSET_B
    require output_script_hash(vout) == MAKER_SPK_HASH
```

The existing settle / refund branches stay; the third branch only
activates when a preimage is supplied. Refund is unchanged
(`check_lock_height(T_long)` + return-to-maker introspection).

`HASHLOCK` becomes a 32-byte template parameter alongside `asset_b`,
`amount_b`, `maker_spk_hash`, `timeout`. The Rust `Tessera` struct
grows `Option<[u8;32]>` (None = legacy non-bridge offers).

**Estimated effort:** 1–2 days of Simplicity work + tests against the
existing `hal-simplicity` harness.

### Gap B — banco preimage-gated claim path

Upstream `arkade-os/bancod` ships a `pkg/preimage` plugin: preimage-
gated VTXO claims, ECIES-encrypted, stateless. The banco *swap*
primitive (the covenant VTXO) does not by default require a preimage
at fulfilment.

The bridge-capable banco offer composes the existing banco covenant
with a preimage tap leaf: a fulfilment spend must include both
- the standard banco fulfilment satisfaction, **and**
- a SHA-256 preimage matching the commitment in the covenant terms.

`@arkade-os/banco` doesn't expose this composition publicly today.
The work splits into:

1. Compose preimage with banco covenant (TS / SDK work, possibly a
   small upstream PR — banco's covenant builder is parameterised).
2. Surface a `preimageHash` field on `POST /banco/maker/offer` in
   `services/banco-proxy` and pass it through to the SDK.
3. Surface the **revealed** preimage in `/banco/maker/offers` or a new
   `/banco/offers/{txid}/preimage` endpoint so the cross-chain
   orchestrator can watch for it.

**Estimated effort:** 3–5 days, gated on upstream banco maturity.

## What ships in Phase 8

1. **`crates/mosaik-bridge`** — coordination layer:
   - `Preimage` / `PreimageHash` types (32-byte SHA-256).
   - `SwapEnvelope` — the addressable swap record, references both
     `Offer`s via their relay event ids + funding txids.
   - `SwapState` exhaustively-typed state machine (no string states;
     invalid transitions are compile errors).
   - `BridgeOrchestrator` traits + a `MockOrchestrator` test
     implementation that walks the full state machine in-memory.
   - Real-orchestrator stub that calls `mosaik-core` for the Liquid
     leg and `mosaik-banco` for the banco leg; the two SETTLE-with-
     preimage entry points return a clean `PrimitiveGap` error
     pointing at the gap docs.
2. **`mosaik-relay`** — `CROSS_CHAIN_SWAP_KIND = 30052` addressable
   event with the `SwapEnvelope` JSON in `content` and filterable
   tags (`backends`, `pair`, `expiry`, `state`).
3. **`mosaik-cli`** — `bridge-make` / `bridge-take` / `bridge-claim`
   / `bridge-status` subcommands.
4. **`docs/PROTOCOL.md`** — kind-30052 section.
5. **`docs/CROSS_CHAIN_SWAP.md`** — this file.

When the two primitive gaps land, the orchestrator's stub becomes
real with **zero additional API design** — the types and the state
machine are already correct.

## A worked example (numbers)

- Pair: L-BTC (Liquid) ↔ BTC (Arkade).
- Amount: 100 000 sats L-BTC ⇄ 99 500 sats BTC-on-Ark. (500 sat spread
  to the maker.)
- Δ: 30 minutes = 1800 s.
- `T_short`: now + 1 h = `now + 3600`.
- `T_long`: now + 2 h = `now + 7200`. (Slack: 1800 s.)

1. Alice runs `mosaik bridge-make --liquid-amount 100000
   --banco-amount 99500 --pair LBTC/BTC` → orchestrator generates P,
   prints H, publishes leg-A (Liquid Tessera with hashlock H, refund
   `T_long`), then leg-B (banco offer with hashlock H, cancel-delay
   `T_short`), then publishes a kind-30052 envelope to the relay.
2. Bob runs `mosaik bridge-take --offer <envelope.json>` → fulfils
   leg-B on Arkade with preimage `P_Bob` (which equals P, learned
   from the swap envelope — wait, no: Bob doesn't know P. Alice does.)

Hold on. Re-reading the construction: **Alice picks P; Bob doesn't
learn P until Alice (or anyone) reveals it by claiming.** So the
order is:

- Bob fulfils leg-B by *providing the counter-payment* — that's the
  banco fulfilment standard path. The preimage doesn't gate this leg
  at all. Wait, that's wrong too.

Re-thinking: the preimage MUST gate the fulfilment branch, not the
counter-payment. Otherwise atomicity breaks.

Actually the standard atomic-swap construction is: **the receiver of
the swapped asset reveals the preimage when claiming.** Mapped onto
covenants:

- Leg-A locks Alice's asset_A on Liquid. *Bob* claims asset_A by
  paying Alice asset_B *and revealing P*.
- Leg-B locks Alice's asset_C on Arkade. *Bob* claims asset_C by
  paying Alice asset_D *and revealing P*.

But Alice owns both legs. Why would she lock two of her own assets?

The standard cross-chain swap is between two parties holding
different assets:

- **Alice** has L-BTC on Liquid, wants BTC-on-Ark.
- **Bob** has BTC-on-Ark, wants L-BTC.

Then:

- *Alice* picks P, computes H. Locks L-BTC on Liquid with hashlock H,
  refund `T_long`. Claim path: any spender pays Bob something? No —
  it's hers, she's giving it away. The covenant just enforces
  *destination*: "this UTXO can be claimed by revealing P; the asset
  goes to the address committed in the covenant, which is **Bob's**."
- *Bob* sees H on the relay. Locks BTC-on-Ark with hashlock H, refund
  `T_short`. Claim path: any spender reveals P; asset goes to
  **Alice's** address (committed in this covenant).
- Alice now claims Bob's BTC-on-Ark by revealing P (she has it). P
  appears on Arkade.
- Bob reads P from Arkade, claims Alice's L-BTC.

Both covenants then have a **single SETTLE branch** parameterised
with `(receiver_address, hashlock_H)`. The counter-payment isn't part
of the covenant — that's enforced by the swap *symmetry*: Alice
funds the leg paying Bob, Bob funds the leg paying Alice. Each leg
is a one-way payment whose claim requires the shared preimage.

OK — that's the correct construction. Let me rewrite the worked
example.

### Worked example (corrected)

- Alice has 100 000 sat L-BTC, wants 99 500 sat BTC-on-Ark.
- Bob has 99 500 sat BTC-on-Ark, wants 100 000 sat L-BTC.

1. Alice picks `P`, `H = SHA256(P)`.
2. Alice publishes **leg-A**: Liquid covenant locking 100 000 sat
   L-BTC, claimable by revealing P with payout to Bob's L-BTC address.
   Refund at `T_long` returns L-BTC to Alice.
3. Bob sees the relay event, picks Alice's terms, publishes **leg-B**:
   Arkade banco covenant locking 99 500 sat BTC-on-Ark, claimable by
   revealing the same H with payout to Alice's Ark address. Refund at
   `T_short` returns BTC-on-Ark to Bob.
4. Alice (who knows P) claims leg-B on Arkade → reveals P on-chain →
   receives 99 500 sat BTC-on-Ark.
5. Bob reads P from leg-B's spending tx, claims leg-A on Liquid →
   receives 100 000 sat L-BTC.

Both happen or neither. Both makers / takers can go offline after
publishing their own leg; the **claim is keyless** (the covenant only
checks preimage + payout destination), so anyone with P and willing to
pay fees can complete a leg — Alice / Bob just have the strongest
economic incentive.

## Open questions for v1.1

- **Watchtower role.** The "anyone may complete a leg" property means
  third parties can be paid to do completion-with-preimage for a fee.
  Is this its own role on the relay (new kind?), or just a market that
  emerges?
- **MEV-resistance of P-reveal.** On Arkade, once P is in the mempool
  but unconfirmed, can a bot front-run Alice's leg-A claim to seize
  the L-BTC? Almost certainly **no**, because leg-A's payout address
  is committed in the covenant — only Bob can receive it. But worth a
  formal write-up.
- **Partial fills across legs.** banco supports partial fills; Liquid
  Tessera does not (today). A bridge swap is all-or-nothing as a
  result. Asymmetric partial fills are out of scope for v1.

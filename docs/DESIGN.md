# covenant-swap — Design

## 1. Goal

A swap where the offer is a single Liquid UTXO and the trade terms are enforced
by a Simplicity covenant. No order book, no maker server, no counterparty trust.

## 2. Roles & terms

- **Maker** — creates the offer. Sells `amount_A` of `asset_A`, wants `amount_B`
  of `asset_B` delivered to `maker_spk` (a script pubkey it controls).
- **Taker** — anyone. Fills the offer by spending the covenant UTXO.

Offer terms, all committed into the covenant tapleaf so they are immutable once
published:

| Term | Meaning |
|---|---|
| `asset_B` | asset id (32 bytes) the maker wants |
| `amount_B` | exact amount of `asset_B` the maker must receive |
| `maker_spk` | script pubkey hash the counter-payment must go to |
| `timeout` | block height after which the maker may reclaim |
| `maker_pk` | maker BIP-340 key for the refund path |

`asset_A` / `amount_A` are simply whatever the maker funds the UTXO with.

## 3. The covenant UTXO

The maker builds a Taproot output on Liquid:

- **Internal key** — a NUMS point (no key-path spend), so spends must use a leaf.
- **Tapleaf** — a Simplicity program (`swap.simf`) parameterised with the terms
  above. Because the terms are part of the leaf, they are part of the address;
  changing any term changes the UTXO.

Two spending paths, both expressed in the one Simplicity program:

### 3.1 SETTLE path (anyone)

Valid iff the spending transaction contains an output `j` such that:

```
output[j].asset  == asset_B
output[j].amount == amount_B
output[j].script == maker_spk
```

The Simplicity program introspects the spending transaction, finds (or is told,
via witness, the index of) that output, and asserts the three equalities. No
signature is required on this path — the covenant alone is the authorisation.
The taker spends asset_A to wherever it likes in another output; the covenant
does not care, because the maker is already made whole.

### 3.2 REFUND path (maker only)

Valid iff:

```
nLockTime / height >= timeout
BIP-340 verify(maker_pk, sighash) == true
```

Lets the maker reclaim an unfilled offer.

## 4. Simplicity program shape

`swap.simf` (SimplicityHL) — see `crates/covenant/contracts/swap.simf`.
Pseudocode:

```
fn settle() {
    let j = witness::SETTLE_OUTPUT_INDEX;
    assert!(jet::output_asset(j)  matches explicit asset_B);
    assert!(jet::output_amount(j) matches explicit amount_B);
    assert!(jet::output_script_hash(j) == maker_spk_hash);
}

fn refund() {
    assert!(jet::check_lock_height(timeout));
    jet::bip_0340_verify((maker_pk, jet::sig_all_hash()), witness::REFUND_SIG);
}

fn main() {
    match witness::PATH {
        Settle => settle(),
        Refund => refund(),
    }
}
```

Introspection jets (`output_asset`, `output_amount`, `output_script_hash`,
`check_lock_height`, `bip_0340_verify`, `sig_all_hash`) are the Simplicity-native
core. **Exact jet names and signatures must be verified against the SimplicityHL
compiler in the codespace** — treat the `.simf` file as a draft until it compiles.

A subtlety on confidential outputs: on Liquid, amounts/assets can be _blinded_.
For the hackathon the counter-payment output is **explicit (unblinded)** so the
covenant can read it directly. A confidential variant (covenant checks a
rangeproof / surjection commitment) is a follow-up, noted in the roadmap.

## 5. Transaction flow

```
make-offer (maker):
    pick a UTXO of asset_A
    derive covenant address from the terms
    send asset_A → covenant address
    publish the resulting outpoint (the "offer")

take-offer (taker):
    build tx:
        input  0: covenant UTXO        (asset_A, amount_A)
        input  1..: taker's coins      (asset_B, ≥ amount_B + fee)
        output 0: amount_B asset_B → maker_spk      [SETTLE target]
        output 1: amount_A asset_A → taker          [the asset taker bought]
        output 2: change + fee
    set witness: PATH=Settle, SETTLE_OUTPUT_INDEX=0
    finalize the Simplicity input, sign the taker inputs, broadcast

reclaim (maker, after timeout):
    build tx spending the covenant UTXO back to itself
    set witness: PATH=Refund, REFUND_SIG=sign(maker_pk)
    nLockTime = timeout, broadcast
```

Atomicity is structural: it is one transaction. The taker cannot construct a
valid transaction that omits the maker's output, because the covenant rejects it.

## 6. Stretch: PTLC variant

Replace the SETTLE path's "anyone" authorisation with a point lock: require a
BIP-340 signature whose completion reveals a secret scalar `t` (adaptor
signature). The same `t` unlocks a second leg on another chain, so a two-leg
swap settles atomically while the two legs share no visible hash — unlinkable,
unlike an HTLC. The adaptor work is off-chain (`secp256k1-zkp`); the covenant
just adds `bip_0340_verify`. No Lightning node / LDK required.

## 7. Threat notes

- **Terms immutability** — guaranteed by committing terms in the tapleaf.
- **Output-index lying** — the witness supplies the SETTLE output index for
  efficiency; the covenant still verifies asset+amount+script at that index, so
  a wrong index simply fails the asserts.
- **Pinning / fee griefing** — the taker builds and pays the fee; standard
  Liquid fee handling applies. Out of scope for the hackathon demo.
- **Confidential outputs** — explicit counter-payment for v1 (see §4).

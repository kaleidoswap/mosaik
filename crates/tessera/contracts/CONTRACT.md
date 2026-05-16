# Tessera covenant — contract notes

Companion to [`tessera.simf`](tessera.simf). Everything Track A needs to take
the covenant from draft to compiling-and-tested on Day 1.

## 1. What the program does

One Liquid UTXO, holding asset A, with two spend paths selected by `witness::PATH`
(an `Either`):

| `PATH` | Path | Authorised by | Checks |
|---|---|---|---|
| `Left(u32)` | **SETTLE** | anyone | the output at the given index pays `AMOUNT_B` of `ASSET_B` to `MAKER_SPK` |
| `Right(Signature)` | **REFUND** | the maker | lock-height ≥ `TIMEOUT`, and a BIP-340 sig under `MAKER_PK` |

SETTLE needs no signature — the covenant *is* the authorisation. The maker's
terms are enforced by the coin, so any taker can fill the offer and none can
cheat.

## 2. Jet reference

Every jet the covenant uses. ✅ = seen in an upstream SimplicityHL example
(`htlc`, `ctv`, `escrow_with_delay`, `non_interactive_fee_bump`); ⚠️ = name/
signature plausible but **confirm in the codespace**.

| Jet | Signature | Used for | |
|---|---|---|---|
| `sig_all_hash` | `() -> u256` | the sighash for `checksig` | ✅ |
| `bip_0340_verify` | `((Pubkey, u256), Signature) -> ()` | verify the maker signature | ✅ |
| `check_lock_height` | `(Height) -> ()` | REFUND timelock (absolute height) | ✅ |
| `eq_256` | `(u256, u256) -> bool` | compare asset / script / pubkey | ✅ |
| `output_amount` | `(u32) -> Option<(Asset1, Amount1)>` | read settle output asset+amount | ⚠️ |
| `output_script_hash` | `(u32) -> Option<u256>` | read settle output scriptPubKey hash | ⚠️ |
| `eq_64` | `(u64, u64) -> bool` | compare the amount | ⚠️ |

Note: `check_lock_height` is **absolute** (CLTV-like). `check_lock_distance`
in `escrow_with_delay.simf` is *relative* (CSV-like) — Tessera wants absolute,
so `check_lock_height` is correct.

## 3. Day-1 verification checklist

Resolve these against the SimplicityHL compiler in
[`Blockstream/simplicity-codespace`](https://github.com/Blockstream/simplicity-codespace).
Listed worst-unknown first.

1. **Confidential-or-explicit unwrap (the one real unknown).**
   `output_amount` returns `Option<(Asset1, Amount1)>`. An Elements output's
   asset and amount can be *blinded*. Confirm:
   - the exact type names — is it `Asset1` / `Amount1`? `ExplicitAsset` is real
     (it appears in `non_interactive_fee_bump.simf`).
   - the explicit arm — is it `Right(explicit)` and is the explicit amount a
     bare `u64`?
   - the blinded arm's type (the draft calls it `Confidential1`).

   The draft isolates this in the two `match out_asset` / `match out_amount`
   blocks in `settle`. Fixing those is milestone 1.

   *Fallback if the unwrap is awkward:* commit to the maker output by hash
   instead — `match jet::output_hash(settle_vout) { Some(h) => eq_256(h, EXPECTED), .. }`
   (the `ctv.simf` pattern). One comparison, no confidential/explicit handling;
   `EXPECTED` is computed off-chain by the `tessera` crate.

2. **`eq_64`** — confirm the 64-bit equality jet name (`eq_8`/`eq_256` are
   confirmed; `eq_64` should exist by analogy).

3. **`match` as a statement.** The draft uses `match` blocks whose arms return
   `()` as statements. Upstream examples mostly use `match` as an *expression*.
   If the statement form is rejected, bind it: `let _: () = match ... ;`.

4. **Module-scope constants.** SimplicityHL examples hardcode literals *inside*
   functions; the draft follows that (terms are inline, tagged
   `TESSERA_PARAM:*`). If module-scope `const` is supported, hoisting the five
   terms to the top is cleaner — but not required.

## 4. The five terms

Inline literals tagged `// TESSERA_PARAM:<NAME>`. The `tessera` Rust crate's
`Tessera::render()` rewrites the value on each tagged line per offer.

| Name | Type | Literal form | Meaning |
|---|---|---|---|
| `ASSET_B` | `ExplicitAsset` | `0x…` (32 bytes) | asset the maker wants |
| `AMOUNT_B` | `u64` | decimal | exact amount of it |
| `MAKER_SPK` | `u256` | `0x…` (32 bytes) | SHA-256 of the maker scriptPubKey |
| `TIMEOUT` | `Height` | decimal | refund unlock height |
| `MAKER_PK` | `Pubkey` | `0x…` (32 bytes) | maker BIP-340 x-only key |

The draft ships with zero placeholders, so it is a valid *template*, not a
live covenant — render it before compiling for a real offer.

## 5. Witness files (`.wit`)

SimplicityHL takes witness data as a JSON `.wit` file. `PATH` is an `Either`;
`Left` = settle, `Right` = refund. Samples in this directory:

- [`tessera.settle.wit`](tessera.settle.wit) — fill an offer; `Left` carries the
  maker-output index (`u32`).
- [`tessera.refund.wit`](tessera.refund.wit) — reclaim an offer; `Right` carries
  the maker's `Signature`.

⚠️ The `.wit` value syntax for an `Either` (`"Left(0)"` / `"Right(0x…)"`) is a
best guess — the upstream docs only show `u32`/`bool`. Confirm the sum-type
encoding in the codespace and fix the samples if needed.

## 6. Compiling

In the codespace:

```sh
# render a concrete covenant for some terms, then compile it
simc tessera.simf --witness tessera.settle.wit
```

Adjust to the codespace's actual entry point. The output to capture is the
program's **Commitment Merkle Root (CMR)** — that 32-byte value is what the
`tessera` crate puts into the Taproot tapleaf, and what `mosaik-core` needs to
derive the offer address.

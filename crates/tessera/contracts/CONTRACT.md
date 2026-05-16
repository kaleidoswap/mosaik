# Tessera covenant — contract notes

Companion to [`tessera.simf`](tessera.simf).

**Status: the covenant compiles.** It builds with the `simplicityhl` compiler
(crate `simplicityhl 0.6.0-rc.0`) and yields a Commitment Merkle Root. The
`tessera` crate's `covenant_compiles_and_yields_a_cmr` test enforces this on
every `cargo test`, and `mosaik compile-tessera …` prints the CMR. What remains
for Track A is *execution* testing — satisfying the covenant with witness data
and running it against a transaction environment (see §3).

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

Every jet the covenant uses — all confirmed to compile against
`simplicityhl 0.6.0-rc.0`.

| Jet | Signature | Used for |
|---|---|---|
| `sig_all_hash` | `() -> u256` | the sighash for `checksig` |
| `bip_0340_verify` | `((Pubkey, u256), Signature) -> ()` | verify the maker signature |
| `check_lock_height` | `(Height) -> ()` | REFUND timelock (absolute height) |
| `eq_256` | `(u256, u256) -> bool` | compare asset / script / pubkey |
| `eq_64` | `(u64, u64) -> bool` | compare the amount |
| `output_amount` | `(u32) -> Option<(Asset1, Amount1)>` | settle output asset+amount |
| `output_script_hash` | `(u32) -> Option<u256>` | settle output scriptPubKey hash |

`check_lock_height` is **absolute** (CLTV-like); `check_lock_distance` would be
relative (CSV-like) — Tessera wants absolute, so `check_lock_height` is correct.

The confidential-or-explicit output types are `Asset1` / `Amount1` — each an
`Either<Confidential1, Explicit>`: the `Right` arm is explicit (`ExplicitAsset`,
or a bare `u64` for the amount), the `Left` arm is blinded. The covenant takes
the `Right` arm and `panic!`s on `Left`, which enforces the v1 rule that the
maker's counter-payment output must be unblinded.

## 3. Execution testing

Compilation proves the program is well-typed and every jet exists. *Execution*
proves the spend logic is correct. `crates/tessera/tests/execution.rs` does this
for **SETTLE** and it passes:

- compiles the covenant, satisfies it with `PATH = Left(0)`,
- builds a transaction whose output 0 explicitly pays the maker,
- runs the covenant in `simplicity::BitMachine` against
  `dummy_env::dummy_with_tx`,
- asserts SETTLE **accepts** a correct payment and **rejects** both an
  underpayment and a payment to the wrong scriptPubKey.

So the introspection logic — asset, amount and destination checks — is verified
end to end.

**Still open — REFUND execution.** REFUND needs a real BIP-340 signature over
the Simplicity spend sighash. Compute it with
`simplicity::policy::sighash::SighashCache::simplicity_spend_signature_hash`
(input index 0, the prevouts, the covenant CMR as `script_cmr`, the control
block from `dummy_with_tx`, an all-zero genesis hash), then Schnorr-sign that
hash with the maker key and pass it as `PATH = Right(0x…)`. The upstream
`htlc.simf` example proves this timelock-plus-signature pattern executes, so
this is plumbing, not a design risk.

**Cross-check the CMR** — `mosaik compile-tessera` should yield the same CMR as
the `simc` toolchain in the Simplicity codespace for identical terms.

A note that did *not* hold up: bare `match` statements are rejected by the
grammar — `match` must be an expression (a function body, or `let`-bound). The
covenant therefore puts each output check in its own helper function whose body
*is* the `match`.

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

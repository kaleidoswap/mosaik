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

## 3. Remaining Track-A work — execution testing

Compilation proves the program is well-typed and every jet exists. It does
**not** prove the spend logic behaves correctly at spend time. To finish:

1. **Satisfy the covenant with witness data.** Use `CompiledProgram::satisfy`
   with a `WitnessValues` map setting `PATH` to `Left(vout)` (settle) or
   `Right(sig)` (refund). A successful `satisfy` confirms the witness layout.

2. **Run against a transaction environment.** Use `satisfy_with_env` /
   `simplicityhl::dummy_env`, or the `test_utils::TestCase` helper
   (`program_text` → `with_witness_values` → `with_lock_time` →
   `assert_run_success`). Craft an environment whose output 0 carries
   `ASSET_B` / `AMOUNT_B` / `MAKER_SPK` and assert SETTLE succeeds; mutate it
   and assert it fails. For REFUND, set the lock height past `TIMEOUT` and
   supply a valid maker signature.

3. **Cross-check the CMR with the codespace.** `mosaik compile-tessera` should
   yield the same CMR as the `simc` toolchain in the Simplicity codespace for
   identical terms.

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

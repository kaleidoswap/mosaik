//! Execution tests for the Tessera covenant's SETTLE path.
//!
//! These go beyond "does it compile": they satisfy the covenant with witness
//! data and run it against a Liquid transaction environment, asserting that
//! SETTLE accepts a transaction that pays the maker and rejects one that does
//! not. The keyless REFUND path is covered by `keyless_spike.rs`.

use sha2::{Digest, Sha256};
use simplicity::BitMachine;
use simplicityhl::{elements, Arguments, CompiledProgram, WitnessValues};
use tessera::Tessera;

const MAKER_AMOUNT: u64 = 50_000;

/// SHA-256 of an empty scriptPubKey — what the covenant's `output_script_hash`
/// jet returns for `Script::default()`.
fn empty_script_hash() -> [u8; 32] {
    Sha256::digest(elements::Script::default().as_bytes()).into()
}

/// A Tessera whose terms match the maker output built by `tx_paying_maker`.
fn matching_tessera() -> Tessera {
    Tessera {
        asset_b: [0u8; 32], // elements::AssetId::default()
        amount_b: MAKER_AMOUNT,
        maker_spk_hash: empty_script_hash(),
        timeout: 0,
    }
}

/// A transaction whose output 0 explicitly pays `amount` of the default asset
/// to an empty scriptPubKey — i.e. the maker's counter-payment.
fn tx_paying_maker(amount: u64) -> elements::Transaction {
    elements::Transaction {
        version: 2,
        lock_time: elements::LockTime::ZERO,
        input: vec![elements::TxIn {
            previous_output: elements::OutPoint::default(),
            is_pegin: false,
            script_sig: elements::Script::new(),
            sequence: elements::Sequence::MAX,
            asset_issuance: elements::AssetIssuance::default(),
            witness: elements::TxInWitness::default(),
        }],
        output: vec![elements::TxOut {
            asset: elements::confidential::Asset::Explicit(elements::AssetId::default()),
            value: elements::confidential::Value::Explicit(amount),
            nonce: elements::confidential::Nonce::Null,
            script_pubkey: elements::Script::default(),
            witness: elements::TxOutWitness::default(),
        }],
    }
}

/// Compile `tessera`, satisfy it on the SETTLE path, and run it against a
/// transaction whose output 0 pays `output_amount`. Returns `Ok` iff the
/// covenant accepts the spend.
fn run_settle(tessera: &Tessera, output_amount: u64) -> Result<(), String> {
    let compiled = CompiledProgram::new(tessera.render(), Arguments::default(), false)
        .map_err(|e| format!("compile: {e}"))?;

    // SETTLE: PATH = Left(0) — the maker output is at index 0.
    let witness: WitnessValues = serde_json::from_str(
        r#"{ "PATH": { "value": "Left(0)", "type": "Either<u32, u32>" } }"#,
    )
    .map_err(|e| format!("witness: {e}"))?;

    let satisfied = compiled.satisfy(witness).map_err(|e| format!("satisfy: {e}"))?;

    let env = simplicityhl::dummy_env::dummy_with_tx(tx_paying_maker(output_amount));
    let pruned = satisfied
        .redeem()
        .prune(&env)
        .map_err(|e| format!("prune: {e}"))?;
    let mut mac = BitMachine::for_program(&pruned).map_err(|e| format!("machine: {e}"))?;
    mac.exec(&pruned, &env).map(|_| ()).map_err(|e| format!("run: {e}"))
}

#[test]
fn settle_accepts_a_transaction_that_pays_the_maker() {
    run_settle(&matching_tessera(), MAKER_AMOUNT)
        .expect("SETTLE must accept a correct payment");
}

#[test]
fn settle_rejects_a_transaction_that_underpays() {
    let result = run_settle(&matching_tessera(), MAKER_AMOUNT - 1);
    assert!(result.is_err(), "SETTLE must reject an underpaying transaction, got {result:?}");
}

#[test]
fn settle_rejects_payment_to_the_wrong_destination() {
    let mut tessera = matching_tessera();
    tessera.maker_spk_hash = [0xff; 32];
    let result = run_settle(&tessera, MAKER_AMOUNT);
    assert!(result.is_err(), "SETTLE must reject payment to the wrong script, got {result:?}");
}

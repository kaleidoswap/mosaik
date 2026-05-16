//! Execution tests for the Tessera covenant.
//!
//! These go beyond "does it compile": they satisfy the covenant with witness
//! data and run it against a Liquid transaction environment, asserting that
//! the SETTLE path accepts a transaction that pays the maker and rejects one
//! that does not.
//!
//! The environment is built with `simplicityhl::dummy_env::dummy_with_tx`, so
//! the test crafts a transaction whose output 0 carries an explicit asset,
//! amount and scriptPubKey, and a `Tessera` whose terms match it.

use secp256k1::{Keypair, Message, Secp256k1, SecretKey};
use sha2::{Digest, Sha256};
use simplicity::hashes::Hash as _;
use simplicity::BitMachine;
use simplicityhl::{elements, Arguments, CompiledProgram, WitnessValues};
use tessera::Tessera;

const MAKER_AMOUNT: u64 = 50_000;

/// A fixed maker secret key, so the tests are deterministic.
const MAKER_SECRET: [u8; 32] = [7u8; 32];

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
        maker_pk: [0u8; 32],
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
        r#"{ "PATH": { "value": "Left(0)", "type": "Either<u32, Signature>" } }"#,
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
    let tessera = matching_tessera();
    run_settle(&tessera, MAKER_AMOUNT).expect("SETTLE must accept a correct payment");
}

#[test]
fn settle_rejects_a_transaction_that_underpays() {
    let tessera = matching_tessera();
    // Output pays one unit less than the covenant demands.
    let result = run_settle(&tessera, MAKER_AMOUNT - 1);
    assert!(
        result.is_err(),
        "SETTLE must reject an underpaying transaction, got {result:?}"
    );
}

#[test]
fn settle_rejects_payment_to_the_wrong_destination() {
    let mut tessera = matching_tessera();
    // The covenant now expects a different scriptPubKey than the tx pays to.
    tessera.maker_spk_hash = [0xff; 32];
    let result = run_settle(&tessera, MAKER_AMOUNT);
    assert!(
        result.is_err(),
        "SETTLE must reject payment to the wrong script, got {result:?}"
    );
}

// ── REFUND path ─────────────────────────────────────────────────────────────

/// The maker keypair for `MAKER_SECRET`.
fn maker_keypair() -> Keypair {
    Keypair::from_secret_key(
        &Secp256k1::new(),
        &SecretKey::from_slice(&MAKER_SECRET).expect("valid secret"),
    )
}

/// A Tessera whose REFUND key is the maker keypair, refundable at `timeout`.
fn refundable_tessera(timeout: u32) -> Tessera {
    Tessera {
        asset_b: [0u8; 32],
        amount_b: MAKER_AMOUNT,
        maker_spk_hash: empty_script_hash(),
        timeout,
        maker_pk: maker_keypair().x_only_public_key().0.serialize(),
    }
}

/// Compile `tessera`, satisfy it on the REFUND path with a signature from
/// `signer`, and run it against a transaction whose lock height is
/// `tx_lock_height`. Returns `Ok` iff the covenant accepts the refund.
fn run_refund(tessera: &Tessera, tx_lock_height: u32, signer: &Keypair) -> Result<(), String> {
    let compiled = CompiledProgram::new(tessera.render(), Arguments::default(), false)
        .map_err(|e| format!("compile: {e}"))?;

    // A transaction at the given lock height, with locktime enabled.
    let env = simplicityhl::dummy_env::dummy_with(
        elements::LockTime::Blocks(
            elements::locktime::Height::from_consensus(tx_lock_height).expect("valid height"),
        ),
        elements::Sequence::ENABLE_LOCKTIME_NO_RBF,
        false,
    );

    // Sign the exact sighash the covenant's `jet::sig_all_hash` will see.
    let sighash = env.c_tx_env().sighash_all();
    let msg = Message::from_digest(sighash.to_byte_array());
    let sig = Secp256k1::new().sign_schnorr_no_aux_rand(&msg, signer);

    // REFUND: PATH = Right(signature).
    let wit_json = format!(
        r#"{{ "PATH": {{ "value": "Right(0x{})", "type": "Either<u32, Signature>" }} }}"#,
        hex::encode(sig.as_ref()),
    );
    let witness: WitnessValues =
        serde_json::from_str(&wit_json).map_err(|e| format!("witness: {e}"))?;

    let satisfied = compiled.satisfy(witness).map_err(|e| format!("satisfy: {e}"))?;
    let pruned = satisfied
        .redeem()
        .prune(&env)
        .map_err(|e| format!("prune: {e}"))?;
    let mut mac = BitMachine::for_program(&pruned).map_err(|e| format!("machine: {e}"))?;
    mac.exec(&pruned, &env).map(|_| ()).map_err(|e| format!("run: {e}"))
}

#[test]
fn refund_accepts_the_maker_after_the_timeout() {
    // timeout 0 — refundable at any height; sign with the maker key.
    let tessera = refundable_tessera(0);
    run_refund(&tessera, 100, &maker_keypair()).expect("REFUND must accept the maker");
}

#[test]
fn refund_rejects_before_the_timeout() {
    // The covenant unlocks at height 200; the transaction is only at 100.
    let tessera = refundable_tessera(200);
    let result = run_refund(&tessera, 100, &maker_keypair());
    assert!(
        result.is_err(),
        "REFUND must reject before the timeout, got {result:?}"
    );
}

#[test]
fn refund_rejects_a_non_maker_signature() {
    let tessera = refundable_tessera(0);
    // A different key than the covenant's MAKER_PK.
    let impostor = Keypair::from_secret_key(
        &Secp256k1::new(),
        &SecretKey::from_slice(&[9u8; 32]).expect("valid secret"),
    );
    let result = run_refund(&tessera, 100, &impostor);
    assert!(
        result.is_err(),
        "REFUND must reject a non-maker signature, got {result:?}"
    );
}

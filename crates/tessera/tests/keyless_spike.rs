//! Spike: does the keyless-REFUND covenant (`tessera-keyless.simf`) compile, and
//! does its REFUND path accept an honest sweep and reject a skimming one?
//!
//! De-risks the SimplicityHL unknowns before the cutover:
//!   * `jet::current_amount` — the covenant reading its own input's amount,
//!   * `jet::le_64` — the "maker gets at least the locked amount" check,
//!   * a key-free REFUND arm (`Either<u32, u32>` witness, no signature).

use std::sync::Arc;

use sha2::{Digest, Sha256};
use simplicity::hashes::Hash as _;
use simplicity::jet::elements::{ElementsEnv, ElementsUtxo};
use simplicity::{BitMachine, Cmr};
use simplicityhl::elements;
use simplicityhl::elements::taproot::ControlBlock;
use simplicityhl::{Arguments, CompiledProgram, WitnessValues};

const KEYLESS_SIMF: &str = include_str!("../contracts/tessera-keyless.simf");

/// SHA-256 of an empty scriptPubKey — what `output_script_hash` returns for
/// `Script::default()`.
fn empty_script_hash() -> [u8; 32] {
    Sha256::digest(elements::Script::default().as_bytes()).into()
}

/// Substitute the four `TESSERA_PARAM` literals into the covenant source.
fn render(asset_b: [u8; 32], amount_b: u64, maker_spk: [u8; 32], timeout: u32) -> String {
    const TAG: &str = "// TESSERA_PARAM:";
    let mut out = String::with_capacity(KEYLESS_SIMF.len() + 256);
    for line in KEYLESS_SIMF.lines() {
        match line.find(TAG) {
            Some(at) => {
                let name = line[at + TAG.len()..].trim();
                let value = match name {
                    "ASSET_B" => format!("0x{}", hex::encode(asset_b)),
                    "AMOUNT_B" => amount_b.to_string(),
                    "MAKER_SPK" => format!("0x{}", hex::encode(maker_spk)),
                    "TIMEOUT" => timeout.to_string(),
                    other => panic!("unknown TESSERA_PARAM:{other}"),
                };
                let eq = line.find("= ").expect("param line has '= '");
                out.push_str(&line[..eq + 2]);
                out.push_str(&value);
                out.push_str("; ");
                out.push_str(&line[at..]);
            }
            None => out.push_str(line),
        }
        out.push('\n');
    }
    out
}

/// A transaction: one input, one output paying `amount` of the default asset to
/// an empty scriptPubKey, locked to block height `lock_height`.
fn sweep_tx(amount: u64, lock_height: u32) -> elements::Transaction {
    elements::Transaction {
        version: 2,
        lock_time: elements::LockTime::Blocks(
            elements::locktime::Height::from_consensus(lock_height).expect("valid height"),
        ),
        input: vec![elements::TxIn {
            previous_output: elements::OutPoint::default(),
            is_pegin: false,
            script_sig: elements::Script::new(),
            sequence: elements::Sequence::ENABLE_LOCKTIME_NO_RBF,
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

/// An Elements environment for `tx` whose single input holds `locked` units of
/// the default asset. Control block / CMR are dummy — the keyless covenant's
/// jets do not depend on tap data.
fn env_for(tx: elements::Transaction, locked: u64) -> ElementsEnv<Arc<elements::Transaction>> {
    let ctrl: [u8; 33] = [
        0xc0, 0xeb, 0x04, 0xb6, 0x8e, 0x9a, 0x26, 0xd1, 0x16, 0x04, 0x6c, 0x76, 0xe8, 0xff, 0x47,
        0x33, 0x2f, 0xb7, 0x1d, 0xda, 0x90, 0xff, 0x4b, 0xef, 0x53, 0x70, 0xf2, 0x52, 0x26, 0xd3,
        0xbc, 0x09, 0xfc,
    ];
    ElementsEnv::new(
        Arc::new(tx),
        vec![ElementsUtxo {
            script_pubkey: elements::Script::default(),
            asset: elements::confidential::Asset::Explicit(elements::AssetId::default()),
            value: elements::confidential::Value::Explicit(locked),
        }],
        0,
        Cmr::from_byte_array([0; 32]),
        ControlBlock::from_slice(&ctrl).expect("valid control block"),
        None,
        elements::BlockHash::all_zeros(),
    )
}

/// Compile the keyless covenant with `timeout`, satisfy the REFUND path, and run
/// it: the covenant locks `locked`, the tx pays `refunded` at `lock_height`.
fn run_refund(timeout: u32, locked: u64, refunded: u64, lock_height: u32) -> Result<(), String> {
    let source = render([0u8; 32], 50_000, empty_script_hash(), timeout);
    let compiled = CompiledProgram::new(source, Arguments::default(), false)
        .map_err(|e| format!("compile: {e}"))?;

    let witness: WitnessValues = serde_json::from_str(
        r#"{ "PATH": { "value": "Right(0)", "type": "Either<u32, u32>" } }"#,
    )
    .map_err(|e| format!("witness: {e}"))?;
    let satisfied = compiled.satisfy(witness).map_err(|e| format!("satisfy: {e}"))?;

    let env = env_for(sweep_tx(refunded, lock_height), locked);
    let pruned = satisfied.redeem().prune(&env).map_err(|e| format!("prune: {e}"))?;
    let mut mac = BitMachine::for_program(&pruned).map_err(|e| format!("machine: {e}"))?;
    mac.exec(&pruned, &env).map(|_| ()).map_err(|e| format!("run: {e}"))
}

#[test]
fn keyless_covenant_compiles_and_yields_a_cmr() {
    let source = render([0x11; 32], 50_000, [0x22; 32], 200);
    let compiled = CompiledProgram::new(source, Arguments::default(), false)
        .expect("the keyless covenant must compile");
    assert_eq!(compiled.commit().cmr().to_string().len(), 64);
}

#[test]
fn refund_accepts_a_full_sweep_after_the_timeout() {
    // timeout 100; tx locks to height 150; the maker is paid the full 1_000_000.
    run_refund(100, 1_000_000, 1_000_000, 150)
        .expect("REFUND must accept a full sweep to the maker past the timeout");
}

#[test]
fn refund_rejects_a_skimming_sweep() {
    // The sweep returns only 999_999 of the locked 1_000_000 — a skim.
    let result = run_refund(100, 1_000_000, 999_999, 150);
    assert!(result.is_err(), "REFUND must reject a sweep that shortchanges the maker");
}

#[test]
fn refund_rejects_before_the_timeout() {
    // timeout 200; the tx only locks to height 150.
    let result = run_refund(200, 1_000_000, 1_000_000, 150);
    assert!(result.is_err(), "REFUND must reject before the timeout height");
}

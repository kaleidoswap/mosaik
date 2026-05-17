//! Phase-1 spike: does the quote covenant (`tessera-quote.simf`) compile, and
//! does its SETTLE path accept a maker-signed quote and reject a tampered one?
//!
//! This de-risks the SimplicityHL unknowns before the rest of Phase 1 is wired:
//!   * the `sha_256_ctx_8_*` jets hashing a structured (asset ‖ amount ‖ height)
//!     message,
//!   * a 4-tuple `(u32, u64, u32, Signature)` carried in a witness `Either` arm,
//!   * BIP-340 verification of a signature over that re-computed hash.

use secp256k1::{Keypair, Message, Secp256k1, SecretKey};
use sha2::{Digest, Sha256};
use simplicity::BitMachine;
use simplicityhl::{elements, Arguments, CompiledProgram, WitnessValues};

const QUOTE_SIMF: &str = include_str!("../contracts/tessera-quote.simf");

const MAKER_SECRET: [u8; 32] = [7u8; 32];
const AMOUNT_B: u64 = 50_000;
const VALID_HEIGHT: u32 = 0;

fn maker_keypair() -> Keypair {
    Keypair::from_secret_key(
        &Secp256k1::new(),
        &SecretKey::from_slice(&MAKER_SECRET).expect("valid secret"),
    )
}

/// SHA-256 of an empty scriptPubKey — what `output_script_hash` returns for
/// `Script::default()`.
fn empty_script_hash() -> [u8; 32] {
    Sha256::digest(elements::Script::default().as_bytes()).into()
}

/// Substitute the four `TESSERA_PARAM` literals into the covenant source.
fn render(asset_b: [u8; 32], maker_spk: [u8; 32], maker_pk: [u8; 32], timeout: u32) -> String {
    const TAG: &str = "// TESSERA_PARAM:";
    let mut out = String::with_capacity(QUOTE_SIMF.len() + 256);
    for line in QUOTE_SIMF.lines() {
        match line.find(TAG) {
            Some(at) => {
                let name = line[at + TAG.len()..].trim();
                let value = match name {
                    "ASSET_B" => format!("0x{}", hex::encode(asset_b)),
                    "MAKER_SPK" => format!("0x{}", hex::encode(maker_spk)),
                    "MAKER_PK" => format!("0x{}", hex::encode(maker_pk)),
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

/// The quote hash the covenant re-computes: SHA-256(asset_b ‖ amount_b ‖ height),
/// big-endian — matching the `sha_256_ctx_8_add_*` jets.
fn quote_hash(asset_b: [u8; 32], amount_b: u64, valid_height: u32) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(asset_b);
    h.update(amount_b.to_be_bytes());
    h.update(valid_height.to_be_bytes());
    h.finalize().into()
}

/// A transaction whose output 0 explicitly pays `amount` of the default asset
/// to an empty scriptPubKey — the maker's counter-payment.
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

/// Compile the quote covenant, satisfy SETTLE with a `(vout, amount, height,
/// sig)` quote, and run it against `tx`. `Ok` iff the covenant accepts.
fn run_settle(
    witness_amount: u64,
    sig_bytes: [u8; 64],
    tx: elements::Transaction,
) -> Result<(), String> {
    let asset_b = [0u8; 32]; // AssetId::default()
    let maker_pk = maker_keypair().x_only_public_key().0.serialize();
    let source = render(asset_b, empty_script_hash(), maker_pk, 0);

    let compiled = CompiledProgram::new(source, Arguments::default(), false)
        .map_err(|e| format!("compile: {e}"))?;

    let wit_json = format!(
        r#"{{ "PATH": {{ "value": "Left((0, {witness_amount}, {VALID_HEIGHT}, 0x{}))", "type": "Either<(u32, u64, u32, Signature), Signature>" }} }}"#,
        hex::encode(sig_bytes),
    );
    let witness: WitnessValues =
        serde_json::from_str(&wit_json).map_err(|e| format!("witness: {e}"))?;

    let satisfied = compiled.satisfy(witness).map_err(|e| format!("satisfy: {e}"))?;
    let env = simplicityhl::dummy_env::dummy_with_tx(tx);
    let pruned = satisfied.redeem().prune(&env).map_err(|e| format!("prune: {e}"))?;
    let mut mac = BitMachine::for_program(&pruned).map_err(|e| format!("machine: {e}"))?;
    mac.exec(&pruned, &env).map(|_| ()).map_err(|e| format!("run: {e}"))
}

#[test]
fn quote_covenant_compiles_and_yields_a_cmr() {
    let source = render([0u8; 32], empty_script_hash(), [0x11; 32], 0);
    let compiled = CompiledProgram::new(source, Arguments::default(), false)
        .expect("the quote covenant must compile");
    let cmr = compiled.commit().cmr().to_string();
    assert_eq!(cmr.len(), 64, "CMR must be 32 bytes hex");
}

#[test]
fn settle_accepts_a_maker_signed_quote() {
    let asset_b = [0u8; 32];
    let digest = quote_hash(asset_b, AMOUNT_B, VALID_HEIGHT);
    let sig =
        Secp256k1::new().sign_schnorr_no_aux_rand(&Message::from_digest(digest), &maker_keypair());

    run_settle(AMOUNT_B, *sig.as_ref(), tx_paying_maker(AMOUNT_B))
        .expect("SETTLE must accept a correctly signed quote that the tx honours");
}

#[test]
fn settle_rejects_a_tampered_amount() {
    // The maker signed a quote for AMOUNT_B; the taker presents AMOUNT_B - 1.
    let asset_b = [0u8; 32];
    let digest = quote_hash(asset_b, AMOUNT_B, VALID_HEIGHT);
    let sig =
        Secp256k1::new().sign_schnorr_no_aux_rand(&Message::from_digest(digest), &maker_keypair());

    // Witness + tx both say AMOUNT_B - 1, so the signature no longer matches.
    let result = run_settle(AMOUNT_B - 1, *sig.as_ref(), tx_paying_maker(AMOUNT_B - 1));
    assert!(result.is_err(), "SETTLE must reject a quote amount the maker did not sign");
}

#[test]
fn settle_rejects_a_non_maker_signature() {
    let asset_b = [0u8; 32];
    let digest = quote_hash(asset_b, AMOUNT_B, VALID_HEIGHT);
    let impostor = Keypair::from_secret_key(
        &Secp256k1::new(),
        &SecretKey::from_slice(&[9u8; 32]).expect("valid secret"),
    );
    let sig = Secp256k1::new().sign_schnorr_no_aux_rand(&Message::from_digest(digest), &impostor);

    let result = run_settle(AMOUNT_B, *sig.as_ref(), tx_paying_maker(AMOUNT_B));
    assert!(result.is_err(), "SETTLE must reject a quote not signed by the maker");
}

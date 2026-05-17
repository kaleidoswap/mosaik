//! Tessera — the Mosaik Simplicity covenant.
//!
//! A **Tessera** is one swap offer expressed as a covenant: a single Liquid
//! coin whose Simplicity program enforces the trade terms. This crate owns the
//! [`Tessera`] terms of an offer and turns them into a concrete Simplicity
//! program — it substitutes the terms into the [`tessera.simf`] template and
//! (once wired to the SimplicityHL compiler) produces the program whose
//! commitment goes into the Taproot tapleaf.
//!
//! A **Mosaik** market is a mosaic of these tesserae.
//!
//! [`tessera.simf`]: ../contracts/tessera.simf

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// The SimplicityHL source for the Tessera covenant. DRAFT — see the file header.
pub const TESSERA_SIMF: &str = include_str!("../contracts/tessera.simf");

/// A Tessera — the immutable terms of one Mosaik swap offer.
///
/// Every field is committed into the covenant tapleaf, so the terms cannot
/// change once the offer UTXO exists. See `docs/DESIGN.md` §2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tessera {
    /// Asset id (32 bytes, hex) the maker wants to receive.
    pub asset_b: [u8; 32],
    /// Exact amount of `asset_b` the maker must be paid.
    pub amount_b: u64,
    /// SHA-256 of the maker's scriptPubKey — where the counter-payment must go.
    pub maker_spk_hash: [u8; 32],
    /// Block height after which the maker may take the REFUND path.
    pub timeout: u32,
    /// Maker BIP-340 x-only public key (32 bytes) for the REFUND path.
    pub maker_pk: [u8; 32],
}

impl Tessera {
    /// Render concrete SimplicityHL source with these terms substituted in.
    ///
    /// The template carries the five terms as inline literals, each on a line
    /// tagged `// TESSERA_PARAM:<NAME>`. This rewrites the literal on each
    /// tagged line so the program is fully concrete and ready to compile.
    pub fn render(&self) -> String {
        const TAG: &str = "// TESSERA_PARAM:";
        let mut out = String::with_capacity(TESSERA_SIMF.len() + 256);

        for line in TESSERA_SIMF.lines() {
            match line.find(TAG) {
                // A tagged parameter line: replace the literal between `= `
                // and `;` with this offer's value.
                Some(tag_at) => {
                    let name = line[tag_at + TAG.len()..].trim();
                    let value = self.param_literal(name);
                    let eq = line.find("= ").expect("param line must contain '= '");
                    out.push_str(&line[..eq + 2]);
                    out.push_str(&value);
                    out.push_str("; ");
                    out.push_str(&line[tag_at..]);
                }
                None => out.push_str(line),
            }
            out.push('\n');
        }
        out
    }

    /// The SimplicityHL literal for a `TESSERA_PARAM` name.
    fn param_literal(&self, name: &str) -> String {
        match name {
            "ASSET_B" => format!("0x{}", hex::encode(self.asset_b)),
            "AMOUNT_B" => self.amount_b.to_string(),
            "MAKER_SPK" => format!("0x{}", hex::encode(self.maker_spk_hash)),
            "TIMEOUT" => self.timeout.to_string(),
            "MAKER_PK" => format!("0x{}", hex::encode(self.maker_pk)),
            other => panic!("unknown TESSERA_PARAM:{other} in tessera.simf"),
        }
    }

    /// Compile the parameterised covenant with the SimplicityHL compiler.
    ///
    /// Renders the terms into concrete source, compiles it to Simplicity, and
    /// returns the program's Commitment Merkle Root — the 32-byte value the
    /// Taproot tapleaf commits to, and what `mosaik-core` needs to derive the
    /// offer's address.
    pub fn compile(&self) -> Result<CompiledTessera> {
        use simplicityhl::{Arguments, CompiledProgram};

        let source = self.render();
        // The covenant takes no SimplicityHL `param`s — every term is already
        // substituted as an inline literal by `render`, so `Arguments` is empty.
        let compiled = CompiledProgram::new(source, Arguments::default(), false)
            .map_err(|e| anyhow::anyhow!("SimplicityHL compilation failed:\n{e}"))?;

        let cmr_hex = compiled.commit().cmr().to_string();
        let cmr_bytes = hex::decode(&cmr_hex)
            .ok()
            .and_then(|b| <[u8; 32]>::try_from(b).ok())
            .ok_or_else(|| anyhow::anyhow!("unexpected CMR encoding: {cmr_hex}"))?;

        Ok(CompiledTessera { cmr: cmr_bytes })
    }

    /// Build the Taproot witness for spending this covenant via SETTLE.
    ///
    /// SETTLE carries no signature — the witness is fully determined by the
    /// covenant and `settle_vout` (the index of the output that pays the
    /// maker). The returned [`TesseraWitness`] is the input's witness stack;
    /// drop it into a transaction whose output `settle_vout` pays the maker.
    pub fn settle_witness(&self, settle_vout: u32) -> Result<TesseraWitness> {
        // SETTLE: PATH = Left(settle_vout). No signature needed.
        self.witness_for(&format!("Left({settle_vout})"))
    }

    /// Build the complete REFUND transaction, signed and ready to broadcast.
    ///
    /// This does the whole REFUND spend in one step, because the pieces are
    /// coupled: the maker's signature is over the Simplicity `sig_all` hash
    /// (which binds to the chain genesis and the spent UTXO), and the covenant
    /// program must be **pruned** against that same transaction environment
    /// before it goes into the witness — an unpruned program still carries the
    /// dead SETTLE branch and the node rejects it (`Program has FAIL node`).
    ///
    /// `raw_tx_hex` is the unsigned refund tx (covenant input at `input_index`,
    /// `nLockTime >= timeout`). `prevout_*` describe the covenant UTXO, and
    /// `genesis_hash` is the chain's genesis block hash (display hex).
    pub fn build_refund_tx(
        &self,
        raw_tx_hex: &str,
        input_index: usize,
        prevout_spk: &[u8],
        prevout_asset: &str,
        prevout_value: u64,
        genesis_hash: &str,
        maker_secret: &[u8; 32],
    ) -> Result<String> {
        use std::str::FromStr;
        use std::sync::Arc;

        use secp256k1::{Keypair, Message, Secp256k1, SecretKey};
        use simplicity::elements::{
            confidential,
            encode::{deserialize, serialize_hex},
            taproot::ControlBlock,
            AssetId, BlockHash, Script, Transaction,
        };
        use simplicity::hashes::Hash as _;
        use simplicity::jet::elements::{ElementsEnv, ElementsUtxo};
        use simplicityhl::{Arguments, CompiledProgram, WitnessValues};

        let compiled = CompiledProgram::new(self.render(), Arguments::default(), false)
            .map_err(|e| anyhow::anyhow!("compile: {e}"))?;
        let cmr = compiled.commit().cmr();
        let cmr_bytes: [u8; 32] = hex::decode(cmr.to_string())
            .ok()
            .and_then(|b| <[u8; 32]>::try_from(b).ok())
            .ok_or_else(|| anyhow::anyhow!("unexpected CMR encoding"))?;

        let (spend_info, leaf_script) = taproot_spend_info(&cmr_bytes)?;
        let control_block: ControlBlock = spend_info
            .control_block(&(leaf_script.clone(), simplicity::leaf_version()))
            .ok_or_else(|| anyhow::anyhow!("no control block for the covenant leaf"))?;

        let tx: Transaction = deserialize(&hex::decode(raw_tx_hex)?)
            .map_err(|e| anyhow::anyhow!("decode refund tx: {e}"))?;

        let utxo = ElementsUtxo {
            script_pubkey: Script::from(prevout_spk.to_vec()),
            asset: confidential::Asset::Explicit(
                AssetId::from_str(prevout_asset).map_err(|e| anyhow::anyhow!("asset id: {e}"))?,
            ),
            value: confidential::Value::Explicit(prevout_value),
        };
        let genesis = BlockHash::from_str(genesis_hash)
            .map_err(|e| anyhow::anyhow!("genesis hash: {e}"))?;

        let env = ElementsEnv::new(
            Arc::new(tx.clone()),
            vec![utxo],
            input_index as u32,
            cmr,
            control_block.clone(),
            None,
            genesis,
        );

        // The covenant's `jet::sig_all_hash` is exactly this `sig_all` sighash.
        let sighash = env.c_tx_env().sighash_all();
        let secp = Secp256k1::new();
        let keypair = Keypair::from_secret_key(
            &secp,
            &SecretKey::from_slice(maker_secret).map_err(|e| anyhow::anyhow!("secret: {e}"))?,
        );
        let msg = Message::from_digest(sighash.to_byte_array());
        let sig = secp.sign_schnorr_no_aux_rand(&msg, &keypair);

        // Satisfy the REFUND path, then prune the program against this exact
        // transaction so the dead SETTLE branch is gone from the witness.
        let wit_json = format!(
            r#"{{ "PATH": {{ "value": "Right(0x{})", "type": "Either<u32, Signature>" }} }}"#,
            hex::encode(sig.as_ref()),
        );
        let witness_values: WitnessValues =
            serde_json::from_str(&wit_json).map_err(|e| anyhow::anyhow!("witness: {e}"))?;
        let satisfied = compiled
            .satisfy(witness_values)
            .map_err(|e| anyhow::anyhow!("satisfy: {e}"))?;
        let pruned = satisfied
            .redeem()
            .prune(&env)
            .map_err(|e| anyhow::anyhow!("prune: {e}"))?;
        let program = pruned.to_vec_without_witness();
        let (_, witness) = pruned.to_vec_with_witness();

        // Drop the witness stack onto the covenant input and serialise.
        let mut tx = tx;
        let txin = tx
            .input
            .get_mut(input_index)
            .ok_or_else(|| anyhow::anyhow!("no input #{input_index} in the transaction"))?;
        txin.witness.script_witness = vec![
            witness,
            program,
            leaf_script.into_bytes(),
            control_block.serialize(),
        ];
        Ok(serialize_hex(&tx))
    }

    /// Compile the covenant and build the Taproot witness for the given `PATH`
    /// value (`"Left(vout)"` for SETTLE, `"Right(0x..sig..)"` for REFUND).
    fn witness_for(&self, path_value: &str) -> Result<TesseraWitness> {
        use simplicityhl::{Arguments, CompiledProgram, WitnessValues};

        let compiled = CompiledProgram::new(self.render(), Arguments::default(), false)
            .map_err(|e| anyhow::anyhow!("compile: {e}"))?;

        let wit_json = format!(
            r#"{{ "PATH": {{ "value": "{path_value}", "type": "Either<u32, Signature>" }} }}"#
        );
        let witness_values: WitnessValues =
            serde_json::from_str(&wit_json).map_err(|e| anyhow::anyhow!("witness: {e}"))?;
        let satisfied = compiled
            .satisfy(witness_values)
            .map_err(|e| anyhow::anyhow!("satisfy: {e}"))?;
        // The program is the standalone commitment encoding; the witness is the
        // separate satisfaction data. (Splitting the redeem node instead yields
        // a program that does not decode on its own.)
        let program = compiled.commit().to_vec_without_witness();
        let (_, witness) = satisfied.redeem().to_vec_with_witness();

        // Taproot control block for the covenant leaf.
        let cmr_hex = compiled.commit().cmr().to_string();
        let cmr: [u8; 32] = hex::decode(&cmr_hex)
            .ok()
            .and_then(|b| <[u8; 32]>::try_from(b).ok())
            .ok_or_else(|| anyhow::anyhow!("unexpected CMR encoding: {cmr_hex}"))?;
        let (spend_info, leaf_script) = taproot_spend_info(&cmr)?;
        let control_block = spend_info
            .control_block(&(leaf_script.clone(), simplicity::leaf_version()))
            .ok_or_else(|| anyhow::anyhow!("no control block for the covenant leaf"))?;

        Ok(TesseraWitness {
            program,
            witness,
            leaf_script: leaf_script.into_bytes(),
            control_block: control_block.serialize(),
        })
    }
}

/// A compiled Tessera covenant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledTessera {
    /// Commitment Merkle Root — the value the Taproot tapleaf commits to.
    pub cmr: [u8; 32],
}

impl CompiledTessera {
    /// The CMR as a 64-character hex string.
    pub fn cmr_hex(&self) -> String {
        hex::encode(self.cmr)
    }

    /// Derive the covenant's Liquid Taproot address (elementsregtest params).
    ///
    /// The covenant lives in a single Taproot leaf: the leaf script is the
    /// 32-byte CMR, the leaf version is the Simplicity version (`0xbe`). There
    /// is no key-path spend, so the internal key is the BIP-341 NUMS point.
    /// Funding this address creates the offer's covenant UTXO.
    pub fn address(&self) -> Result<simplicityhl::elements::Address> {
        use simplicityhl::elements::{Address, AddressParams};
        let (spend_info, _) = taproot_spend_info(&self.cmr)?;
        Ok(Address::p2tr_tweaked(
            spend_info.output_key(),
            None,
            &AddressParams::ELEMENTS,
        ))
    }
}

/// Derive the BIP-340 x-only public key (32 bytes) for a secret key.
///
/// Mosaik uses this to set a Tessera's `maker_pk` from the maker's secret, so
/// the covenant's REFUND path verifies against the right key.
pub fn x_only_pubkey(secret: &[u8; 32]) -> Result<[u8; 32]> {
    use simplicityhl::elements::secp256k1_zkp::{Keypair, Secp256k1, SecretKey};

    let secp = Secp256k1::new();
    let sk = SecretKey::from_slice(secret).map_err(|e| anyhow::anyhow!("secret key: {e}"))?;
    let keypair = Keypair::from_secret_key(&secp, &sk);
    Ok(keypair.x_only_public_key().0.serialize())
}

/// Build the single-leaf Taproot for a covenant with the given CMR.
///
/// The leaf script is the 32-byte CMR, the leaf version is the Simplicity
/// version (`0xbe`), and the internal key is the BIP-341 NUMS point — provably
/// no known discrete log, so the covenant can only be spent through the leaf.
/// Returns the spend info and the leaf script.
fn taproot_spend_info(
    cmr: &[u8; 32],
) -> Result<(
    simplicityhl::elements::taproot::TaprootSpendInfo,
    simplicityhl::elements::Script,
)> {
    use simplicityhl::elements::{
        secp256k1_zkp::{Secp256k1, XOnlyPublicKey},
        taproot::TaprootBuilder,
        Script,
    };

    const NUMS_X_ONLY: &str =
        "50929b74c1a04954b78b4b6035e97a5e078a5a0f28ec96d547bfee9ace803ac0";
    let nums = hex::decode(NUMS_X_ONLY).expect("valid NUMS hex");
    let internal_key =
        XOnlyPublicKey::from_slice(&nums).map_err(|e| anyhow::anyhow!("NUMS key: {e}"))?;

    let leaf_script = Script::from(cmr.to_vec());
    let secp = Secp256k1::verification_only();
    let spend_info = TaprootBuilder::new()
        .add_leaf_with_ver(0, leaf_script.clone(), simplicity::leaf_version())
        .map_err(|e| anyhow::anyhow!("taproot leaf: {e:?}"))?
        .finalize(&secp, internal_key)
        .map_err(|e| anyhow::anyhow!("taproot finalize: {e:?}"))?;

    Ok((spend_info, leaf_script))
}

/// The Taproot script-path witness for spending a Tessera covenant.
///
/// The four parts form the input's witness stack (bottom to top). On a
/// Simplicity-capable node all four are consumed; the program is executed
/// against the spending transaction.
#[derive(Debug, Clone)]
pub struct TesseraWitness {
    /// The encoded Simplicity program.
    pub program: Vec<u8>,
    /// The encoded Simplicity witness (the satisfied `PATH`).
    pub witness: Vec<u8>,
    /// The tapleaf script — the 32-byte CMR.
    pub leaf_script: Vec<u8>,
    /// The Taproot control block proving the leaf is in the tree.
    pub control_block: Vec<u8>,
}

impl TesseraWitness {
    /// The full Taproot script-path witness stack, bottom to top.
    ///
    /// Simplicity's canonical order puts the **witness** before the **program**,
    /// then the tapleaf script (the CMR) and the control block.
    pub fn witness_stack(&self) -> Vec<Vec<u8>> {
        vec![
            self.witness.clone(),
            self.program.clone(),
            self.leaf_script.clone(),
            self.control_block.clone(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tessera() -> Tessera {
        Tessera {
            asset_b: [0x11; 32],
            amount_b: 50_000,
            maker_spk_hash: [0x22; 32],
            timeout: 200,
            maker_pk: [0x33; 32],
        }
    }

    #[test]
    fn simf_template_is_embedded() {
        assert!(TESSERA_SIMF.contains("fn settle"));
        assert!(TESSERA_SIMF.contains("fn refund"));
        assert!(TESSERA_SIMF.contains("fn main"));
    }

    #[test]
    fn render_substitutes_every_term() {
        let rendered = sample_tessera().render();
        // amount + timeout as decimal literals on their tagged lines
        assert!(rendered.contains("= 50000; // TESSERA_PARAM:AMOUNT_B"));
        assert!(rendered.contains("= 200; // TESSERA_PARAM:TIMEOUT"));
        // 32-byte terms as 0x-hex literals
        assert!(rendered.contains(&format!("0x{}; // TESSERA_PARAM:ASSET_B", "11".repeat(32))));
        assert!(rendered.contains(&format!("0x{}; // TESSERA_PARAM:MAKER_SPK", "22".repeat(32))));
        assert!(rendered.contains(&format!("0x{}; // TESSERA_PARAM:MAKER_PK", "33".repeat(32))));
        // no all-zero placeholder literal survives substitution
        assert!(!rendered.contains(&"0".repeat(64)));
        // the program body is intact
        assert!(rendered.contains("fn main"));
        assert!(rendered.contains("fn settle"));
    }

    #[test]
    fn covenant_compiles_and_yields_a_cmr() {
        let compiled = sample_tessera()
            .compile()
            .expect("the Tessera covenant must compile");
        assert_ne!(compiled.cmr, [0u8; 32], "CMR must not be all-zero");
        assert_eq!(compiled.cmr_hex().len(), 64);
    }

    #[test]
    fn covenant_yields_a_taproot_address() {
        let compiled = sample_tessera().compile().expect("compile");
        let addr = compiled.address().expect("derive address");
        // elementsregtest Taproot (bech32m) addresses start with `ert1p`.
        assert!(
            addr.to_string().starts_with("ert1p"),
            "expected an elementsregtest P2TR address, got {addr}"
        );
    }

    #[test]
    fn settle_witness_is_well_formed() {
        let tessera = sample_tessera();
        let wit = tessera.settle_witness(0).expect("build settle witness");

        assert!(!wit.program.is_empty(), "program must be non-empty");
        assert!(!wit.witness.is_empty(), "witness must be non-empty");
        // the tapleaf script is exactly the 32-byte CMR
        assert_eq!(wit.leaf_script.len(), 32);
        assert_eq!(wit.leaf_script, tessera.compile().unwrap().cmr);
        // a single-leaf Taproot control block is 33 bytes
        assert_eq!(wit.control_block.len(), 33);
        assert_eq!(wit.witness_stack().len(), 4);
    }

    #[test]
    fn tessera_roundtrips_json() {
        let terms = sample_tessera();
        let json = serde_json::to_string(&terms).unwrap();
        let back: Tessera = serde_json::from_str(&json).unwrap();
        assert_eq!(terms, back);
    }
}

//! Tessera — the Mosaik Simplicity covenant.
//!
//! A **Tessera** is one swap offer expressed as a covenant: a single Liquid
//! coin whose Simplicity program enforces the trade terms. This crate owns the
//! [`Tessera`] terms of an offer and turns them into a concrete Simplicity
//! program — it substitutes the terms into [`tessera.simf`] and compiles it.
//!
//! The covenant is **keyless** — pure transaction introspection, no signatures.
//! SETTLE pays the maker the counter-asset; REFUND, after a timeout, lets anyone
//! sweep the coin home to the maker. See `contracts/tessera.simf`.
//!
//! [`tessera.simf`]: ../contracts/tessera.simf

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// The SimplicityHL source for the Tessera covenant.
pub const TESSERA_SIMF: &str = include_str!("../contracts/tessera.simf");

/// The witness `PATH` type — `Left(settle_vout)` or `Right(refund_vout)`,
/// both plain `u32` (the covenant carries no signatures).
const PATH_TYPE: &str = "Either<u32, u32>";

/// A Tessera — the immutable terms of one Mosaik swap offer.
///
/// Every field is committed into the covenant tapleaf, so the terms cannot
/// change once the offer UTXO exists. There is no maker key: the maker is
/// identified purely by the scriptPubKey their payment must reach.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tessera {
    /// Asset id (32 bytes) the maker wants to receive.
    pub asset_b: [u8; 32],
    /// Exact amount of `asset_b` the maker must be paid.
    pub amount_b: u64,
    /// SHA-256 of the maker's scriptPubKey — where the counter-payment, or a
    /// refund sweep, must go.
    pub maker_spk_hash: [u8; 32],
    /// Block height after which anyone may take the keyless REFUND path.
    pub timeout: u32,
}

impl Tessera {
    /// Render concrete SimplicityHL source with these terms substituted in.
    pub fn render(&self) -> String {
        render_template(TESSERA_SIMF, |name| match name {
            "ASSET_B" => format!("0x{}", hex::encode(self.asset_b)),
            "AMOUNT_B" => self.amount_b.to_string(),
            "MAKER_SPK" => format!("0x{}", hex::encode(self.maker_spk_hash)),
            "TIMEOUT" => self.timeout.to_string(),
            other => panic!("unknown TESSERA_PARAM:{other} in tessera.simf"),
        })
    }

    /// Compile the covenant and return its Commitment Merkle Root.
    pub fn compile(&self) -> Result<CompiledTessera> {
        compile_cmr(&self.render())
    }

    /// Build the Taproot witness for spending this covenant via SETTLE.
    ///
    /// `settle_vout` is the index of the output that pays the maker. The
    /// witness carries no signature.
    pub fn settle_witness(&self, settle_vout: u32) -> Result<TesseraWitness> {
        build_witness(&self.render(), &format!("Left({settle_vout})"))
    }

    /// Build the complete REFUND transaction, ready to broadcast.
    ///
    /// REFUND is keyless: no signing. `raw_tx_hex` is the unsigned sweep tx
    /// (covenant input at `input_index`, `nLockTime >= timeout`, an output at
    /// `refund_vout` returning the full locked amount to the maker). The
    /// covenant program is pruned against that transaction and dropped onto the
    /// input. `prevout_*` describe the covenant UTXO.
    pub fn build_refund_tx(
        &self,
        raw_tx_hex: &str,
        input_index: usize,
        refund_vout: u32,
        prevout_spk: &[u8],
        prevout_asset: &str,
        prevout_value: u64,
    ) -> Result<String> {
        refund_tx_impl(
            &self.render(),
            raw_tx_hex,
            input_index,
            refund_vout,
            prevout_spk,
            prevout_asset,
            prevout_value,
        )
    }
}

/// Substitute `// TESSERA_PARAM:<NAME>` literals in a covenant template.
fn render_template(template: &str, lookup: impl Fn(&str) -> String) -> String {
    const TAG: &str = "// TESSERA_PARAM:";
    let mut out = String::with_capacity(template.len() + 256);
    for line in template.lines() {
        match line.find(TAG) {
            Some(tag_at) => {
                let name = line[tag_at + TAG.len()..].trim();
                let value = lookup(name);
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

/// Compile a rendered covenant `source` to its Commitment Merkle Root.
fn compile_cmr(source: &str) -> Result<CompiledTessera> {
    use simplicityhl::{Arguments, CompiledProgram};

    let compiled = CompiledProgram::new(source.to_string(), Arguments::default(), false)
        .map_err(|e| anyhow::anyhow!("SimplicityHL compilation failed:\n{e}"))?;
    let cmr_hex = compiled.commit().cmr().to_string();
    let cmr = hex::decode(&cmr_hex)
        .ok()
        .and_then(|b| <[u8; 32]>::try_from(b).ok())
        .ok_or_else(|| anyhow::anyhow!("unexpected CMR encoding: {cmr_hex}"))?;
    Ok(CompiledTessera { cmr })
}

/// Build the Taproot script-path witness for covenant `source` and a witness
/// `PATH` value (`"Left(vout)"` or `"Right(vout)"`).
fn build_witness(source: &str, path_value: &str) -> Result<TesseraWitness> {
    use simplicityhl::{Arguments, CompiledProgram, WitnessValues};

    let compiled = CompiledProgram::new(source.to_string(), Arguments::default(), false)
        .map_err(|e| anyhow::anyhow!("compile: {e}"))?;

    let wit_json =
        format!(r#"{{ "PATH": {{ "value": "{path_value}", "type": "{PATH_TYPE}" }} }}"#);
    let witness_values: WitnessValues =
        serde_json::from_str(&wit_json).map_err(|e| anyhow::anyhow!("witness: {e}"))?;
    let satisfied = compiled
        .satisfy(witness_values)
        .map_err(|e| anyhow::anyhow!("satisfy: {e}"))?;
    // The program is the standalone commitment encoding; the witness is the
    // separate satisfaction data.
    let program = compiled.commit().to_vec_without_witness();
    let (_, witness) = satisfied.redeem().to_vec_with_witness();

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

/// Build the complete REFUND transaction for covenant `source`.
///
/// Keyless — no signing. The covenant program is **pruned** against the exact
/// transaction so the dead SETTLE branch is gone from the witness (an unpruned
/// program is rejected with `Program has FAIL node`). The keyless covenant
/// never reads the chain genesis, so a dummy genesis is fine for pruning.
fn refund_tx_impl(
    source: &str,
    raw_tx_hex: &str,
    input_index: usize,
    refund_vout: u32,
    prevout_spk: &[u8],
    prevout_asset: &str,
    prevout_value: u64,
) -> Result<String> {
    use std::str::FromStr;
    use std::sync::Arc;

    use simplicity::elements::{
        confidential,
        encode::{deserialize, serialize_hex},
        taproot::ControlBlock,
        AssetId, BlockHash, Script, Transaction,
    };
    use simplicity::hashes::Hash as _;
    use simplicity::jet::elements::{ElementsEnv, ElementsUtxo};
    use simplicityhl::{Arguments, CompiledProgram, WitnessValues};

    let compiled = CompiledProgram::new(source.to_string(), Arguments::default(), false)
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

    let env = ElementsEnv::new(
        Arc::new(tx.clone()),
        vec![utxo],
        input_index as u32,
        cmr,
        control_block.clone(),
        None,
        BlockHash::all_zeros(),
    );

    // REFUND: PATH = Right(refund_vout). Prune against this exact transaction.
    let wit_json =
        format!(r#"{{ "PATH": {{ "value": "Right({refund_vout})", "type": "{PATH_TYPE}" }} }}"#);
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

    let mut tx = tx;
    let txin = tx
        .input
        .get_mut(input_index)
        .ok_or_else(|| anyhow::anyhow!("no input #{input_index} in the transaction"))?;
    txin.witness.script_witness =
        vec![witness, program, leaf_script.into_bytes(), control_block.serialize()];
    Ok(serialize_hex(&tx))
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

/// Build the single-leaf Taproot for a covenant with the given CMR.
///
/// The leaf script is the 32-byte CMR, the leaf version is the Simplicity
/// version (`0xbe`), and the internal key is the BIP-341 NUMS point — provably
/// no known discrete log, so the covenant can only be spent through the leaf.
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
        assert!(rendered.contains("= 50000; // TESSERA_PARAM:AMOUNT_B"));
        assert!(rendered.contains("= 200; // TESSERA_PARAM:TIMEOUT"));
        assert!(rendered.contains(&format!("0x{}; // TESSERA_PARAM:ASSET_B", "11".repeat(32))));
        assert!(rendered.contains(&format!("0x{}; // TESSERA_PARAM:MAKER_SPK", "22".repeat(32))));
        // no all-zero placeholder literal survives substitution
        assert!(!rendered.contains(&"0".repeat(64)));
        assert!(rendered.contains("fn main"));
        assert!(rendered.contains("fn settle"));
    }

    #[test]
    fn covenant_compiles_and_yields_a_cmr() {
        let compiled = sample_tessera().compile().expect("the covenant must compile");
        assert_ne!(compiled.cmr, [0u8; 32], "CMR must not be all-zero");
        assert_eq!(compiled.cmr_hex().len(), 64);
    }

    #[test]
    fn covenant_yields_a_taproot_address() {
        let compiled = sample_tessera().compile().expect("compile");
        let addr = compiled.address().expect("derive address");
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
        assert_eq!(wit.leaf_script.len(), 32);
        assert_eq!(wit.leaf_script, tessera.compile().unwrap().cmr);
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

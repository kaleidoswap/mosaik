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
    /// Render the SimplicityHL source with these terms substituted in.
    ///
    /// The template declares the terms as `param::*` constants; this emits a
    /// matching parameter block so the program is fully concrete. The exact
    /// parameter-passing form is finalised against the SimplicityHL compiler
    /// (see `compile`).
    pub fn render(&self) -> String {
        format!(
            "// auto-generated parameter block — Mosaik / Tessera\n\
             // ASSET_B  = 0x{}\n\
             // AMOUNT_B = {}\n\
             // MAKER_SPK = 0x{}\n\
             // TIMEOUT  = {}\n\
             // MAKER_PK = 0x{}\n\n{}",
            hex::encode(self.asset_b),
            self.amount_b,
            hex::encode(self.maker_spk_hash),
            self.timeout,
            hex::encode(self.maker_pk),
            TESSERA_SIMF,
        )
    }

    /// Compile the parameterised covenant to a Simplicity program.
    ///
    /// TODO(hackathon): invoke the SimplicityHL compiler (`simc` / the
    /// `simplicityhl` crate from the codespace) on [`render`](Self::render)
    /// and return the program bytes + its commitment Merkle root. The root is
    /// what the Taproot tapleaf commits to.
    pub fn compile(&self) -> Result<CompiledTessera> {
        anyhow::bail!(
            "SimplicityHL compilation not wired yet — compile {} in the \
             Simplicity codespace and feed the result back here",
            "contracts/tessera.simf"
        )
    }
}

/// A compiled Tessera covenant ready to embed in a Taproot leaf.
#[derive(Debug, Clone)]
pub struct CompiledTessera {
    /// Encoded Simplicity program.
    pub program: Vec<u8>,
    /// Commitment Merkle root — the value the tapleaf commits to.
    pub cmr: [u8; 32],
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
    fn render_includes_terms_and_template() {
        let rendered = sample_tessera().render();
        assert!(rendered.contains("AMOUNT_B = 50000"));
        assert!(rendered.contains(&hex::encode([0x11u8; 32])));
        assert!(rendered.contains("fn main"));
    }

    #[test]
    fn tessera_roundtrips_json() {
        let terms = sample_tessera();
        let json = serde_json::to_string(&terms).unwrap();
        let back: Tessera = serde_json::from_str(&json).unwrap();
        assert_eq!(terms, back);
    }
}

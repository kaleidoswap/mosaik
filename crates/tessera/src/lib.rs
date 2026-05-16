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
    fn tessera_roundtrips_json() {
        let terms = sample_tessera();
        let json = serde_json::to_string(&terms).unwrap();
        let back: Tessera = serde_json::from_str(&json).unwrap();
        assert_eq!(terms, back);
    }
}

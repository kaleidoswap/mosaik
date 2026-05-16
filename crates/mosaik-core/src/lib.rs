//! Mosaik core: offers, and the Liquid PSET / key plumbing.
//!
//! This crate turns a [`tessera::Tessera`] into a live offer (a funded covenant
//! UTXO) and builds the transactions that fill or reclaim it.
//!
//! The Elements/LWK calls are TODOs at this stage — the types and the flow are
//! defined so the hackathon work is pure fill-in. See `docs/DESIGN.md` §5.

use anyhow::Result;
use serde::{Deserialize, Serialize};

pub mod rpc;

pub use tessera::Tessera;

/// A published offer: a funded covenant UTXO plus the terms needed to fill it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Offer {
    /// The covenant UTXO, as an Elements outpoint string `"txid:vout"`.
    pub outpoint: String,
    /// Asset id (hex) locked in the UTXO — the asset the maker is selling.
    pub asset_a: String,
    /// Amount of `asset_a` locked.
    pub amount_a: u64,
    /// The Tessera — what the maker wants in return, and the refund conditions.
    pub tessera: Tessera,
}

/// Maker side — publish an offer.
pub trait MakeOffer {
    /// Fund a covenant UTXO with `amount_a` of `asset_a` and return the [`Offer`].
    fn make_offer(&self, asset_a: &str, amount_a: u64, tessera: &Tessera) -> Result<Offer>;
}

/// A maker that publishes offers against an Elements node.
pub struct MosaikMaker {
    rpc: rpc::ElementsRpc,
}

impl MosaikMaker {
    pub fn new(rpc: rpc::ElementsRpc) -> Self {
        Self { rpc }
    }

    /// A maker wired to the local Mosaik regtest (see `scripts/regtest.sh`).
    pub fn regtest() -> Self {
        Self::new(rpc::ElementsRpc::regtest_wallet())
    }
}

impl MakeOffer for MosaikMaker {
    /// Compile the Tessera, derive its covenant address, fund it on-chain, and
    /// return the published [`Offer`].
    ///
    /// Funds L-BTC offers (`asset_a` = "BTC"); the maker locks `amount_a`
    /// satoshis into the covenant UTXO. Funding any P2TR address is a normal
    /// payment, so this works on stock `elementsd` — only *spending* the
    /// covenant needs a Simplicity-capable node.
    fn make_offer(&self, asset_a: &str, amount_a: u64, tessera: &Tessera) -> Result<Offer> {
        if !matches!(asset_a.to_ascii_uppercase().as_str(), "BTC" | "LBTC" | "L-BTC") {
            anyhow::bail!("make_offer funds L-BTC offers only; got asset_a={asset_a}");
        }

        // Compile the covenant and derive its Taproot address.
        let compiled = tessera.compile()?;
        let address = compiled.address()?;
        let spk_hex = hex::encode(address.script_pubkey().as_bytes());

        // Fund the covenant UTXO and confirm it.
        let amount_btc = amount_a as f64 / 1e8;
        let txid = self.rpc.send_to_address(&address.to_string(), amount_btc)?;
        self.rpc.generate(1)?;

        // Locate the funding output among the transaction's vouts.
        let tx = self.rpc.raw_transaction(&txid)?;
        let vout = find_output_index(&tx, &spk_hex)
            .ok_or_else(|| anyhow::anyhow!("funding output for {txid} not found"))?;

        Ok(Offer {
            outpoint: format!("{txid}:{vout}"),
            asset_a: asset_a.to_string(),
            amount_a,
            tessera: tessera.clone(),
        })
    }
}

/// Find the index of the output whose scriptPubKey hex matches `spk_hex`.
fn find_output_index(tx: &serde_json::Value, spk_hex: &str) -> Option<u64> {
    tx.get("vout")?.as_array()?.iter().find_map(|out| {
        let hex = out.get("scriptPubKey")?.get("hex")?.as_str()?;
        (hex == spk_hex).then(|| out.get("n")?.as_u64()).flatten()
    })
}

/// Taker side — fill an offer.
pub trait TakeOffer {
    /// Build, finalise and broadcast the transaction that fills `offer`.
    ///
    /// The covenant-specific part is done: `offer.tessera.settle_witness(vout)`
    /// yields the SETTLE input's Taproot witness stack. What remains is
    /// standard Liquid tx assembly — covenant UTXO + taker coins in;
    /// counter-payment to the maker + the bought asset to the taker + fee out
    /// — best done with LWK, then broadcast. Returns the settlement txid.
    ///
    /// Enforcement of the covenant requires a Simplicity-capable node.
    fn take_offer(&self, offer: &Offer) -> Result<String>;
}

/// Maker side — reclaim an unfilled offer after its timeout.
pub trait ReclaimOffer {
    /// Spend the covenant UTXO back to the maker via the REFUND path.
    ///
    /// TODO(hackathon): build a tx spending the covenant UTXO, set the
    /// Simplicity witness to the REFUND path with the maker's BIP-340
    /// signature, set `nLockTime = tessera.timeout`, broadcast.
    fn reclaim(&self, offer: &Offer) -> Result<String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offer_roundtrips_json() {
        let offer = Offer {
            outpoint: "0000000000000000000000000000000000000000000000000000000000000001:0"
                .into(),
            asset_a: "aa".repeat(32),
            amount_a: 100_000,
            tessera: Tessera {
                asset_b: [0x11; 32],
                amount_b: 50_000,
                maker_spk_hash: [0x22; 32],
                timeout: 200,
                maker_pk: [0x33; 32],
            },
        };
        let json = serde_json::to_string(&offer).unwrap();
        let back: Offer = serde_json::from_str(&json).unwrap();
        assert_eq!(back.amount_a, 100_000);
        assert_eq!(back.tessera.amount_b, 50_000);
    }
}

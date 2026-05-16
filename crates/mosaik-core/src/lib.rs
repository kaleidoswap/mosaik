//! Mosaik core: offers, and the Liquid PSET / key plumbing.
//!
//! This crate turns a [`tessera::Tessera`] into a live offer (a funded covenant
//! UTXO) and builds the transactions that fill or reclaim it.
//!
//! The Elements/LWK calls are TODOs at this stage — the types and the flow are
//! defined so the hackathon work is pure fill-in. See `docs/DESIGN.md` §5.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tessera::Tessera;

pub mod rpc;

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
    ///
    /// TODO(hackathon): derive the covenant address from
    /// [`Tessera::compile`], build + sign a funding PSET with LWK, broadcast,
    /// and return the resulting outpoint.
    fn make_offer(&self, asset_a: &str, amount_a: u64, tessera: &Tessera) -> Result<Offer>;
}

/// Taker side — fill an offer.
pub trait TakeOffer {
    /// Build, finalise and broadcast the transaction that fills `offer`.
    ///
    /// TODO(hackathon): construct the Elements tx (covenant UTXO + taker coins
    /// in; counter-payment to the maker + the bought asset to the taker + fee
    /// out), set the Simplicity witness to the SETTLE path with the
    /// counter-payment output index, sign the taker inputs, broadcast. Returns
    /// the settlement txid.
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

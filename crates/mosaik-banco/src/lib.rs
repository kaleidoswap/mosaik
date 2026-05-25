//! Mosaik's Arkade-banco settlement backend.
//!
//! A banco offer is a single covenant VTXO published by funding an Arkade
//! transaction whose extension packet carries the offer terms. The maker can
//! go offline after publishing; any taker fulfils in one transaction. The
//! covenant guarantee comes from Arkade Script introspection opcodes — this
//! is the structural twin of Mosaik's Liquid Tessera.
//!
//! Arkade has no Rust SDK; this crate is a thin layer on top of
//! [`BancoClient`] (the `ureq` client for `banco-proxy`) plus the
//! [`MakeOffer`] / [`TakeOffer`] / [`ReclaimOffer`] traits from `mosaik-core`.
//!
//! [`MakeOffer`]: mosaik_core::MakeOffer
//! [`TakeOffer`]: mosaik_core::TakeOffer
//! [`ReclaimOffer`]: mosaik_core::ReclaimOffer

use anyhow::Result;
use mosaik_core::{BancoOffer, Offer, OfferKind};

mod client;
pub use client::{BancoClient, BuiltOffer, CreateOfferRequest, Identity, MakerOffer};

/// Terms a Mosaik caller passes to `BancoMaker::make_offer`.
#[derive(Debug, Clone)]
pub struct OfferTerms {
    /// Raw units of the asset the maker locks (leg A).
    pub sell_amount: u64,
    /// Asset id of the locked asset; `None` = native BTC on Arkade.
    pub sell_asset: Option<String>,
    /// Raw units of the asset the maker wants paid (leg B).
    pub want_amount: u64,
    /// Asset id of the wanted asset; `None` = native BTC.
    pub want_asset: Option<String>,
    /// Unix-second timestamp after which the maker may cancel.
    pub cancel_delay: u64,
    /// Accept partial fills?
    pub partial_fill: bool,
}

/// Maker side of an Arkade-banco offer.
pub struct BancoMaker {
    client: BancoClient,
}

impl BancoMaker {
    pub fn new(client: BancoClient) -> Self {
        Self { client }
    }

    /// Build, fund and publish a banco offer. Returns the [`Offer`] in the
    /// shared `mosaik-core` shape — same envelope as a Liquid Tessera offer.
    pub fn make_offer(&self, terms: &OfferTerms) -> Result<Offer> {
        let req = CreateOfferRequest {
            want_amount: terms.want_amount,
            want_asset: terms.want_asset.as_deref(),
            sell_amount: terms.sell_amount,
            sell_asset: terms.sell_asset.as_deref(),
            cancel_delay: terms.cancel_delay,
            partial_fill: terms.partial_fill,
        };
        let built = self.client.create_offer(&req)?;

        let banco = BancoOffer {
            offer_hex: built.offer_hex,
            swap_pk_script: built.swap_pk_script,
            want_asset: terms.want_asset.clone(),
            want_amount: terms.want_amount,
            cancel_delay: terms.cancel_delay,
            partial_fill: terms.partial_fill,
        };

        Ok(Offer {
            outpoint: format!("{}:{}", built.funding_txid, built.funding_vout),
            asset_a: terms.sell_asset.clone().unwrap_or_else(|| "BTC".to_string()),
            amount_a: terms.sell_amount,
            kind: OfferKind::Banco(banco),
            maker_address: String::new(),
        })
    }

    /// Cancel an offer after its `cancel_delay`. Returns the cancel txid.
    pub fn cancel(&self, offer: &Offer) -> Result<String> {
        let banco = offer.banco()?;
        self.client.cancel(&banco.offer_hex)
    }
}

/// Taker side of an Arkade-banco offer.
pub struct BancoTaker {
    client: BancoClient,
}

impl BancoTaker {
    pub fn new(client: BancoClient) -> Self {
        Self { client }
    }

    /// Fulfil an offer in a single arkd transaction. Returns the settlement txid.
    /// `fill_amount` is honoured only if the offer carries `partial_fill = true`.
    pub fn fulfill(&self, offer: &Offer, fill_amount: Option<u64>) -> Result<String> {
        let banco = offer.banco()?;
        if fill_amount.is_some() && !banco.partial_fill {
            anyhow::bail!("offer does not accept partial fills");
        }
        self.client.fulfill(&banco.offer_hex, fill_amount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offer_terms_carry_into_an_offer_via_a_synthetic_built_response() {
        // We can't hit a live banco-proxy in unit tests; check the
        // serde shape of CreateOfferRequest and the BancoOffer mapping
        // by hand-rolling the proxy's response and the request body.
        let terms = OfferTerms {
            sell_amount: 100_000,
            sell_asset: None, // BTC
            want_amount: 50_000,
            want_asset: Some("rgb:abc".into()),
            cancel_delay: 2_000_000_000,
            partial_fill: true,
        };

        let req = CreateOfferRequest {
            want_amount: terms.want_amount,
            want_asset: terms.want_asset.as_deref(),
            sell_amount: terms.sell_amount,
            sell_asset: terms.sell_asset.as_deref(),
            cancel_delay: terms.cancel_delay,
            partial_fill: terms.partial_fill,
        };
        let req_json = serde_json::to_value(&req).unwrap();
        assert_eq!(req_json["wantAmount"], 50_000);
        assert_eq!(req_json["wantAsset"], "rgb:abc");
        assert!(req_json.get("sellAsset").is_none()); // omitted when None
        assert_eq!(req_json["partialFill"], true);

        // The Mosaik `Offer` projection.
        let banco = BancoOffer {
            offer_hex: "cafe".into(),
            swap_pk_script: "00".repeat(34),
            want_asset: terms.want_asset.clone(),
            want_amount: terms.want_amount,
            cancel_delay: terms.cancel_delay,
            partial_fill: terms.partial_fill,
        };
        let offer = Offer {
            outpoint: "feedbeef:0".into(),
            asset_a: "BTC".into(),
            amount_a: terms.sell_amount,
            kind: OfferKind::Banco(banco),
            maker_address: String::new(),
        };
        assert_eq!(offer.backend(), "banco");
        assert_eq!(offer.banco().unwrap().offer_hex, "cafe");
    }
}

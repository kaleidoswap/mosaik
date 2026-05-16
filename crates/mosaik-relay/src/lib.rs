//! Mosaik's Nostr orderbook.
//!
//! A Tessera offer is published as an addressable Nostr event (kind 30050).
//! The event `content` is the full offer as JSON; a few tags carry the
//! filterable fields so relays and clients can query by asset or order id.
//!
//! Discovery is over Nostr; the chain is the source of truth. A taker always
//! re-derives the covenant address and checks the on-chain UTXO before
//! filling, so a stale or forged offer event simply fails verification.
//!
//! See `docs/NOSTR_PROTOCOL.md`.

use anyhow::{Context, Result};
use mosaik_core::Offer;
use nostr::{Event, EventBuilder, Filter, Kind, Tag, TagKind, Timestamp};
use serde::{Deserialize, Serialize};

/// Nostr event kind for a Tessera offer (addressable / replaceable).
pub const TESSERA_OFFER_KIND: u16 = 30050;

/// The lifecycle status of an offer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OfferStatus {
    Active,
    Cancelled,
}

/// A Tessera offer as carried over Nostr — a published, funded covenant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TesseraOffer {
    /// Addressable identifier — updates and cancellation reuse it.
    pub order_id: String,
    /// The funded covenant offer (outpoint, assets, terms, maker address).
    pub offer: Offer,
    /// Unix timestamp after which the offer should be treated as stale.
    pub expiry: u64,
    /// Lifecycle status.
    pub status: OfferStatus,
}

impl TesseraOffer {
    /// Build and sign the Nostr event for this offer.
    ///
    /// `content` is the whole offer as JSON; tags carry the filterable fields.
    pub fn to_event(&self, keys: &Keys) -> Result<Event> {
        let content = serde_json::to_string(self).context("serialising offer")?;
        let asset_to = hex::encode(self.offer.tessera.asset_b);

        let builder = EventBuilder::new(Kind::Custom(TESSERA_OFFER_KIND), content)
            .tag(Tag::identifier(self.order_id.clone()))
            .tag(custom_tag("status", status_str(&self.status)))
            .tag(custom_tag("asset_from", &self.offer.asset_a))
            .tag(custom_tag("asset_to", &asset_to))
            .tag(custom_tag("covenant_utxo", &self.offer.outpoint))
            .tag(custom_tag("expiry", &self.expiry.to_string()));

        builder
            .sign_with_keys(keys)
            .context("signing offer event")
    }

    /// Parse a Tessera offer from a Nostr event.
    pub fn from_event(event: &Event) -> Result<Self> {
        if event.kind != Kind::Custom(TESSERA_OFFER_KIND) {
            anyhow::bail!("not a Tessera offer event (kind {})", event.kind.as_u16());
        }
        serde_json::from_str(&event.content).context("parsing offer from event content")
    }

    /// A Nostr filter that matches Tessera offer events.
    pub fn filter() -> Filter {
        Filter::new().kind(Kind::Custom(TESSERA_OFFER_KIND))
    }

    /// True if the offer is past its expiry relative to now.
    pub fn is_expired(&self) -> bool {
        Timestamp::now().as_secs() >= self.expiry
    }
}

fn status_str(s: &OfferStatus) -> &'static str {
    match s {
        OfferStatus::Active => "active",
        OfferStatus::Cancelled => "cancelled",
    }
}

fn custom_tag(name: &str, value: &str) -> Tag {
    Tag::custom(TagKind::custom(name.to_string()), [value.to_string()])
}

mod client;
pub use client::MosaikRelay;

/// Nostr signing keys, re-exported so callers need not depend on `nostr`.
pub use nostr::Keys;

/// Run an in-process Nostr relay on `127.0.0.1:<port>` until the process ends.
///
/// Lets the demo be self-contained — no external relay needed.
pub async fn run_local_relay(port: u16) -> Result<()> {
    use nostr_relay_builder::prelude::*;

    let relay = LocalRelay::new(RelayBuilder::default().port(port));
    relay.run().await.context("starting the local relay")?;
    println!("Mosaik relay listening at {}", relay.url().await);
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mosaik_core::Tessera;

    fn sample_offer() -> TesseraOffer {
        TesseraOffer {
            order_id: "order-001".into(),
            offer: Offer {
                outpoint: "aa".repeat(32) + ":0",
                asset_a: "BTC".into(),
                amount_a: 1_000_000,
                tessera: Tessera {
                    asset_b: [0x11; 32],
                    amount_b: 600_000,
                    maker_spk_hash: [0x22; 32],
                    timeout: 500,
                    maker_pk: [0x33; 32],
                },
                maker_address: "ert1qexample".into(),
            },
            expiry: 9_999_999_999,
            status: OfferStatus::Active,
        }
    }

    #[test]
    fn offer_roundtrips_through_a_nostr_event() {
        let keys = Keys::generate();
        let offer = sample_offer();

        let event = offer.to_event(&keys).expect("build event");
        assert_eq!(event.kind, Kind::Custom(TESSERA_OFFER_KIND));
        assert!(event.verify().is_ok(), "event signature must verify");

        let back = TesseraOffer::from_event(&event).expect("parse event");
        assert_eq!(back.order_id, offer.order_id);
        assert_eq!(back.offer.amount_a, 1_000_000);
        assert_eq!(back.offer.tessera.amount_b, 600_000);
        assert_eq!(back.status, OfferStatus::Active);
    }

    #[test]
    fn offer_carries_filterable_tags() {
        let keys = Keys::generate();
        let event = sample_offer().to_event(&keys).unwrap();
        let tag_values: Vec<String> = event
            .tags
            .iter()
            .flat_map(|t| t.clone().to_vec())
            .collect();
        assert!(tag_values.iter().any(|v| v == "order-001"));
        assert!(tag_values.iter().any(|v| v == "BTC"));
        assert!(tag_values.iter().any(|v| v.contains(':')));
    }

    #[test]
    fn rejects_a_non_offer_event() {
        let keys = Keys::generate();
        let other = EventBuilder::new(Kind::TextNote, "hello")
            .sign_with_keys(&keys)
            .unwrap();
        assert!(TesseraOffer::from_event(&other).is_err());
    }
}

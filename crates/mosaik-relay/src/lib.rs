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
use mosaik_core::{Offer, OfferKind};
use nostr::{Event, EventBuilder, Filter, Kind, Tag, TagKind, Timestamp};
use serde::{Deserialize, Serialize};

/// Nostr event kind for a Tessera offer (addressable / replaceable).
pub const TESSERA_OFFER_KIND: u16 = 30050;

/// Nostr event kind for a Mosaik maker profile (addressable / replaceable).
///
/// One profile per maker pubkey: contact handle, supported pairs, typical
/// fill latency, optional self-attested badges. Lets takers route on
/// reputation alongside price.
pub const MAKER_PROFILE_KIND: u16 = 30051;

/// Nostr event kind for a Mosaik cross-chain swap announcement
/// (addressable / replaceable). Carries a [`SwapEnvelope`] pairing two
/// single-leg offers under a shared preimage commitment.
///
/// The encoder lives in `mosaik-bridge::relay` so the relay crate doesn't
/// have to depend on the bridge crate. This constant is declared here only
/// so all Mosaik event-kind numbers live in one place.
///
/// [`SwapEnvelope`]: ../mosaik_bridge/struct.SwapEnvelope.html
pub const CROSS_CHAIN_SWAP_KIND: u16 = 30052;

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
        // The asset-to tag is backend-specific.
        let asset_to = match &self.offer.kind {
            OfferKind::Liquid(t) => hex::encode(t.asset_b),
            OfferKind::Banco(b) => b.want_asset.clone().unwrap_or_else(|| "BTC".to_string()),
        };

        let builder = EventBuilder::new(Kind::Custom(TESSERA_OFFER_KIND), content)
            .tag(Tag::identifier(self.order_id.clone()))
            .tag(custom_tag("status", status_str(&self.status)))
            .tag(custom_tag("backend", self.offer.backend()))
            .tag(custom_tag("asset_from", &self.offer.asset_a))
            .tag(custom_tag("asset_to", &asset_to))
            .tag(custom_tag("covenant_utxo", &self.offer.outpoint))
            .tag(custom_tag("expiry", &self.expiry.to_string()));

        builder.sign_with_keys(keys).context("signing offer event")
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

// ─── Maker profile (kind 30051) ──────────────────────────────────────────────

/// A trading pair a maker advertises support for.
///
/// `lock` / `want` use the same labels as the Mosaik order-book UI
/// (`"BTC"`, `"USDT"`, or a chain-native asset id). `backend` declares
/// which settlement layer the maker handles for this pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MakerPair {
    pub backend: String,
    pub lock: String,
    pub want: String,
}

/// Self-attested maker reputation surface. Lives at kind 30051 on the
/// relay, addressable so a maker can update by re-publishing under the
/// same `d` tag (typically `"mosaik-maker"`).
///
/// All fields are advisory; takers should still re-derive covenant
/// addresses on-chain before filling — the relay is discovery only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MakerProfile {
    /// Addressable identifier, e.g. `"mosaik-maker"`. Re-publishing under
    /// the same `d` tag replaces the previous profile.
    pub d_tag: String,
    /// Human-readable name shown next to offers in the UI.
    pub name: String,
    /// Optional short description / motto.
    #[serde(default)]
    pub about: Option<String>,
    /// Optional contact URI (mailto, NIP-05, https URL, …). Not validated.
    #[serde(default)]
    pub contact: Option<String>,
    /// Trading pairs and backends this maker advertises.
    #[serde(default)]
    pub pairs: Vec<MakerPair>,
    /// Self-reported typical fill latency, in seconds. `None` means unspecified.
    #[serde(default)]
    pub fill_latency_secs: Option<u32>,
    /// Self-reported cancel-delay floor (Liquid block-height for Tesseras,
    /// unix seconds for banco). Lets takers filter out makers whose offers
    /// would lock funds for too long before reclaim is possible.
    #[serde(default)]
    pub min_cancel_delay: Option<u64>,
    /// Free-form badges (e.g. `["audited", "kyc-optional"]`). Self-attested
    /// today; future phases may cross-check via signed attestations.
    #[serde(default)]
    pub badges: Vec<String>,
}

impl MakerProfile {
    /// Build and sign the Nostr profile event.
    ///
    /// `content` is the full profile JSON; a handful of tags surface the
    /// filterable fields so relays / clients can query by backend or pair
    /// without parsing the body.
    pub fn to_event(&self, keys: &Keys) -> Result<Event> {
        let content = serde_json::to_string(self).context("serialising maker profile")?;

        let mut builder = EventBuilder::new(Kind::Custom(MAKER_PROFILE_KIND), content)
            .tag(Tag::identifier(self.d_tag.clone()))
            .tag(custom_tag("name", &self.name));

        if let Some(contact) = &self.contact {
            builder = builder.tag(custom_tag("contact", contact));
        }
        for pair in &self.pairs {
            // One tag per advertised pair so a relay can index them all.
            builder = builder.tag(custom_tag(
                "pair",
                &format!("{}:{}/{}", pair.backend, pair.lock, pair.want),
            ));
            builder = builder.tag(custom_tag("backend", &pair.backend));
        }
        for badge in &self.badges {
            builder = builder.tag(custom_tag("badge", badge));
        }

        builder
            .sign_with_keys(keys)
            .context("signing maker profile event")
    }

    /// Parse a maker profile from a Nostr event.
    pub fn from_event(event: &Event) -> Result<Self> {
        if event.kind != Kind::Custom(MAKER_PROFILE_KIND) {
            anyhow::bail!("not a maker profile event (kind {})", event.kind.as_u16());
        }
        serde_json::from_str(&event.content).context("parsing maker profile from event content")
    }

    /// A Nostr filter that matches maker-profile events.
    pub fn filter() -> Filter {
        Filter::new().kind(Kind::Custom(MAKER_PROFILE_KIND))
    }
}

mod client;
pub use client::MosaikRelay;

/// Re-exported (under the in-scope `Event` import elsewhere in this file)
/// so dependents (mosaik-api) can build pre-signed events for
/// `MosaikRelay::publish_event` without taking their own `nostr` dep.
pub use nostr::Event as NostrEvent;

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
                kind: OfferKind::Liquid(Tessera {
                    asset_b: [0x11; 32],
                    amount_b: 600_000,
                    maker_spk_hash: [0x22; 32],
                    timeout: 500,
                }),
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
        assert_eq!(back.offer.tessera().unwrap().amount_b, 600_000);
        assert_eq!(back.status, OfferStatus::Active);
    }

    #[test]
    fn offer_carries_filterable_tags() {
        let keys = Keys::generate();
        let event = sample_offer().to_event(&keys).unwrap();
        let tag_values: Vec<String> = event.tags.iter().flat_map(|t| t.clone().to_vec()).collect();
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

    fn sample_profile() -> MakerProfile {
        MakerProfile {
            d_tag: "mosaik-maker".into(),
            name: "Helix MM".into(),
            about: Some("Liquid + Arkade liquidity, 24/5".into()),
            contact: Some("helix@example.com".into()),
            pairs: vec![
                MakerPair {
                    backend: "liquid".into(),
                    lock: "BTC".into(),
                    want: "USDT".into(),
                },
                MakerPair {
                    backend: "banco".into(),
                    lock: "BTC".into(),
                    want: "USDT".into(),
                },
            ],
            fill_latency_secs: Some(45),
            min_cancel_delay: Some(120),
            badges: vec!["audited".into()],
        }
    }

    #[test]
    fn maker_profile_roundtrips_through_a_nostr_event() {
        let keys = Keys::generate();
        let profile = sample_profile();

        let event = profile.to_event(&keys).expect("build profile event");
        assert_eq!(event.kind, Kind::Custom(MAKER_PROFILE_KIND));
        assert!(event.verify().is_ok(), "profile signature must verify");

        let back = MakerProfile::from_event(&event).expect("parse profile event");
        assert_eq!(back.d_tag, profile.d_tag);
        assert_eq!(back.name, "Helix MM");
        assert_eq!(back.pairs.len(), 2);
        assert_eq!(back.fill_latency_secs, Some(45));
        assert_eq!(back.badges, vec!["audited".to_string()]);
    }

    #[test]
    fn maker_profile_carries_filterable_pair_tags() {
        let keys = Keys::generate();
        let event = sample_profile().to_event(&keys).unwrap();
        let tag_values: Vec<String> = event.tags.iter().flat_map(|t| t.clone().to_vec()).collect();
        assert!(tag_values.iter().any(|v| v == "mosaik-maker"));
        assert!(tag_values.iter().any(|v| v == "Helix MM"));
        assert!(tag_values.iter().any(|v| v == "liquid:BTC/USDT"));
        assert!(tag_values.iter().any(|v| v == "banco:BTC/USDT"));
        assert!(tag_values.iter().any(|v| v == "audited"));
    }

    #[test]
    fn maker_profile_rejects_non_profile_event() {
        let keys = Keys::generate();
        let other = EventBuilder::new(Kind::TextNote, "hello")
            .sign_with_keys(&keys)
            .unwrap();
        assert!(MakerProfile::from_event(&other).is_err());
    }
}

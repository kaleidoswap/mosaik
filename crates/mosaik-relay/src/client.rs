//! The Nostr client — publish and discover Tessera offers.

use std::time::Duration;

use anyhow::{Context, Result};
use nostr_sdk::prelude::*;

use crate::TesseraOffer;

/// A Mosaik orderbook client connected to one or more Nostr relays.
pub struct MosaikRelay {
    client: Client,
    keys: Keys,
}

impl MosaikRelay {
    /// Connect to the given relay URLs, signing events with `keys`.
    pub async fn connect(keys: Keys, relays: &[&str]) -> Result<Self> {
        let client = Client::new(keys.clone());
        for url in relays {
            client
                .add_relay(*url)
                .await
                .with_context(|| format!("adding relay {url}"))?;
        }
        client.connect().await;
        Ok(Self { client, keys })
    }

    /// Publish an offer to the connected relays. Returns the Nostr event id.
    pub async fn publish_offer(&self, offer: &TesseraOffer) -> Result<String> {
        let event = offer.to_event(&self.keys)?;
        self.publish_event(event).await
    }

    /// Publish a pre-signed event of any kind to the connected relays.
    /// Used by callers that build their own events (maker profiles,
    /// cross-chain swap envelopes, NIP-09 deletions) and only need the
    /// relay's signing + transport.
    pub async fn publish_event(&self, event: Event) -> Result<String> {
        let output = self
            .client
            .send_event(&event)
            .await
            .context("publishing event")?;
        Ok(output.val.to_string())
    }

    /// Borrow this client's signing keys. mosaik-api needs them to build
    /// events with the same identity the relay client signed-up under.
    pub fn keys(&self) -> &Keys {
        &self.keys
    }

    /// Fetch the current Tessera offers from the relays.
    pub async fn fetch_offers(&self, timeout: Duration) -> Result<Vec<TesseraOffer>> {
        let events = self
            .client
            .fetch_events(TesseraOffer::filter(), timeout)
            .await
            .context("fetching offer events")?;

        Ok(events
            .into_iter()
            .filter_map(|e| TesseraOffer::from_event(&e).ok())
            .collect())
    }
}

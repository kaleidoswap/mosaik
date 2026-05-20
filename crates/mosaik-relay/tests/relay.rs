//! End-to-end test of the Nostr orderbook against a local relay.

use std::time::Duration;

use mosaik_core::{Offer, Tessera};
use mosaik_relay::{MosaikRelay, OfferStatus, TesseraOffer};
use nostr_relay_builder::prelude::*;

fn sample_offer(order_id: &str) -> TesseraOffer {
    TesseraOffer {
        order_id: order_id.into(),
        offer: Offer {
            outpoint: "bb".repeat(32) + ":1",
            asset_a: "BTC".into(),
            amount_a: 2_000_000,
            tessera: Tessera {
                asset_b: [0xab; 32],
                amount_b: 1_200_000,
                maker_spk_hash: [0xcd; 32],
                timeout: 800,
            },
            covenant_address: "ert1pcovenant".into(),
            maker_address: "ert1qmaker".into(),
        },
        expiry: 9_999_999_999,
        status: OfferStatus::Active,
    }
}

#[tokio::test]
async fn publish_and_discover_an_offer_over_nostr() {
    // A local in-process Nostr relay — no external dependency.
    let relay = LocalRelay::new(RelayBuilder::default());
    relay.run().await.expect("start local relay");
    let url = relay.url().await.to_string();

    // Maker publishes an offer.
    let maker = MosaikRelay::connect(Keys::generate(), &[url.as_str()])
        .await
        .expect("maker connects");
    let offer = sample_offer("order-xyz");
    maker
        .publish_offer(&offer)
        .await
        .expect("publish the offer");

    // Taker discovers it from the relay.
    let taker = MosaikRelay::connect(Keys::generate(), &[url.as_str()])
        .await
        .expect("taker connects");
    let offers = taker
        .fetch_offers(Duration::from_secs(5))
        .await
        .expect("fetch offers");

    let found = offers
        .iter()
        .find(|o| o.order_id == "order-xyz")
        .expect("the published offer must be discoverable");
    assert_eq!(found.offer.amount_a, 2_000_000);
    assert_eq!(found.offer.tessera.amount_b, 1_200_000);
    assert_eq!(found.status, OfferStatus::Active);
}

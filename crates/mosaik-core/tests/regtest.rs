//! Integration tests against a live Elements regtest node.
//!
//! Start the node first:
//!     export ELEMENTSD_EXEC=/path/to/elementsd
//!     ./scripts/regtest.sh up && ./scripts/regtest.sh mine 101
//!
//! If no node is reachable on the Mosaik regtest port the tests skip cleanly,
//! so `cargo test` stays green in environments without one.

use mosaik_core::rpc::ElementsRpc;
use mosaik_core::{
    demo_maker_pk, quote_tessera_for, Cheat, MakeOffer, MosaikMaker, MosaikTaker, Offer, Quote,
    QuoteTessera, ReclaimOffer, TakeOffer, DEMO_MAKER_SECRET,
};
use serde_json::json;

/// A sample quote covenant (arbitrary terms — fine for funding tests, which
/// never run the covenant).
fn sample_quote_tessera() -> QuoteTessera {
    QuoteTessera {
        asset_b: [0xab; 32],
        maker_spk_hash: [0xcd; 32],
        timeout: 500,
        maker_pk: [0x11; 32],
    }
}

/// Issue a test asset, place an explicit (unconfidential) UTXO of it for the
/// taker, and publish a real two-asset offer: the maker locks 1,000,000 sats of
/// L-BTC, priced per fill by a quote. Returns the offer and the asset id.
fn two_asset_offer(rpc: &ElementsRpc) -> (Offer, String) {
    let (asset, _) = rpc.issue_asset(100.0, 0.0).expect("issue asset");
    rpc.generate(1).expect("confirm issuance");

    // A covenant settlement must spend an explicit asset input.
    let unconf = rpc.new_unconfidential_address().expect("unconfidential address");
    rpc.send_asset_to(&unconf, 50.0, &asset).expect("explicit asset UTXO");
    rpc.generate(1).expect("confirm asset UTXO");

    let maker_addr = rpc.new_unconfidential_address().expect("maker address");
    let tessera = quote_tessera_for(rpc, &maker_addr, &asset, 500, demo_maker_pk())
        .expect("build quote covenant");
    let offer = MosaikMaker::regtest()
        .make_offer("BTC", 1_000_000, &tessera, &maker_addr)
        .expect("make_offer");
    (offer, asset)
}

/// Sign a quote for `offer` at `amount_b`, anchored to the current chain tip.
fn signed_quote(rpc: &ElementsRpc, offer: &Offer, amount_b: u64) -> (Quote, [u8; 64]) {
    let valid_height = rpc.block_count().expect("block count") as u32;
    let quote = Quote { amount_b, valid_height };
    let sig = quote
        .sign(&offer.tessera.asset_b, &DEMO_MAKER_SECRET)
        .expect("sign quote");
    (quote, sig)
}

/// Returns the regtest RPC handle, or `None` (test skips) if no node answers.
fn regtest() -> Option<ElementsRpc> {
    let rpc = ElementsRpc::regtest_wallet();
    match rpc.block_count() {
        Ok(_) => Some(rpc),
        Err(_) => {
            eprintln!("SKIP: no Elements regtest node on 127.0.0.1:7040");
            None
        }
    }
}

#[test]
fn node_is_synced_and_mineable() {
    let Some(rpc) = regtest() else { return };

    let before = rpc.block_count().expect("block count");
    assert!(before >= 1, "regtest should have a genesis-funded chain");

    let hashes = rpc.generate(2).expect("mine 2 blocks");
    assert_eq!(hashes.len(), 2);

    // `>=`, not `==`: these integration tests share one regtest node and run
    // in parallel, so other tests may mine blocks concurrently.
    let after = rpc.block_count().expect("block count");
    assert!(after >= before + 2, "mining should advance the chain");
}

#[test]
fn can_generate_addresses_and_fund_them() {
    let Some(rpc) = regtest() else { return };

    let addr = rpc.new_address().expect("new address");
    assert!(!addr.is_empty(), "address must be non-empty");

    let txid = rpc.send_to_address(&addr, 0.001).expect("send L-BTC");
    assert_eq!(txid.len(), 64, "txid should be 32 bytes hex");

    rpc.generate(1).expect("confirm funding tx");
    let tx = rpc.raw_transaction(&txid).expect("fetch funding tx");
    assert_eq!(tx.get("txid").and_then(|v| v.as_str()), Some(txid.as_str()));
}

#[test]
fn make_offer_funds_a_real_covenant_utxo() {
    let Some(rpc) = regtest() else { return };

    let tessera = sample_quote_tessera();
    let locked: u64 = 200_000; // sats of L-BTC locked in the covenant
    let maker_addr = rpc.new_unconfidential_address().expect("maker address");

    let offer = MosaikMaker::regtest()
        .make_offer("BTC", locked, &tessera, &maker_addr)
        .expect("make_offer should fund the covenant");

    let (txid, vout) = offer.outpoint.split_once(':').expect("outpoint txid:vout");
    assert_eq!(txid.len(), 64);

    let txout = rpc
        .call("gettxout", json!([txid, vout.parse::<u64>().unwrap()]))
        .expect("gettxout");
    assert!(!txout.is_null(), "covenant UTXO must be present in the UTXO set");

    let value_btc = txout.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0);
    assert!(
        (value_btc - locked as f64 / 1e8).abs() < 1e-8,
        "covenant UTXO should hold {locked} sats, got {value_btc} BTC"
    );
}

#[test]
fn take_offer_settles_at_a_quote() {
    let Some(rpc) = regtest() else { return };

    // Maker locks 1,000,000 sats L-BTC; the fill is priced by a signed quote
    // for 3.0 units of the issued asset.
    let (offer, asset) = two_asset_offer(&rpc);
    let (quote, sig) = signed_quote(&rpc, &offer, 300_000_000);

    let txid = MosaikTaker::regtest()
        .take_offer(&offer, &quote, &sig)
        .expect("take_offer");
    assert_eq!(txid.len(), 64);
    rpc.generate(1).expect("confirm settlement");

    // The covenant UTXO is now spent.
    let (cov_txid, cov_vout) = offer.outpoint.split_once(':').unwrap();
    let spent = rpc
        .call("gettxout", json!([cov_txid, cov_vout.parse::<u64>().unwrap()]))
        .expect("gettxout");
    assert!(spent.is_null(), "covenant UTXO must be spent after take_offer");

    // Output 0 paid the maker exactly the quoted 3.0 of the asset.
    let settle = rpc.raw_transaction(&txid).expect("settlement tx");
    let out0 = &settle.get("vout").and_then(|v| v.as_array()).unwrap()[0];
    let paid = out0.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0);
    assert!((paid - 3.0).abs() < 1e-8, "maker output should be 3.0 units, got {paid}");
    assert_eq!(
        out0.get("asset").and_then(|v| v.as_str()),
        Some(asset.as_str()),
        "maker output 0 must be the wanted asset"
    );
}

#[test]
fn covenant_rejects_cheating_settlements() {
    let Some(rpc) = regtest() else { return };

    // A valid offer + signed quote; each cheat builds a balanced but fraudulent
    // settlement, so only the covenant can reject it. A rejected cheat leaves
    // the covenant UTXO unspent, so all three can be tried against one offer.
    let (offer, _asset) = two_asset_offer(&rpc);
    let (quote, sig) = signed_quote(&rpc, &offer, 300_000_000);

    for cheat in [Cheat::Underpay, Cheat::WrongRecipient, Cheat::WrongIndex] {
        let result = MosaikTaker::regtest().settle(&offer, &quote, &sig, cheat);
        assert!(
            result.is_err(),
            "the covenant must reject {cheat:?}, got {result:?}"
        );
    }

    // The offer is still fillable honestly after the failed attacks.
    let txid = MosaikTaker::regtest()
        .take_offer(&offer, &quote, &sig)
        .expect("honest take_offer");
    assert_eq!(txid.len(), 64);
}

#[test]
fn covenant_rejects_an_unsigned_quote() {
    let Some(rpc) = regtest() else { return };

    // A quote the maker never signed — the SETTLE path must reject it.
    let (offer, _asset) = two_asset_offer(&rpc);
    let valid_height = rpc.block_count().expect("block count") as u32;
    let quote = Quote { amount_b: 300_000_000, valid_height };
    let forged_sig = [0u8; 64];

    let result = MosaikTaker::regtest().settle(&offer, &quote, &forged_sig, Cheat::None);
    assert!(result.is_err(), "the covenant must reject an unsigned quote, got {result:?}");
}

#[test]
fn reclaim_returns_the_locked_lbtc_after_the_timeout() {
    let Some(rpc) = regtest() else { return };

    // An L-BTC-locked offer refundable from height 1 — i.e. immediately.
    let maker_addr = rpc.new_unconfidential_address().expect("maker address");
    let lbtc = rpc.policy_asset().expect("policy asset");
    let tessera = quote_tessera_for(&rpc, &maker_addr, &lbtc, 1, demo_maker_pk())
        .expect("build quote covenant");
    let offer = MosaikMaker::regtest()
        .make_offer("BTC", 1_000_000, &tessera, &maker_addr)
        .expect("make_offer");

    let txid = MosaikMaker::regtest()
        .reclaim(&offer, &DEMO_MAKER_SECRET)
        .expect("reclaim");
    assert_eq!(txid.len(), 64);
    rpc.generate(1).expect("confirm reclaim");

    let (cov_txid, cov_vout) = offer.outpoint.split_once(':').unwrap();
    let spent = rpc
        .call("gettxout", json!([cov_txid, cov_vout.parse::<u64>().unwrap()]))
        .expect("gettxout");
    assert!(spent.is_null(), "covenant UTXO must be spent after reclaim");

    let tx = rpc.raw_transaction(&txid).expect("reclaim tx");
    let out0 = &tx.get("vout").and_then(|v| v.as_array()).unwrap()[0];
    let paid = out0.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0);
    assert!(
        (paid - 999_000.0 / 1e8).abs() < 1e-8,
        "maker should reclaim 999000 sats, got {paid} BTC"
    );
}

#[test]
fn reclaim_rejected_before_the_timeout() {
    let Some(rpc) = regtest() else { return };

    let height = rpc.block_count().expect("block count") as u32;
    let maker_addr = rpc.new_unconfidential_address().expect("maker address");
    let lbtc = rpc.policy_asset().expect("policy asset");
    let tessera = quote_tessera_for(&rpc, &maker_addr, &lbtc, height + 10_000, demo_maker_pk())
        .expect("build quote covenant");
    let offer = MosaikMaker::regtest()
        .make_offer("BTC", 1_000_000, &tessera, &maker_addr)
        .expect("make_offer");

    let result = MosaikMaker::regtest().reclaim(&offer, &DEMO_MAKER_SECRET);
    assert!(
        result.is_err(),
        "reclaim must be rejected before the timeout, got {result:?}"
    );
}

#[test]
fn can_issue_a_liquid_asset() {
    let Some(rpc) = regtest() else { return };

    let (asset_id, issue_txid) = rpc.issue_asset(1000.0, 1.0).expect("issue asset");
    assert_eq!(asset_id.len(), 64, "asset id should be 32 bytes hex");
    assert_eq!(issue_txid.len(), 64);

    rpc.generate(1).expect("confirm issuance");

    let balances = rpc.balances().expect("balances");
    assert!(
        balances.get(&asset_id).is_some(),
        "issued asset {asset_id} should appear in wallet balances: {balances}"
    );
}

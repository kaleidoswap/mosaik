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
    demo_maker_pk, tessera_for, Cheat, MakeOffer, MosaikMaker, MosaikTaker, Offer, ReclaimOffer,
    TakeOffer, Tessera, DEMO_MAKER_SECRET,
};
use serde_json::json;

/// A sample Tessera for the given maker-payment amount (arbitrary terms —
/// fine for funding tests, which never run the covenant).
fn sample_tessera(amount_b: u64) -> Tessera {
    Tessera {
        asset_b: [0xab; 32],
        amount_b,
        maker_spk_hash: [0xcd; 32],
        timeout: 500,
        maker_pk: [0x11; 32],
    }
}

/// Issue a test asset, place an explicit (unconfidential) UTXO of it for the
/// taker, and publish a real two-asset offer: the maker locks 1,000,000 sats of
/// L-BTC and wants `amount_b` units of the issued asset. Returns the offer and
/// the asset's display id.
fn two_asset_offer(rpc: &ElementsRpc, amount_b: u64) -> (Offer, String) {
    let (asset, _) = rpc.issue_asset(100.0, 0.0).expect("issue asset");
    rpc.generate(1).expect("confirm issuance");

    // A Tessera settlement must spend an explicit asset input.
    let unconf = rpc.new_unconfidential_address().expect("unconfidential address");
    rpc.send_asset_to(&unconf, 50.0, &asset).expect("explicit asset UTXO");
    rpc.generate(1).expect("confirm asset UTXO");

    let maker_addr = rpc.new_unconfidential_address().expect("maker address");
    let tessera = tessera_for(rpc, &maker_addr, &asset, amount_b, 500, [0x11; 32])
        .expect("build tessera");
    let offer = MosaikMaker::regtest()
        .make_offer("BTC", 1_000_000, &tessera, &maker_addr)
        .expect("make_offer");
    (offer, asset)
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

    // Fund the fresh address and confirm the funding tx.
    let txid = rpc.send_to_address(&addr, 0.001).expect("send L-BTC");
    assert_eq!(txid.len(), 64, "txid should be 32 bytes hex");

    rpc.generate(1).expect("confirm funding tx");
    let tx = rpc.raw_transaction(&txid).expect("fetch funding tx");
    assert_eq!(tx.get("txid").and_then(|v| v.as_str()), Some(txid.as_str()));
}

#[test]
fn make_offer_funds_a_real_covenant_utxo() {
    let Some(rpc) = regtest() else { return };

    let tessera = sample_tessera(100_000);
    let locked: u64 = 200_000; // sats of L-BTC locked in the covenant
    let maker_addr = rpc.new_unconfidential_address().expect("maker address");

    let offer = MosaikMaker::regtest()
        .make_offer("BTC", locked, &tessera, &maker_addr)
        .expect("make_offer should fund the covenant");

    // The offer references an on-chain outpoint "txid:vout".
    let (txid, vout) = offer.outpoint.split_once(':').expect("outpoint txid:vout");
    assert_eq!(txid.len(), 64);

    // The covenant UTXO must exist and hold the locked amount.
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
fn take_offer_settles_and_pays_the_maker() {
    let Some(rpc) = regtest() else { return };

    // Maker locks 1,000,000 sats L-BTC, wants 3.0 units of an issued asset.
    let amount_b = 300_000_000; // 3.0 units (8-decimal asset)
    let (offer, asset) = two_asset_offer(&rpc, amount_b);

    // Taker fills it — spends the covenant UTXO via SETTLE.
    let txid = MosaikTaker::regtest().take_offer(&offer).expect("take_offer");
    assert_eq!(txid.len(), 64);
    rpc.generate(1).expect("confirm settlement");

    // The covenant UTXO is now spent.
    let (cov_txid, cov_vout) = offer.outpoint.split_once(':').unwrap();
    let spent = rpc
        .call("gettxout", json!([cov_txid, cov_vout.parse::<u64>().unwrap()]))
        .expect("gettxout");
    assert!(spent.is_null(), "covenant UTXO must be spent after take_offer");

    // The settlement tx's output 0 paid the maker exactly 3.0 of the asset.
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

    // A valid offer; each cheat builds a balanced but fraudulent settlement,
    // so only the covenant can reject it. A rejected cheat leaves the covenant
    // UTXO unspent, so all three can be tried against the same offer.
    let (offer, _asset) = two_asset_offer(&rpc, 300_000_000);

    for cheat in [Cheat::Underpay, Cheat::WrongRecipient, Cheat::WrongIndex] {
        let result = MosaikTaker::regtest().settle(&offer, cheat);
        assert!(
            result.is_err(),
            "the covenant must reject {cheat:?}, got {result:?}"
        );
    }

    // The offer is still fillable honestly after the failed attacks.
    let txid = MosaikTaker::regtest().take_offer(&offer).expect("honest take_offer");
    assert_eq!(txid.len(), 64);
}

#[test]
fn reclaim_returns_the_locked_lbtc_after_the_timeout() {
    let Some(rpc) = regtest() else { return };

    // An L-BTC-locked offer refundable from height 1 — i.e. immediately.
    let maker_addr = rpc.new_unconfidential_address().expect("maker address");
    let lbtc = rpc.policy_asset().expect("policy asset");
    let tessera = tessera_for(&rpc, &maker_addr, &lbtc, 600_000, 1, demo_maker_pk())
        .expect("build tessera");
    let offer = MosaikMaker::regtest()
        .make_offer("BTC", 1_000_000, &tessera, &maker_addr)
        .expect("make_offer");

    // The maker reclaims via the covenant's REFUND path.
    let txid = MosaikMaker::regtest()
        .reclaim(&offer, &DEMO_MAKER_SECRET)
        .expect("reclaim");
    assert_eq!(txid.len(), 64);
    rpc.generate(1).expect("confirm reclaim");

    // The covenant UTXO is spent.
    let (cov_txid, cov_vout) = offer.outpoint.split_once(':').unwrap();
    let spent = rpc
        .call("gettxout", json!([cov_txid, cov_vout.parse::<u64>().unwrap()]))
        .expect("gettxout");
    assert!(spent.is_null(), "covenant UTXO must be spent after reclaim");

    // Output 0 returns the locked L-BTC (less the fee) to the maker.
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

    // An offer whose refund height is far in the future.
    let height = rpc.block_count().expect("block count") as u32;
    let maker_addr = rpc.new_unconfidential_address().expect("maker address");
    let lbtc = rpc.policy_asset().expect("policy asset");
    let tessera = tessera_for(&rpc, &maker_addr, &lbtc, 600_000, height + 10_000, demo_maker_pk())
        .expect("build tessera");
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

    // Issue 1000 units of a new asset with 1 reissuance token.
    let (asset_id, issue_txid) = rpc.issue_asset(1000.0, 1.0).expect("issue asset");
    assert_eq!(asset_id.len(), 64, "asset id should be 32 bytes hex");
    assert_eq!(issue_txid.len(), 64);

    rpc.generate(1).expect("confirm issuance");

    // The issued asset must now show up in the wallet balance map.
    let balances = rpc.balances().expect("balances");
    assert!(
        balances.get(&asset_id).is_some(),
        "issued asset {asset_id} should appear in wallet balances: {balances}"
    );
}

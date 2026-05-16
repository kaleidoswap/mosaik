//! Integration tests against a live Elements regtest node.
//!
//! Start the node first:
//!     export ELEMENTSD_EXEC=/path/to/elementsd
//!     ./scripts/regtest.sh up && ./scripts/regtest.sh mine 101
//!
//! If no node is reachable on the Mosaik regtest port the tests skip cleanly,
//! so `cargo test` stays green in environments without one.

use mosaik_core::rpc::ElementsRpc;
use mosaik_core::{MakeOffer, MosaikMaker, Tessera};
use serde_json::json;

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

    let after = rpc.block_count().expect("block count");
    assert_eq!(after, before + 2);
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

    // A sample offer: sell L-BTC, want some Liquid asset back.
    let tessera = Tessera {
        asset_b: [0xab; 32],
        amount_b: 100_000,
        maker_spk_hash: [0xcd; 32],
        timeout: 500,
        maker_pk: [0x11; 32],
    };
    let locked: u64 = 200_000; // sats of L-BTC locked in the covenant

    let offer = MosaikMaker::regtest()
        .make_offer("BTC", locked, &tessera)
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

//! Integration tests against a live Elements regtest node.
//!
//! Start the node first:
//!     export ELEMENTSD_EXEC=/path/to/elementsd
//!     ./scripts/regtest.sh up && ./scripts/regtest.sh mine 101
//!
//! If no node is reachable on the Mosaik regtest port the tests skip cleanly,
//! so `cargo test` stays green in environments without one.

use mosaik_core::rpc::ElementsRpc;

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

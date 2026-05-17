//! Mosaik wallet UI — a tiny HTTP server that drives the regtest covenant DEX
//! with two separate wallets: a **maker** and a **taker**.
//!
//! The single-page app in `webapp/index.html` shows both sides. The maker locks
//! an asset in a covenant UTXO; the taker fills it by paying the asset the maker
//! asked for. Settlement is one on-chain transaction, executed and enforced by
//! the Simplicity covenant — the two wallets are genuinely distinct parties, so
//! balances really move.
//!
//! The covenant never inspects the *locked* asset (it only enforces the maker's
//! counter-payment), so any combination works: L-BTC/asset, asset/L-BTC, and
//! asset/asset, in either direction.
//!
//! Wallets on the Elements node:
//!   * `mosaik`        — the regtest treasury (holds the swept freecoins)
//!   * `mosaik-maker`  — the maker
//!   * `mosaik-taker`  — the taker

use std::collections::BTreeMap;
use std::sync::Mutex;

use anyhow::{anyhow, Result};
use mosaik_core::rpc::ElementsRpc;
use mosaik_core::{
    lbtc_tessera, tessera_for, Cheat, MakeOffer, MosaikMaker, MosaikTaker, Offer, ReclaimOffer,
};
use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server};

/// The single-page wallet UI, baked into the binary.
const INDEX_HTML: &str = include_str!("../../../webapp/index.html");

const BASE: &str = "http://127.0.0.1:7040";
const RPC_USER: &str = "user";
const RPC_PASS: &str = "pass";

/// The two test Liquid assets the demo issues, so asset/asset swaps are real.
const TEST_ASSETS: [&str; 2] = ["USDT", "EURx"];

/// In-memory server state: the offers made this session and the test assets
/// (name -> RPC display-order id).
struct AppState {
    offers: Vec<Offer>,
    assets: BTreeMap<String, String>,
}

fn treasury() -> ElementsRpc {
    ElementsRpc::wallet(BASE, "mosaik", RPC_USER, RPC_PASS)
}
fn maker_rpc() -> ElementsRpc {
    ElementsRpc::wallet(BASE, "mosaik-maker", RPC_USER, RPC_PASS)
}
fn taker_rpc() -> ElementsRpc {
    ElementsRpc::wallet(BASE, "mosaik-taker", RPC_USER, RPC_PASS)
}
fn node_rpc() -> ElementsRpc {
    ElementsRpc::node(BASE, RPC_USER, RPC_PASS)
}

/// Run the wallet UI server until the process is killed.
pub fn run(port: u16) -> Result<()> {
    let node = node_rpc();
    node.ensure_wallet("mosaik-maker")?;
    node.ensure_wallet("mosaik-taker")?;

    let server = Server::http(("127.0.0.1", port))
        .map_err(|e| anyhow!("could not bind 127.0.0.1:{port}: {e}"))?;
    let state = Mutex::new(AppState { offers: Vec::new(), assets: BTreeMap::new() });

    println!("Mosaik wallet UI  ->  http://127.0.0.1:{port}");
    println!("Wallets: mosaik (treasury), mosaik-maker, mosaik-taker");
    println!("(Elements regtest expected at {BASE} — run scripts/regtest.sh)");

    for mut req in server.incoming_requests() {
        let (status, payload) = route(&mut req, &state);
        if status == 0 {
            let header = Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..])
                .expect("valid header");
            let _ = req.respond(Response::from_string(INDEX_HTML).with_header(header));
        } else {
            let body = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".into());
            let header =
                Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).expect("hdr");
            let _ = req.respond(
                Response::from_string(body).with_status_code(status).with_header(header),
            );
        }
    }
    Ok(())
}

/// Dispatch one request; `(0, _)` means "serve the HTML page".
fn route(req: &mut Request, state: &Mutex<AppState>) -> (u16, Value) {
    let method = req.method().clone();
    let url = req.url().split('?').next().unwrap_or("/").to_string();

    let result: Result<Value> = match (&method, url.as_str()) {
        (Method::Get, "/") | (Method::Get, "/index.html") => return (0, Value::Null),
        (Method::Get, "/api/state") => api_state(state),
        (Method::Post, "/api/fund/maker") => api_fund(state, &maker_rpc()),
        (Method::Post, "/api/fund/taker") => api_fund(state, &taker_rpc()),
        (Method::Post, "/api/make-offer") => api_make_offer(req, state),
        (Method::Post, "/api/take-offer") => api_take_offer(req, state),
        (Method::Post, "/api/reclaim") => api_reclaim(req, state),
        (Method::Post, "/api/contract") => api_contract(req, state),
        _ => return (404, json!({ "error": "not found" })),
    };

    match result {
        Ok(v) => (200, v),
        Err(e) => (400, json!({ "error": e.to_string() })),
    }
}

/// Parse a JSON request body, defaulting to `null` on an empty/invalid body.
fn body(req: &mut Request) -> Value {
    let mut raw = String::new();
    let _ = req.as_reader().read_to_string(&mut raw);
    serde_json::from_str(&raw).unwrap_or(Value::Null)
}

/// Raw balance (smallest unit) of `asset` from a `getbalance` map.
fn raw_balance(balances: &Value, asset: &str) -> u64 {
    balances
        .get(asset)
        .and_then(Value::as_f64)
        .map(|v| (v * 1e8).round() as u64)
        .unwrap_or(0)
}

/// Make sure the test assets are issued; returns the `name -> id` map.
fn ensure_assets(state: &Mutex<AppState>) -> Result<BTreeMap<String, String>> {
    let mut st = state.lock().unwrap();
    if st.assets.is_empty() {
        let treasury = treasury();
        for name in TEST_ASSETS {
            let (id, _) = treasury.issue_asset(100_000.0, 0.0)?;
            st.assets.insert(name.to_string(), id);
        }
        treasury.generate(1)?;
    }
    Ok(st.assets.clone())
}

/// Resolve a UI asset label (`"BTC"` or a test-asset name) to a display-order
/// asset id. `"BTC"` stays `"BTC"` — `make_offer` resolves it to the policy id.
fn label_to_id(assets: &BTreeMap<String, String>, label: &str, lbtc: &str) -> Result<String> {
    match label {
        "BTC" | "L-BTC" | "LBTC" => Ok(lbtc.to_string()),
        name => assets
            .get(name)
            .cloned()
            .ok_or_else(|| anyhow!("unknown asset '{name}' — fund a wallet to issue it")),
    }
}

/// Human name for a display-order asset id, given the test-asset map.
fn id_to_name(assets: &BTreeMap<String, String>, id: &str, lbtc: &str) -> String {
    if id == lbtc {
        return "L-BTC".to_string();
    }
    for (name, aid) in assets {
        if aid == id {
            return name.clone();
        }
    }
    format!("{}…", &id[..id.len().min(8)])
}

// ---- endpoints -------------------------------------------------------------

/// A `{ name, lbtc_sats, assets:{name:raw} }` snapshot of one wallet.
fn wallet_snapshot(rpc: &ElementsRpc, assets: &BTreeMap<String, String>) -> Result<Value> {
    let balances = rpc.balances()?;
    let mut map = serde_json::Map::new();
    for (name, id) in assets {
        map.insert(name.clone(), json!(raw_balance(&balances, id)));
    }
    Ok(json!({
        "address": rpc.new_unconfidential_address()?,
        "lbtc_sats": raw_balance(&balances, "bitcoin"),
        "assets": map,
    }))
}

/// Both wallets, chain height, the test assets, and the open offers.
fn api_state(state: &Mutex<AppState>) -> Result<Value> {
    let lbtc = node_rpc().policy_asset()?;
    let (offers, assets) = {
        let st = state.lock().unwrap();
        (st.offers.clone(), st.assets.clone())
    };

    let offers: Vec<Value> = offers
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let compiled = o.tessera.compile().ok();
            // asset_b is stored internal-order; reverse for the display id.
            let mut ab = o.tessera.asset_b.to_vec();
            ab.reverse();
            let asset_b = hex::encode(ab);
            json!({
                "index": i,
                "outpoint": o.outpoint,
                "amount_a": o.amount_a,
                "amount_b": o.tessera.amount_b,
                "lock_name": id_to_name(&assets, &o.asset_a, &lbtc),
                "want_name": id_to_name(&assets, &asset_b, &lbtc),
                "timeout": o.tessera.timeout,
                "maker_address": o.maker_address,
                "covenant_address": compiled.as_ref().and_then(|c| c.address().ok())
                    .map(|a| a.to_string()),
                "cmr": compiled.as_ref().map(|c| c.cmr_hex()),
            })
        })
        .collect();

    Ok(json!({
        "block_count": node_rpc().block_count()?,
        "assets": assets.keys().cloned().collect::<Vec<_>>(),
        "maker": wallet_snapshot(&maker_rpc(), &assets)?,
        "taker": wallet_snapshot(&taker_rpc(), &assets)?,
        "offers": offers,
    }))
}

/// Treasury -> wallet: top up with 1 L-BTC and 100 of every test asset, so the
/// wallet can be a maker or a taker for any pair.
fn api_fund(state: &Mutex<AppState>, target: &ElementsRpc) -> Result<Value> {
    let assets = ensure_assets(state)?;
    let treasury = treasury();

    let addr = target.new_unconfidential_address()?;
    let lbtc_txid = treasury.send_to_address(&addr, 1.0)?;

    // Explicit (unconfidential) UTXOs — a covenant settlement cannot spend a
    // blinded input.
    for id in assets.values() {
        let addr = target.new_unconfidential_address()?;
        treasury.send_asset_to(&addr, 100.0, id)?;
    }
    treasury.generate(1)?;
    Ok(json!({ "ok": true, "txid": lbtc_txid, "funded": "1 L-BTC + 100 of each asset" }))
}

/// Maker: fund a covenant UTXO and register the offer.
fn api_make_offer(req: &mut Request, state: &Mutex<AppState>) -> Result<Value> {
    let b = body(req);
    let amount_a = b.get("amount_a").and_then(Value::as_u64).ok_or_else(|| anyhow!("amount_a"))?;
    let amount_b = b.get("amount_b").and_then(Value::as_u64).ok_or_else(|| anyhow!("amount_b"))?;
    let timeout = b.get("timeout").and_then(Value::as_u64).unwrap_or(500) as u32;
    let lock = b.get("lock").and_then(Value::as_str).unwrap_or("BTC");
    let want = b.get("want").and_then(Value::as_str).unwrap_or("USDT");
    if lock == want {
        anyhow::bail!("lock and want assets must differ");
    }

    let maker = maker_rpc();
    let lbtc = maker.policy_asset()?;
    let assets = { state.lock().unwrap().assets.clone() };
    let maker_address = maker.new_unconfidential_address()?;

    // The covenant enforces the maker's counter-payment in the `want` asset.
    let want_id = label_to_id(&assets, want, &lbtc)?;
    let tessera = if want_id == lbtc {
        lbtc_tessera(&maker, &maker_address, amount_b, timeout)?
    } else {
        tessera_for(&maker, &maker_address, &want_id, amount_b, timeout)?
    };

    // `lock` is what the maker locks in the covenant UTXO.
    let lock_label = if lock == "BTC" { "BTC".to_string() } else { label_to_id(&assets, lock, &lbtc)? };
    let offer =
        MosaikMaker::new(maker).make_offer(&lock_label, amount_a, &tessera, &maker_address)?;

    let mut st = state.lock().unwrap();
    let index = st.offers.len();
    st.offers.push(offer.clone());

    Ok(json!({
        "ok": true,
        "index": index,
        "outpoint": offer.outpoint,
        "maker_address": offer.maker_address,
    }))
}

/// Taker: fill a registered offer — the node executes the covenant.
///
/// An optional `cheat` field (`underpay` | `wrong_recipient` | `wrong_index`)
/// builds a deliberately fraudulent settlement. The transaction is well-formed
/// and balanced, so the node's mempool would accept it — but the covenant must
/// reject it. The endpoint reports the rejection as the expected outcome, and
/// flags a `covenant_bug` if a cheat ever broadcasts.
fn api_take_offer(req: &mut Request, state: &Mutex<AppState>) -> Result<Value> {
    let b = body(req);
    let index = b.get("index").and_then(Value::as_u64).ok_or_else(|| anyhow!("index"))? as usize;
    let cheat = Cheat::parse(b.get("cheat").and_then(Value::as_str).unwrap_or("none"));

    let offer = {
        let st = state.lock().unwrap();
        st.offers.get(index).cloned().ok_or_else(|| anyhow!("no offer #{index}"))?
    };

    let result = MosaikTaker::new(taker_rpc()).settle(&offer, cheat);

    match (cheat, result) {
        // Honest fill that succeeded: confirm it and drop the offer.
        (Cheat::None, Ok(txid)) => {
            treasury().generate(1)?;
            let mut st = state.lock().unwrap();
            if index < st.offers.len() {
                st.offers.remove(index);
            }
            Ok(json!({ "ok": true, "txid": txid }))
        }
        (Cheat::None, Err(e)) => Err(e),
        // A cheat the covenant rejected — the expected, correct outcome. The
        // offer is untouched and still fillable honestly.
        (_, Err(e)) => Ok(json!({
            "ok": true,
            "rejected": true,
            "reason": e.to_string(),
        })),
        // A cheat that broadcast: the covenant failed to enforce its terms.
        (_, Ok(txid)) => {
            treasury().generate(1)?;
            Ok(json!({
                "ok": false,
                "covenant_bug": true,
                "txid": txid,
                "reason": "a fraudulent settlement was accepted on-chain",
            }))
        }
    }
}

/// Maker: reclaim an unfilled offer via the covenant's REFUND path.
fn api_reclaim(req: &mut Request, state: &Mutex<AppState>) -> Result<Value> {
    let b = body(req);
    let index = b.get("index").and_then(Value::as_u64).ok_or_else(|| anyhow!("index"))? as usize;

    let offer = {
        let st = state.lock().unwrap();
        st.offers.get(index).cloned().ok_or_else(|| anyhow!("no offer #{index}"))?
    };

    let txid = MosaikMaker::new(maker_rpc()).reclaim(&offer)?;
    treasury().generate(1)?; // confirm the reclaim

    let mut st = state.lock().unwrap();
    if index < st.offers.len() {
        st.offers.remove(index);
    }
    Ok(json!({ "ok": true, "txid": txid }))
}

/// Inspect the Simplicity covenant behind an offer — the on-chain "contract".
fn api_contract(req: &mut Request, state: &Mutex<AppState>) -> Result<Value> {
    let b = body(req);
    let index = b.get("index").and_then(Value::as_u64).ok_or_else(|| anyhow!("index"))? as usize;

    let offer = {
        let st = state.lock().unwrap();
        st.offers.get(index).cloned().ok_or_else(|| anyhow!("no offer #{index}"))?
    };
    let compiled = offer.tessera.compile()?;

    Ok(json!({
        "source": offer.tessera.render(),
        "cmr": compiled.cmr_hex(),
        "address": compiled.address()?.to_string(),
        "settle": "Taker path: spends the UTXO if output 0 pays the maker exactly \
            `amount_b` of `asset_b`. The node runs the covenant and rejects any \
            transaction that underpays. The locked asset is never inspected.",
        "refund": "Maker path: once the chain reaches the refund block height \
            the maker reclaims the locked L-BTC, signing the Simplicity sig_all \
            hash with a BIP-340 key. The covenant rejects an early or wrongly \
            signed reclaim.",
    }))
}

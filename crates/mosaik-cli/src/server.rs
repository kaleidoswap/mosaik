//! Mosaik wallet UI — a tiny HTTP server.
//!
//! Two networks, picked at startup via `--network`:
//!
//!   * `regtest` (default) — local Elements regtest on port 7040. Free coins,
//!                            instant blocks (we mine after every action), test
//!                            assets issued by the server on first Fund.
//!
//!   * `testnet`           — local Elements node on port 7041 running
//!                            `chain=liquidtestnet`. Bootstrapped by
//!                            `./scripts/testnet.sh up && ./scripts/testnet.sh fund`
//!                            (treasury faucet-funded, USDT/EURx issued once and
//!                            persisted to `.testnet/assets.json`). Same UI flow
//!                            as regtest, but blocks come from the network at
//!                            ~1 min/block, so we don't mine after actions.
//!
//! Wallets expected on the node:
//!   * `mosaik`        — treasury (free coins on regtest, faucet-funded on testnet)
//!   * `mosaik-maker`  — the maker role
//!   * `mosaik-taker`  — the taker role

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use anyhow::{anyhow, Result};
use mosaik_core::rpc::ElementsRpc;
use mosaik_core::{
    demo_maker_pk, lbtc_tessera, tessera_for, Cheat, MakeOffer, MosaikMaker, MosaikTaker, Offer,
    ReclaimOffer, DEMO_MAKER_SECRET,
};
use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server};

const INDEX_HTML: &str = include_str!("../../../webapp/index.html");
const RPC_USER: &str = "user";
const RPC_PASS: &str = "pass";

/// The two demo Liquid assets. Issued on regtest by the server on first Fund;
/// pre-issued on testnet by `./scripts/testnet.sh fund` and loaded from
/// `.testnet/assets.json`.
const TEST_ASSETS: [&str; 2] = ["USDT", "EURx"];

/// Env var the testnet.sh `serve` command sets, pointing at the persisted
/// `{name: asset_id}` map for the demo assets.
const TESTNET_ASSETS_ENV: &str = "MOSAIK_TESTNET_ASSETS_FILE";

// ── network ──────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Network {
    Regtest,
    Testnet,
}

static NETWORK: OnceLock<Network> = OnceLock::new();

fn network() -> Network { *NETWORK.get().unwrap_or(&Network::Regtest) }
fn is_regtest() -> bool { network() == Network::Regtest }

fn node_url() -> &'static str {
    match network() {
        Network::Regtest => "http://127.0.0.1:7040",
        Network::Testnet => "http://127.0.0.1:7041",
    }
}

fn treasury()  -> ElementsRpc { ElementsRpc::wallet(node_url(), "mosaik",       RPC_USER, RPC_PASS) }
fn maker_rpc() -> ElementsRpc { ElementsRpc::wallet(node_url(), "mosaik-maker", RPC_USER, RPC_PASS) }
fn taker_rpc() -> ElementsRpc { ElementsRpc::wallet(node_url(), "mosaik-taker", RPC_USER, RPC_PASS) }
fn node()      -> ElementsRpc { ElementsRpc::node(node_url(),                   RPC_USER, RPC_PASS) }

// ── server state ─────────────────────────────────────────────────────────────

struct AppState {
    offers: Vec<Offer>,
    assets: BTreeMap<String, String>, // name → display-order asset id
}

pub fn run(port: u16, net: Network) -> Result<()> {
    let _ = NETWORK.set(net);

    // Make sure the maker/taker wallets exist on the node.
    let n = node();
    n.ensure_wallet("mosaik-maker")?;
    n.ensure_wallet("mosaik-taker")?;

    // Testnet pre-seeds the test asset map from the bootstrap script.
    let mut initial_assets: BTreeMap<String, String> = BTreeMap::new();
    if net == Network::Testnet {
        if let Ok(path) = std::env::var(TESTNET_ASSETS_ENV) {
            initial_assets = load_assets_file(&path)?;
        }
    }

    let server = Server::http(("127.0.0.1", port))
        .map_err(|e| anyhow!("could not bind 127.0.0.1:{port}: {e}"))?;
    let state = Mutex::new(AppState {
        offers: Vec::new(),
        assets: initial_assets,
    });

    let label = match net { Network::Regtest => "regtest", Network::Testnet => "testnet" };
    println!("Mosaik wallet UI  →  http://127.0.0.1:{port}  [{label}]");
    println!("Backend: elementsd at {}  |  wallets: mosaik, mosaik-maker, mosaik-taker", node_url());
    if net == Network::Testnet {
        let st = state.lock().unwrap();
        if st.assets.is_empty() {
            println!("WARN: no testnet assets loaded. Run ./scripts/testnet.sh fund first.");
        } else {
            println!("Loaded {} test assets: {}", st.assets.len(),
                st.assets.keys().cloned().collect::<Vec<_>>().join(", "));
        }
    }

    for mut req in server.incoming_requests() {
        // Browser preflight for CORS (the IDE preview panel needs this).
        if req.method() == &Method::Options {
            let _ = req.respond(
                Response::from_string("")
                    .with_status_code(204)
                    .with_header(cors_header())
                    .with_header(Header::from_bytes(
                        &b"Access-Control-Allow-Methods"[..],
                        &b"GET, POST, OPTIONS"[..],
                    ).expect("header"))
                    .with_header(Header::from_bytes(
                        &b"Access-Control-Allow-Headers"[..],
                        &b"Content-Type"[..],
                    ).expect("header")),
            );
            continue;
        }

        let (status, payload) = route(&mut req, &state);
        if status == 0 {
            let hdr = Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..])
                .expect("header");
            let _ = req.respond(
                Response::from_string(INDEX_HTML).with_header(hdr).with_header(cors_header()),
            );
        } else {
            let body = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".into());
            let hdr = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                .expect("header");
            let _ = req.respond(
                Response::from_string(body).with_status_code(status)
                    .with_header(hdr).with_header(cors_header()),
            );
        }
    }
    Ok(())
}

fn cors_header() -> Header {
    Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).expect("header")
}

fn route(req: &mut Request, state: &Mutex<AppState>) -> (u16, Value) {
    let method = req.method().clone();
    let url = req.url().split('?').next().unwrap_or("/").to_string();

    let result: Result<Value> = match (&method, url.as_str()) {
        (Method::Get, "/") | (Method::Get, "/index.html") => return (0, Value::Null),
        (Method::Get,  "/api/state")        => api_state(state),
        (Method::Post, "/api/fund/maker")   => api_fund(state, &maker_rpc()),
        (Method::Post, "/api/fund/taker")   => api_fund(state, &taker_rpc()),
        (Method::Post, "/api/make-offer")   => api_make_offer(req, state),
        (Method::Post, "/api/take-offer")   => api_take_offer(req, state),
        (Method::Post, "/api/reclaim")      => api_reclaim(req, state),
        (Method::Post, "/api/contract")     => api_contract(req, state),
        _ => return (404, json!({ "error": "not found" })),
    };

    match result {
        Ok(v)  => (200, v),
        Err(e) => (400, json!({ "error": e.to_string() })),
    }
}

fn body(req: &mut Request) -> Value {
    let mut raw = String::new();
    let _ = req.as_reader().read_to_string(&mut raw);
    serde_json::from_str(&raw).unwrap_or(Value::Null)
}

fn raw_balance(balances: &Value, asset: &str) -> u64 {
    balances.get(asset).and_then(Value::as_f64)
        .map(|v| (v * 1e8).round() as u64).unwrap_or(0)
}

/// Issue USDT + EURx (regtest only). On testnet the assets are pre-issued by
/// scripts/testnet.sh and loaded from disk at startup; this fn is a no-op for
/// testnet (returns the already-loaded map).
fn ensure_assets(state: &Mutex<AppState>) -> Result<BTreeMap<String, String>> {
    {
        let st = state.lock().unwrap();
        if !st.assets.is_empty() { return Ok(st.assets.clone()); }
    }
    if !is_regtest() {
        anyhow::bail!(
            "no testnet assets loaded. Run ./scripts/testnet.sh fund to issue them."
        );
    }
    let mut st = state.lock().unwrap();
    let t = treasury();
    for name in TEST_ASSETS {
        let (id, _) = t.issue_asset(100_000.0, 0.0)?;
        st.assets.insert(name.to_string(), id);
    }
    t.generate(1)?;
    Ok(st.assets.clone())
}

fn load_assets_file(path: &str) -> Result<BTreeMap<String, String>> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| anyhow!("read {path}: {e}"))?;
    let v: Value = serde_json::from_str(&raw)
        .map_err(|e| anyhow!("parse {path}: {e}"))?;
    let obj = v.as_object().ok_or_else(|| anyhow!("{path}: expected JSON object"))?;
    let mut out = BTreeMap::new();
    for (name, val) in obj {
        let s = val.as_str().ok_or_else(|| anyhow!("{path}/{name}: expected string"))?;
        out.insert(name.clone(), s.to_string());
    }
    Ok(out)
}

fn label_to_id(assets: &BTreeMap<String, String>, label: &str, lbtc: &str) -> Result<String> {
    match label {
        "BTC" | "L-BTC" | "LBTC" => Ok(lbtc.to_string()),
        name => assets.get(name).cloned()
            .ok_or_else(|| anyhow!("unknown asset '{name}'")),
    }
}

fn id_to_name(assets: &BTreeMap<String, String>, id: &str, lbtc: &str) -> String {
    if id == lbtc { return "L-BTC".to_string(); }
    assets.iter().find(|(_, v)| v.as_str() == id)
        .map(|(k, _)| k.clone())
        .unwrap_or_else(|| format!("{}…", &id[..id.len().min(8)]))
}

// ── endpoints ────────────────────────────────────────────────────────────────

fn wallet_snapshot(rpc: &ElementsRpc, assets: &BTreeMap<String, String>) -> Result<Value> {
    let balances = rpc.balances()?;
    let mut map = serde_json::Map::new();
    for (name, id) in assets {
        map.insert(name.clone(), json!(raw_balance(&balances, id)));
    }
    Ok(json!({
        "address":   rpc.new_unconfidential_address()?,
        "lbtc_sats": raw_balance(&balances, "bitcoin"),
        "assets":    map,
    }))
}

fn api_state(state: &Mutex<AppState>) -> Result<Value> {
    let lbtc = node().policy_asset()?;
    let (offers, assets) = {
        let st = state.lock().unwrap();
        (st.offers.clone(), st.assets.clone())
    };

    let offers: Vec<Value> = offers.iter().enumerate().map(|(i, o)| {
        let compiled = o.tessera.compile().ok();
        let mut ab = o.tessera.asset_b.to_vec(); ab.reverse();
        let asset_b = hex::encode(ab);
        let address = compiled.as_ref().and_then(|c| match network() {
            Network::Testnet => c.address_for(
                &simplicityhl::elements::AddressParams::LIQUID_TESTNET,
            ).ok(),
            Network::Regtest => c.address().ok(),
        }).map(|a| a.to_string());
        json!({
            "index":            i,
            "outpoint":         o.outpoint,
            "amount_a":         o.amount_a,
            "amount_b":         o.tessera.amount_b,
            "lock_name":        id_to_name(&assets, &o.asset_a, &lbtc),
            "want_name":        id_to_name(&assets, &asset_b, &lbtc),
            "timeout":          o.tessera.timeout,
            "maker_address":    o.maker_address,
            "covenant_address": address,
            "cmr":              compiled.as_ref().map(|c| c.cmr_hex()),
            "status":           "ready",
        })
    }).collect();

    Ok(json!({
        "network":     if is_regtest() { "regtest" } else { "testnet" },
        "block_count": node().block_count()?,
        "assets":      assets.keys().cloned().collect::<Vec<_>>(),
        "maker":       wallet_snapshot(&maker_rpc(), &assets)?,
        "taker":       wallet_snapshot(&taker_rpc(), &assets)?,
        "offers":      offers,
    }))
}

/// Send funds from the treasury to a wallet.
///
/// Regtest: 1 L-BTC + 100 of each test asset, mine to confirm.
/// Testnet: 0.001 L-BTC + 100 of each test asset; no mining (block ~1 min).
fn api_fund(state: &Mutex<AppState>, target: &ElementsRpc) -> Result<Value> {
    let assets = ensure_assets(state)?;
    let treasury = treasury();

    let (lbtc_amount, asset_amount, funded_label) = if is_regtest() {
        (1.0, 100.0, "1 L-BTC + 100 of each asset")
    } else {
        // Testnet treasury starts with one faucet drop (~100k sats) minus the
        // fees spent during issuance, leaving room for ~5 fund calls before it
        // empties. The user can re-run ./scripts/testnet.sh fund to top up.
        (0.00010, 10.0, "0.0001 L-BTC + 10 of each asset (testnet: ~1 min to confirm)")
    };

    let addr = target.new_unconfidential_address()?;
    let lbtc_txid = treasury.send_to_address(&addr, lbtc_amount)
        .map_err(|e| {
            if e.to_string().contains("Insufficient funds") && !is_regtest() {
                anyhow!("treasury out of L-BTC — run ./scripts/testnet.sh fund to top up")
            } else { e }
        })?;
    for id in assets.values() {
        let a = target.new_unconfidential_address()?;
        treasury.send_asset_to(&a, asset_amount, id)?;
    }
    if is_regtest() { treasury.generate(1)?; }
    Ok(json!({ "ok": true, "txid": lbtc_txid, "funded": funded_label }))
}

fn api_make_offer(req: &mut Request, state: &Mutex<AppState>) -> Result<Value> {
    let b = body(req);
    let amount_a = b.get("amount_a").and_then(Value::as_u64).ok_or_else(|| anyhow!("amount_a"))?;
    let amount_b = b.get("amount_b").and_then(Value::as_u64).ok_or_else(|| anyhow!("amount_b"))?;
    let timeout  = b.get("timeout").and_then(Value::as_u64).unwrap_or(500) as u32;
    let lock = b.get("lock").and_then(Value::as_str).unwrap_or("BTC");
    let want = b.get("want").and_then(Value::as_str).unwrap_or("USDT");
    if lock == want {
        anyhow::bail!("lock and want assets must differ");
    }

    let maker = maker_rpc();
    let lbtc   = maker.policy_asset()?;
    let assets = { state.lock().unwrap().assets.clone() };
    let maker_address = maker.new_unconfidential_address()?;

    let want_id = label_to_id(&assets, want, &lbtc)?;
    let tessera = if want_id == lbtc {
        lbtc_tessera(&maker, &maker_address, amount_b, timeout, demo_maker_pk())?
    } else {
        tessera_for(&maker, &maker_address, &want_id, amount_b, timeout, demo_maker_pk())?
    };
    let lock_label = if lock == "BTC" { "BTC".to_string() } else { label_to_id(&assets, lock, &lbtc)? };
    let offer = MosaikMaker::new(maker).make_offer(&lock_label, amount_a, &tessera, &maker_address)?;

    let mut st = state.lock().unwrap();
    let index = st.offers.len();
    st.offers.push(offer.clone());
    Ok(json!({ "ok": true, "index": index, "outpoint": offer.outpoint, "maker_address": offer.maker_address }))
}

fn api_take_offer(req: &mut Request, state: &Mutex<AppState>) -> Result<Value> {
    let b = body(req);
    let index = b.get("index").and_then(Value::as_u64).ok_or_else(|| anyhow!("index"))? as usize;
    let cheat  = Cheat::parse(b.get("cheat").and_then(Value::as_str).unwrap_or("none"));
    let offer = { state.lock().unwrap().offers.get(index).cloned()
        .ok_or_else(|| anyhow!("no offer #{index}"))? };

    let result = MosaikTaker::new(taker_rpc()).settle(&offer, cheat);
    match (cheat, result) {
        (Cheat::None, Ok(txid)) => {
            if is_regtest() { treasury().generate(1)?; }
            let mut st = state.lock().unwrap();
            if index < st.offers.len() { st.offers.remove(index); }
            Ok(json!({ "ok": true, "txid": txid }))
        }
        (Cheat::None, Err(e)) => Err(e),
        (_, Err(e)) => Ok(json!({ "ok": true, "rejected": true, "reason": e.to_string() })),
        (_, Ok(txid)) => {
            if is_regtest() { treasury().generate(1)?; }
            Ok(json!({ "ok": false, "covenant_bug": true, "txid": txid,
                        "reason": "a fraudulent settlement was accepted on-chain" }))
        }
    }
}

fn api_reclaim(req: &mut Request, state: &Mutex<AppState>) -> Result<Value> {
    let b = body(req);
    let index = b.get("index").and_then(Value::as_u64).ok_or_else(|| anyhow!("index"))? as usize;
    let offer = { state.lock().unwrap().offers.get(index).cloned()
        .ok_or_else(|| anyhow!("no offer #{index}"))? };
    let txid = MosaikMaker::new(maker_rpc()).reclaim(&offer, &DEMO_MAKER_SECRET)?;
    if is_regtest() { treasury().generate(1)?; }
    let mut st = state.lock().unwrap();
    if index < st.offers.len() { st.offers.remove(index); }
    Ok(json!({ "ok": true, "txid": txid }))
}

fn api_contract(req: &mut Request, state: &Mutex<AppState>) -> Result<Value> {
    let b = body(req);
    let index = b.get("index").and_then(Value::as_u64).ok_or_else(|| anyhow!("index"))? as usize;
    let offer = { state.lock().unwrap().offers.get(index).cloned()
        .ok_or_else(|| anyhow!("no offer #{index}"))? };
    let compiled = offer.tessera.compile()?;
    let address = match network() {
        Network::Testnet => compiled.address_for(
            &simplicityhl::elements::AddressParams::LIQUID_TESTNET)?,
        Network::Regtest => compiled.address()?,
    };
    Ok(json!({
        "source":  offer.tessera.render(),
        "cmr":     compiled.cmr_hex(),
        "address": address.to_string(),
        "settle":  "Taker path: spends the UTXO if output 0 pays the maker exactly \
                    `amount_b` of `asset_b`.",
        "refund":  "Maker path: once the chain reaches the refund block height the \
                    maker reclaims the locked asset with a BIP-340 signature.",
    }))
}

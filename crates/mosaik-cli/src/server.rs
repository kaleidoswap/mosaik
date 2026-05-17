//! Mosaik trading UI — HTTP server for the covenant DEX.
//!
//! Two networks, picked at startup via `--network`:
//!
//!   * `regtest` (default) — local Elements regtest on port 7040.
//!   * `testnet`           — local Elements node on port 7041 running
//!                            `chain=liquidtestnet`.
//!
//! Serves the React trading UI (from `ui/dist/` if built, else the baked-in
//! HTML fallback) and exposes API surfaces:
//!
//! **Market Data API**
//!   GET  /api/orderbook      aggregated bid/ask levels
//!   GET  /api/trades         recent market trades
//!   GET  /api/chart          OHLCV candles (synthesised for demo)
//!   GET  /api/depth          cumulative depth chart data
//!   GET  /api/user/orders    current user's open orders
//!   POST /api/cancel         cancel (reclaim) an open order
//!
//! **Covenant DEX API**
//!   GET  /api/state
//!   POST /api/fund/maker|taker
//!   POST /api/make-offer | /api/take-offer | /api/reclaim | /api/contract

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{anyhow, Result};
use mosaik_core::lwk::{LwkMaker, LwkNode, LwkTaker, LwkWallet};
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

const TEST_ASSETS: [&str; 2] = ["USDT", "EURx"];
const TESTNET_ASSETS_ENV: &str = "MOSAIK_TESTNET_ASSETS_FILE";

/// Fixed RFQ pricing (sats of 'to' per sat of 'from', before fee).
const BTC_USDT: f64 = 48_732.0;
const BTC_EURX: f64 = 44_500.0;
const FEE_BPS: f64 = 10.0; // 0.1 %

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

#[derive(Clone)]
struct SwapRecord {
    from_ticker: String,
    to_ticker: String,
    from_amount: u64,
    to_amount: u64,
    txid: String,
    timestamp: u64,
}

struct AppState {
    offers: Vec<Offer>,
    assets: BTreeMap<String, String>,
    history: Vec<SwapRecord>,
    lwk: Option<Arc<LwkNode>>,
}

/// Path to the React build output, if present.
fn dist_dir() -> Option<std::path::PathBuf> {
    for candidate in &["ui/dist", "../ui/dist", "webapp/dist", "../webapp/dist"] {
        let p = std::path::Path::new(candidate);
        if p.join("index.html").exists() {
            return Some(p.to_path_buf());
        }
    }
    None
}

fn mime_for(path: &str) -> &'static str {
    if path.ends_with(".js") || path.ends_with(".mjs") {
        "application/javascript"
    } else if path.ends_with(".css") {
        "text/css"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else if path.ends_with(".woff2") {
        "font/woff2"
    } else {
        "text/html; charset=utf-8"
    }
}

fn serve_static(req: Request, url_path: &str) {
    if let Some(dist) = dist_dir() {
        let file = if url_path == "/" || url_path == "/index.html" || !url_path.contains('.') {
            dist.join("index.html")
        } else {
            dist.join(url_path.trim_start_matches('/'))
        };

        if let Ok(bytes) = std::fs::read(&file) {
            let mime = mime_for(url_path);
            let header = Header::from_bytes(b"Content-Type", mime.as_bytes())
                .expect("valid header");
            let _ = req.respond(Response::from_data(bytes).with_header(header));
            return;
        }
        if let Ok(bytes) = std::fs::read(dist.join("index.html")) {
            let header =
                Header::from_bytes(b"Content-Type", b"text/html; charset=utf-8").unwrap();
            let _ = req.respond(Response::from_data(bytes).with_header(header));
            return;
        }
    }
    let header =
        Header::from_bytes(b"Content-Type", b"text/html; charset=utf-8").expect("valid header");
    let _ = req.respond(Response::from_string(INDEX_HTML).with_header(header));
}

pub fn run(port: u16, net: Network) -> Result<()> {
    let _ = NETWORK.set(net);

    let mut initial_assets: BTreeMap<String, String> = BTreeMap::new();
    let lwk = if net == Network::Testnet {
        println!("Initializing LWK wallets for Liquid testnet…");
        let node = LwkNode::testnet()?;
        println!("Syncing wallets with Esplora…");
        node.sync()?;
        println!("Treasury address: {}", node.treasury.address()?);
        if let Ok(path) = std::env::var(TESTNET_ASSETS_ENV) {
            initial_assets = load_assets_file(&path)?;
        }
        Some(Arc::new(node))
    } else {
        let n = node();
        n.ensure_wallet("mosaik-maker")?;
        n.ensure_wallet("mosaik-taker")?;
        None
    };

    let server = Server::http(("0.0.0.0", port))
        .map_err(|e| anyhow!("could not bind 0.0.0.0:{port}: {e}"))?;
    let state = Mutex::new(AppState {
        offers: Vec::new(),
        assets: initial_assets,
        history: Vec::new(),
        lwk,
    });

    let label = match net { Network::Regtest => "regtest", Network::Testnet => "testnet" };
    if dist_dir().is_some() {
        println!("Mosaik trading UI  ->  http://127.0.0.1:{port}  (React build)  [{label}]");
    } else {
        println!("Mosaik wallet UI   ->  http://127.0.0.1:{port}  (fallback HTML)  [{label}]");
    }
    if is_regtest() {
        println!("Backend: elementsd at {}  |  wallets: mosaik, mosaik-maker, mosaik-taker", node_url());
    } else {
        println!("Backend: LWK + Blockstream Esplora (testnet)");
    }
    if net == Network::Testnet {
        let st = state.lock().unwrap();
        if st.assets.is_empty() {
            println!("WARN: no testnet assets loaded. Fund treasury address above, then restart.");
        } else {
            println!("Loaded {} test assets: {}", st.assets.len(),
                st.assets.keys().cloned().collect::<Vec<_>>().join(", "));
        }
    }

    for mut req in server.incoming_requests() {
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

        let method = req.method().clone();
        let url = req.url().split('?').next().unwrap_or("/").to_string();

        // Static assets and SPA root
        let is_static = method == Method::Get
            && (url == "/"
                || url == "/index.html"
                || url.starts_with("/assets/")
                || url.starts_with("/fonts/"));
        if is_static {
            serve_static(req, &url);
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
        (Method::Get,  "/api/state")        => api_state(state),
        (Method::Post, "/api/fund/maker")   => api_fund(state, "maker"),
        (Method::Post, "/api/fund/taker")   => api_fund(state, "taker"),
        (Method::Post, "/api/make-offer")   => api_make_offer(req, state),
        (Method::Post, "/api/take-offer")   => api_take_offer(req, state),
        (Method::Post, "/api/reclaim")      => api_reclaim(req, state),
        (Method::Post, "/api/contract")     => api_contract(req, state),
        // Market data API
        (Method::Get, "/api/orderbook")     => api_orderbook(req, state),
        (Method::Get, "/api/trades")        => api_trades(req, state),
        (Method::Get, "/api/chart")         => api_chart(req, state),
        (Method::Get, "/api/depth")         => api_depth(req, state),
        (Method::Get, "/api/user/orders")   => api_user_orders(state),
        (Method::Post, "/api/cancel")       => api_cancel(req, state),
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

fn ensure_assets(state: &Mutex<AppState>) -> Result<BTreeMap<String, String>> {
    {
        let st = state.lock().unwrap();
        if !st.assets.is_empty() { return Ok(st.assets.clone()); }
    }
    if !is_regtest() {
        anyhow::bail!("no testnet assets loaded. Fund treasury and restart, or set MOSAIK_TESTNET_ASSETS_FILE.");
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

fn norm(t: &str) -> &str {
    match t { "L-BTC" | "LBTC" => "BTC", other => other }
}

fn format_timestamp(ts: u64) -> String {
    let mins = (ts / 60) % 60;
    let hours = (ts / 3600) % 24;
    let days = (ts / 86400) % 31 + 1;
    let months = ((ts / 86400) / 31) % 12 + 1;
    format!("{:02}:{:02} {:02}/{:02}/{}", hours, mins, days, months, 2019)
}

// ── endpoints ────────────────────────────────────────────────────────────────

fn wallet_snapshot_rpc(rpc: &ElementsRpc, assets: &BTreeMap<String, String>) -> Result<Value> {
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

fn wallet_snapshot_lwk(wallet: &LwkWallet, lbtc: &str, assets: &BTreeMap<String, String>) -> Result<Value> {
    let balances = wallet.balances()?;
    let mut map = serde_json::Map::new();
    for (name, id) in assets {
        map.insert(name.clone(), json!(raw_balance(&balances, id)));
    }
    Ok(json!({
        "address":   wallet.unconfidential_address()?,
        "lbtc_sats": raw_balance(&balances, lbtc),
        "assets":    map,
    }))
}

fn api_state(state: &Mutex<AppState>) -> Result<Value> {
    let (offers, assets, lwk) = {
        let st = state.lock().unwrap();
        (st.offers.clone(), st.assets.clone(), st.lwk.clone())
    };

    let (lbtc, block_count, maker, taker) = if let Some(ref lwk) = lwk {
        let lbtc = lwk.policy_asset()?;
        let maker = wallet_snapshot_lwk(&lwk.maker, &lbtc, &assets)?;
        let taker = wallet_snapshot_lwk(&lwk.taker, &lbtc, &assets)?;
        (lbtc, lwk.block_count()?, maker, taker)
    } else {
        let lbtc = node().policy_asset()?;
        let maker = wallet_snapshot_rpc(&maker_rpc(), &assets)?;
        let taker = wallet_snapshot_rpc(&taker_rpc(), &assets)?;
        (lbtc, node().block_count()?, maker, taker)
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
        "block_count": block_count,
        "assets":      assets.keys().cloned().collect::<Vec<_>>(),
        "maker":       maker,
        "taker":       taker,
        "offers":      offers,
    }))
}

fn api_fund(state: &Mutex<AppState>, target_name: &str) -> Result<Value> {
    let assets = ensure_assets(state)?;
    let (lbtc_amount, asset_amount, funded_label) = if is_regtest() {
        (1.0, 100.0, "1 L-BTC + 100 of each asset")
    } else {
        (0.00010, 10.0, "0.0001 L-BTC + 10 of each asset (testnet: ~1 min to confirm)")
    };

    let st = state.lock().unwrap();
    if let Some(ref lwk) = st.lwk {
        let target = match target_name {
            "maker" => &lwk.maker,
            "taker" => &lwk.taker,
            _ => anyhow::bail!("unknown wallet"),
        };
        let addr = target.unconfidential_address()?;
        let lbtc_txid = lwk.treasury.send_to_address(&addr, lbtc_amount, None)
            .map_err(|e| {
                if e.to_string().contains("Insufficient funds") {
                    anyhow!("treasury out of L-BTC — fund the treasury address and retry")
                } else { e }
            })?;
        for id in assets.values() {
            lwk.treasury.sync().ok();
            let a = target.unconfidential_address()?;
            lwk.treasury.send_to_address(&a, asset_amount, Some(id))?;
        }
        lwk.sync().ok();
        Ok(json!({ "ok": true, "txid": lbtc_txid, "funded": funded_label }))
    } else {
        let target_rpc = match target_name {
            "maker" => maker_rpc(),
            "taker" => taker_rpc(),
            _ => anyhow::bail!("unknown wallet"),
        };
        let treasury = treasury();
        let addr = target_rpc.new_unconfidential_address()?;
        let lbtc_txid = treasury.send_to_address(&addr, lbtc_amount)
            .map_err(|e| {
                if e.to_string().contains("Insufficient funds") && !is_regtest() {
                    anyhow!("treasury out of L-BTC — run ./scripts/testnet.sh fund to top up")
                } else { e }
            })?;
        for id in assets.values() {
            let a = target_rpc.new_unconfidential_address()?;
            treasury.send_asset_to(&a, asset_amount, id)?;
        }
        if is_regtest() { treasury.generate(1)?; }
        Ok(json!({ "ok": true, "txid": lbtc_txid, "funded": funded_label }))
    }
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

    let st = state.lock().unwrap();
    let assets = st.assets.clone();
    let lwk = st.lwk.clone();
    drop(st);

    let offer = if let Some(ref lwk) = lwk {
        let lbtc = lwk.policy_asset()?;
        let maker_address = lwk.maker.unconfidential_address()?;
        let want_id = label_to_id(&assets, want, &lbtc)?;
        let addr: simplicityhl::elements::Address = maker_address.parse()
            .map_err(|e| anyhow!("invalid maker address: {e}"))?;
        let script = addr.script_pubkey();
        let spk = script.as_bytes();
        let tessera = mosaik_core::build_tessera(spk, &want_id, amount_b, timeout, demo_maker_pk())?;
        let lock_label = if lock == "BTC" { "BTC".to_string() } else { label_to_id(&assets, lock, &lbtc)? };
        LwkMaker::new(&lwk.maker, lwk).make_offer(&lock_label, amount_a, &tessera, &maker_address)?
    } else {
        let maker = maker_rpc();
        let lbtc = maker.policy_asset()?;
        let maker_address = maker.new_unconfidential_address()?;
        let want_id = label_to_id(&assets, want, &lbtc)?;
        let tessera = if want_id == lbtc {
            lbtc_tessera(&maker, &maker_address, amount_b, timeout, demo_maker_pk())?
        } else {
            tessera_for(&maker, &maker_address, &want_id, amount_b, timeout, demo_maker_pk())?
        };
        let lock_label = if lock == "BTC" { "BTC".to_string() } else { label_to_id(&assets, lock, &lbtc)? };
        MosaikMaker::new(maker).make_offer(&lock_label, amount_a, &tessera, &maker_address)?
    };

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

    let lwk = state.lock().unwrap().lwk.clone();
    let result = if let Some(ref lwk) = lwk {
        LwkTaker::new(&lwk.taker, lwk).settle(&offer, cheat)
    } else {
        MosaikTaker::new(taker_rpc()).settle(&offer, cheat)
    };

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

    let lwk = state.lock().unwrap().lwk.clone();
    let txid = if let Some(ref lwk) = lwk {
        LwkMaker::new(&lwk.maker, lwk).reclaim(&offer, &DEMO_MAKER_SECRET)?
    } else {
        MosaikMaker::new(maker_rpc()).reclaim(&offer, &DEMO_MAKER_SECRET)?
    };

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
        "settle":  "Taker path: spends the UTXO if output 0 pays the maker exactly \" +
                    \"`amount_b` of `asset_b`.",
        "refund":  "Maker path: once the chain reaches the refund block height the \" +
                    \"maker reclaims the locked asset with a BIP-340 signature.",
    }))
}

// ── Market data API ---------------------------------------------------------

fn api_orderbook(_req: &mut Request, state: &Mutex<AppState>) -> Result<Value> {
    let st = state.lock().unwrap();
    let lbtc = if let Some(ref lwk) = st.lwk {
        lwk.policy_asset().unwrap_or_default()
    } else {
        node().policy_asset().unwrap_or_default()
    };

    let mut bids: Vec<Value> = Vec::new();
    let mut asks: Vec<Value> = Vec::new();

    for offer in &st.offers {
        let mut ab = offer.tessera.asset_b.to_vec();
        ab.reverse();
        let asset_b = hex::encode(ab);
        let lock_name = id_to_name(&st.assets, &offer.asset_a, &lbtc);
        let want_name = id_to_name(&st.assets, &asset_b, &lbtc);

        if offer.amount_a == 0 { continue; }

        if lock_name == "L-BTC" && want_name == "USDT" {
            let price = offer.tessera.amount_b as f64 / offer.amount_a as f64;
            let price = (price * 100.0).round() / 100.0;
            asks.push(json!({
                "price": price,
                "amount": offer.amount_a as f64 / 1e8,
                "total": offer.tessera.amount_b as f64 / 1e8,
            }));
        } else if lock_name == "USDT" && want_name == "L-BTC" {
            let price = offer.amount_a as f64 / offer.tessera.amount_b as f64;
            let price = (price * 100.0).round() / 100.0;
            bids.push(json!({
                "price": price,
                "amount": offer.tessera.amount_b as f64 / 1e8,
                "total": offer.amount_a as f64 / 1e8,
            }));
        } else if lock_name == "L-BTC" && want_name == "EURx" {
            let price = offer.tessera.amount_b as f64 / offer.amount_a as f64;
            let price = (price * 100.0).round() / 100.0;
            asks.push(json!({
                "price": price,
                "amount": offer.amount_a as f64 / 1e8,
                "total": offer.tessera.amount_b as f64 / 1e8,
            }));
        } else if lock_name == "EURx" && want_name == "L-BTC" {
            let price = offer.amount_a as f64 / offer.tessera.amount_b as f64;
            let price = (price * 100.0).round() / 100.0;
            bids.push(json!({
                "price": price,
                "amount": offer.tessera.amount_b as f64 / 1e8,
                "total": offer.amount_a as f64 / 1e8,
            }));
        }
    }

    bids.sort_by(|a, b| {
        let pa = a.get("price").and_then(Value::as_f64).unwrap_or(0.0);
        let pb = b.get("price").and_then(Value::as_f64).unwrap_or(0.0);
        pb.partial_cmp(&pa).unwrap_or(std::cmp::Ordering::Equal)
    });
    asks.sort_by(|a, b| {
        let pa = a.get("price").and_then(Value::as_f64).unwrap_or(0.0);
        let pb = b.get("price").and_then(Value::as_f64).unwrap_or(0.0);
        pa.partial_cmp(&pb).unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut bid_cum = 0.0;
    for b in &mut bids {
        let amt = b.get("amount").and_then(Value::as_f64).unwrap_or(0.0);
        bid_cum += amt;
        b.as_object_mut().unwrap().insert("total".to_string(), json!(bid_cum));
    }
    let mut ask_cum = 0.0;
    for a in &mut asks {
        let amt = a.get("amount").and_then(Value::as_f64).unwrap_or(0.0);
        ask_cum += amt;
        a.as_object_mut().unwrap().insert("total".to_string(), json!(ask_cum));
    }

    Ok(json!({ "bids": bids, "asks": asks }))
}

fn api_trades(req: &mut Request, state: &Mutex<AppState>) -> Result<Value> {
    let url = req.url().to_string();
    let limit = url.split('?').nth(1).unwrap_or("")
        .split('&').find_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            if k == "limit" { v.parse::<usize>().ok() } else { None }
        }).unwrap_or(50);

    let st = state.lock().unwrap();
    let records: Vec<Value> = st.history.iter().rev().take(limit).map(|r| {
        let side = if r.to_ticker == "L-BTC" || r.to_ticker == "BTC" { "buy" } else { "sell" };
        let price = if r.to_amount > 0 { r.from_amount as f64 / r.to_amount as f64 } else { 0.0 };
        let dt = format_timestamp(r.timestamp);
        json!({
            "price": (price * 100.0).round() / 100.0,
            "amount": r.to_amount as f64 / 1e8,
            "side": side,
            "time": dt,
        })
    }).collect();
    Ok(json!(records))
}

fn api_chart(req: &mut Request, _state: &Mutex<AppState>) -> Result<Value> {
    let url = req.url().to_string();
    let tf = url.split('?').nth(1).unwrap_or("")
        .split('&').find_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            if k == "tf" { Some(v.to_string()) } else { None }
        }).unwrap_or_else(|| "1h".to_string());

    let limit = url.split('?').nth(1).unwrap_or("")
        .split('&').find_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            if k == "limit" { v.parse::<usize>().ok() } else { None }
        }).unwrap_or(200);

    let seconds_per_candle: i64 = match tf.as_str() {
        "1m" => 60, "15m" => 900, "1h" => 3600,
        "4h" => 14400, "1d" => 86400, "1w" => 604800,
        _ => 3600,
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;

    let base_price = BTC_USDT;
    let mut candles = Vec::new();
    let mut price = base_price;

    for i in (0..limit as i64).rev() {
        let time = (now - i * seconds_per_candle) * 1000;
        let volatility = price * 0.005;
        let change = (i * 7919 % 100) as f64 / 100.0 * volatility * 2.0 - volatility;
        let open = price;
        let close = price + change;
        let high = open.max(close) + volatility * 0.3;
        let low = open.min(close) - volatility * 0.3;
        let volume = (i * 104729 % 1000) as f64 + 100.0;
        candles.push(json!({
            "time": time,
            "open": (open * 100.0).round() / 100.0,
            "high": (high * 100.0).round() / 100.0,
            "low": (low * 100.0).round() / 100.0,
            "close": (close * 100.0).round() / 100.0,
            "volume": (volume * 100.0).round() / 100.0,
        }));
        price = close;
    }

    Ok(json!(candles))
}

fn api_depth(_req: &mut Request, state: &Mutex<AppState>) -> Result<Value> {
    let st = state.lock().unwrap();
    let lbtc = if let Some(ref lwk) = st.lwk {
        lwk.policy_asset().unwrap_or_default()
    } else {
        node().policy_asset().unwrap_or_default()
    };

    let mut bid_points: Vec<(f64, f64)> = Vec::new();
    let mut ask_points: Vec<(f64, f64)> = Vec::new();

    for offer in &st.offers {
        let mut ab = offer.tessera.asset_b.to_vec();
        ab.reverse();
        let asset_b = hex::encode(ab);
        let lock_name = id_to_name(&st.assets, &offer.asset_a, &lbtc);
        let want_name = id_to_name(&st.assets, &asset_b, &lbtc);

        if offer.amount_a == 0 { continue; }

        if lock_name == "L-BTC" && (want_name == "USDT" || want_name == "EURx") {
            let price = offer.tessera.amount_b as f64 / offer.amount_a as f64;
            let price = (price * 100.0).round() / 100.0;
            ask_points.push((price, offer.amount_a as f64 / 1e8));
        } else if (lock_name == "USDT" || lock_name == "EURx") && want_name == "L-BTC" {
            let price = offer.amount_a as f64 / offer.tessera.amount_b as f64;
            let price = (price * 100.0).round() / 100.0;
            bid_points.push((price, offer.tessera.amount_b as f64 / 1e8));
        }
    }

    bid_points.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    ask_points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut points: Vec<Value> = Vec::new();
    let mut cum = 0.0;
    for (price, amt) in bid_points {
        cum += amt;
        points.push(json!({ "price": price, "bidDepth": cum, "askDepth": 0 }));
    }
    cum = 0.0;
    for (price, amt) in ask_points {
        cum += amt;
        points.push(json!({ "price": price, "bidDepth": 0, "askDepth": cum }));
    }

    points.sort_by(|a, b| {
        let pa = a.get("price").and_then(Value::as_f64).unwrap_or(0.0);
        let pb = b.get("price").and_then(Value::as_f64).unwrap_or(0.0);
        pa.partial_cmp(&pb).unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(json!(points))
}

fn api_user_orders(state: &Mutex<AppState>) -> Result<Value> {
    let st = state.lock().unwrap();
    let lbtc = if let Some(ref lwk) = st.lwk {
        lwk.policy_asset().unwrap_or_default()
    } else {
        node().policy_asset().unwrap_or_default()
    };

    let orders: Vec<Value> = st.offers.iter().enumerate().map(|(i, o)| {
        let mut ab = o.tessera.asset_b.to_vec();
        ab.reverse();
        let asset_b = hex::encode(ab);
        let lock_name = id_to_name(&st.assets, &o.asset_a, &lbtc);
        let want_name = id_to_name(&st.assets, &asset_b, &lbtc);
        let is_sell = lock_name == "L-BTC";
        let price = if is_sell {
            if o.amount_a > 0 { (o.tessera.amount_b as f64 / o.amount_a as f64 * 100.0).round() / 100.0 } else { 0.0 }
        } else {
            if o.tessera.amount_b > 0 { (o.amount_a as f64 / o.tessera.amount_b as f64 * 100.0).round() / 100.0 } else { 0.0 }
        };
        let pair = format!("{}/{}", lock_name, want_name);
        let typ = if is_sell { "SELL" } else { "BUY" };
        let amount = if is_sell { o.amount_a as f64 / 1e8 } else { o.tessera.amount_b as f64 / 1e8 };
        let total = if is_sell { o.tessera.amount_b as f64 / 1e8 } else { o.amount_a as f64 / 1e8 };
        json!({
            "id": i, "type": typ, "pair": pair, "price": price,
            "amount": amount, "filled": 0.0, "total": total,
            "status": "open", "outpoint": o.outpoint,
        })
    }).collect();

    Ok(json!(orders))
}

fn api_cancel(req: &mut Request, state: &Mutex<AppState>) -> Result<Value> {
    api_reclaim(req, state)
}

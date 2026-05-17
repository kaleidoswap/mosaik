//! Mosaik wallet UI — a tiny HTTP server.
//!
//! Two networks, picked at startup via `--network`:
//!
//!   * `regtest` — local Elements regtest on port 7040. Two wallet roles
//!                 (mosaik-maker, mosaik-taker), test-asset issuance, and a
//!                 mining helper. Full DEX experience.
//!
//!   * `testnet` — Liquid testnet without a local node. Mirrors the Blockstream
//!                 simplicity-codespace pattern: public faucet funds covenant
//!                 addresses directly; public Esplora reads chain state and
//!                 broadcasts; `hal-simplicity` builds PSETs. L-BTC only.

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use anyhow::{anyhow, Result};
use mosaik_core::rpc::ElementsRpc;
use mosaik_core::testnet as tn;
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

// ── network ──────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Network {
    Regtest,
    Testnet,
}

static NETWORK: OnceLock<Network> = OnceLock::new();

fn network() -> Network { *NETWORK.get().unwrap_or(&Network::Regtest) }

fn regtest_node_url() -> &'static str { "http://127.0.0.1:7040" }

fn treasury()    -> ElementsRpc { ElementsRpc::wallet(regtest_node_url(), "mosaik",       RPC_USER, RPC_PASS) }
fn maker_rpc()   -> ElementsRpc { ElementsRpc::wallet(regtest_node_url(), "mosaik-maker", RPC_USER, RPC_PASS) }
fn taker_rpc()   -> ElementsRpc { ElementsRpc::wallet(regtest_node_url(), "mosaik-taker", RPC_USER, RPC_PASS) }
fn regtest_node() -> ElementsRpc { ElementsRpc::node(regtest_node_url(), RPC_USER, RPC_PASS) }

// ── server state ─────────────────────────────────────────────────────────────

struct AppState {
    offers: Vec<Offer>,                 // regtest offers (covenant lifecycle)
    assets: BTreeMap<String, String>,   // regtest: name → display-order asset id
    testnet_offers: Vec<tn::TestnetOffer>, // testnet offers (with faucet txids)
}

pub fn run(port: u16, net: Network) -> Result<()> {
    let _ = NETWORK.set(net);

    // Regtest: make sure maker/taker wallets exist on the node.
    if net == Network::Regtest {
        let node = regtest_node();
        node.ensure_wallet("mosaik-maker")?;
        node.ensure_wallet("mosaik-taker")?;
    }

    let server = Server::http(("127.0.0.1", port))
        .map_err(|e| anyhow!("could not bind 127.0.0.1:{port}: {e}"))?;
    let state = Mutex::new(AppState {
        offers: Vec::new(),
        assets: BTreeMap::new(),
        testnet_offers: Vec::new(),
    });

    let label = match net { Network::Regtest => "regtest", Network::Testnet => "testnet" };
    println!("Mosaik wallet UI  →  http://127.0.0.1:{port}  [{label}]");
    match net {
        Network::Regtest => println!("Backend: elementsd at {}", regtest_node_url()),
        Network::Testnet => println!("Backend: Esplora at {} + public faucet", tn::ESPLORA_DEFAULT),
    }

    for mut req in server.incoming_requests() {
        // Browser preflight for CORS — answer once, here.
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
                Response::from_string(INDEX_HTML)
                    .with_header(hdr)
                    .with_header(cors_header()),
            );
        } else {
            let body = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".into());
            let hdr = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                .expect("header");
            let _ = req.respond(
                Response::from_string(body)
                    .with_status_code(status)
                    .with_header(hdr)
                    .with_header(cors_header()),
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
        (Method::Post, "/api/fund/maker")   => api_fund(state, /* taker = */ false),
        (Method::Post, "/api/fund/taker")   => api_fund(state, true),
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

// ── /api/state ───────────────────────────────────────────────────────────────

fn api_state(state: &Mutex<AppState>) -> Result<Value> {
    match network() {
        Network::Regtest => api_state_regtest(state),
        Network::Testnet => api_state_testnet(state),
    }
}

fn api_state_regtest(state: &Mutex<AppState>) -> Result<Value> {
    let lbtc = regtest_node().policy_asset()?;
    let (offers, assets) = {
        let st = state.lock().unwrap();
        (st.offers.clone(), st.assets.clone())
    };

    let offers: Vec<Value> = offers.iter().enumerate().map(|(i, o)| offer_to_json(i, o, &assets, &lbtc)).collect();

    Ok(json!({
        "network":     "regtest",
        "block_count": regtest_node().block_count()?,
        "assets":      assets.keys().cloned().collect::<Vec<_>>(),
        "maker":       wallet_snapshot_regtest(&maker_rpc(), &assets)?,
        "taker":       wallet_snapshot_regtest(&taker_rpc(), &assets)?,
        "offers":      offers,
    }))
}

fn api_state_testnet(state: &Mutex<AppState>) -> Result<Value> {
    let esplora = tn::Esplora::testnet();
    let height = esplora.block_height().unwrap_or(0);

    let tracked = { state.lock().unwrap().testnet_offers.clone() };
    let offers: Vec<Value> = tracked.iter().enumerate().map(|(i, to)| {
        let s = tn::probe_offer(&to.offer, &to.faucet_txid, &esplora);
        let compiled = to.offer.tessera.compile().ok();
        json!({
            "index":            i,
            "outpoint":         to.offer.outpoint,
            "amount_a":         to.offer.amount_a,
            "amount_b":         to.offer.tessera.amount_b,
            "lock_name":        "L-BTC",
            "want_name":        "L-BTC",
            "timeout":          to.offer.tessera.timeout,
            "maker_address":    to.offer.maker_address,
            "covenant_address": to.covenant_address,
            "cmr":              compiled.as_ref().map(|c| c.cmr_hex()),
            "faucet_txid":      to.faucet_txid,
            "status":           s.status,
            "value_sats":       s.value_sats,
            "confirmed_at":     s.confirmed_at,
        })
    }).collect();

    Ok(json!({
        "network":     "testnet",
        "block_count": height,
        "assets":      Vec::<String>::new(),
        "maker":       wallet_snapshot_testnet(&esplora, tn::DEMO_MAKER_ADDRESS),
        "taker":       wallet_snapshot_testnet(&esplora, tn::demo_taker_address()),
        "offers":      offers,
    }))
}

fn wallet_snapshot_regtest(rpc: &ElementsRpc, assets: &BTreeMap<String, String>) -> Result<Value> {
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

fn wallet_snapshot_testnet(esplora: &tn::Esplora, address: &str) -> Value {
    let sats = esplora.address_lbtc_balance(address).unwrap_or(0);
    json!({
        "address":   address,
        "lbtc_sats": sats,
        "assets":    {},
    })
}

fn raw_balance(balances: &Value, asset: &str) -> u64 {
    balances.get(asset).and_then(Value::as_f64)
        .map(|v| (v * 1e8).round() as u64).unwrap_or(0)
}

fn offer_to_json(i: usize, o: &Offer, assets: &BTreeMap<String, String>, lbtc: &str) -> Value {
    let compiled = o.tessera.compile().ok();
    let mut ab = o.tessera.asset_b.to_vec(); ab.reverse();
    let asset_b = hex::encode(ab);
    json!({
        "index":            i,
        "outpoint":         o.outpoint,
        "amount_a":         o.amount_a,
        "amount_b":         o.tessera.amount_b,
        "lock_name":        id_to_name(assets, &o.asset_a, lbtc),
        "want_name":        id_to_name(assets, &asset_b, lbtc),
        "timeout":          o.tessera.timeout,
        "maker_address":    o.maker_address,
        "covenant_address": compiled.as_ref().and_then(|c| c.address().ok()).map(|a| a.to_string()),
        "cmr":              compiled.as_ref().map(|c| c.cmr_hex()),
        "status":           "ready",
    })
}

fn id_to_name(assets: &BTreeMap<String, String>, id: &str, lbtc: &str) -> String {
    if id == lbtc { return "L-BTC".to_string(); }
    assets.iter().find(|(_, v)| v.as_str() == id)
        .map(|(k, _)| k.clone())
        .unwrap_or_else(|| format!("{}…", &id[..id.len().min(8)]))
}

// ── /api/fund/{maker,taker} ──────────────────────────────────────────────────

fn api_fund(state: &Mutex<AppState>, taker: bool) -> Result<Value> {
    match network() {
        Network::Regtest => {
            let target = if taker { taker_rpc() } else { maker_rpc() };
            api_fund_regtest(state, &target)
        }
        Network::Testnet => api_fund_testnet(taker),
    }
}

fn ensure_assets(state: &Mutex<AppState>) -> Result<BTreeMap<String, String>> {
    let mut st = state.lock().unwrap();
    if st.assets.is_empty() {
        let t = treasury();
        for name in TEST_ASSETS {
            let (id, _) = t.issue_asset(100_000.0, 0.0)?;
            st.assets.insert(name.to_string(), id);
        }
        t.generate(1)?;
    }
    Ok(st.assets.clone())
}

fn api_fund_regtest(state: &Mutex<AppState>, target: &ElementsRpc) -> Result<Value> {
    let assets = ensure_assets(state)?;
    let treasury = treasury();
    let addr = target.new_unconfidential_address()?;
    let lbtc_txid = treasury.send_to_address(&addr, 1.0)?;
    for id in assets.values() {
        let a = target.new_unconfidential_address()?;
        treasury.send_asset_to(&a, 100.0, id)?;
    }
    treasury.generate(1)?;
    Ok(json!({ "ok": true, "txid": lbtc_txid, "funded": "1 L-BTC + 100 of each asset" }))
}

fn api_fund_testnet(taker: bool) -> Result<Value> {
    let address = if taker { tn::demo_taker_address() } else { tn::DEMO_MAKER_ADDRESS };
    let faucet = tn::Faucet::testnet();
    let txid = faucet.request_lbtc(address)?;
    Ok(json!({
        "ok": true,
        "txid": txid,
        "funded": format!("faucet hit (~0.001 L-BTC, ~1 min to confirm)"),
        "explorer": format!("https://blockstream.info/liquidtestnet/tx/{txid}"),
    }))
}

// ── /api/make-offer ──────────────────────────────────────────────────────────

fn api_make_offer(req: &mut Request, state: &Mutex<AppState>) -> Result<Value> {
    match network() {
        Network::Regtest => api_make_offer_regtest(req, state),
        Network::Testnet => api_make_offer_testnet(req, state),
    }
}

fn api_make_offer_regtest(req: &mut Request, state: &Mutex<AppState>) -> Result<Value> {
    let b = body(req);
    let amount_a = b.get("amount_a").and_then(Value::as_u64).ok_or_else(|| anyhow!("amount_a"))?;
    let amount_b = b.get("amount_b").and_then(Value::as_u64).ok_or_else(|| anyhow!("amount_b"))?;
    let timeout  = b.get("timeout").and_then(Value::as_u64).unwrap_or(500) as u32;
    let lock = b.get("lock").and_then(Value::as_str).unwrap_or("BTC");
    let want = b.get("want").and_then(Value::as_str).unwrap_or("USDT");
    if lock == want { anyhow::bail!("lock and want assets must differ"); }

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

fn label_to_id(assets: &BTreeMap<String, String>, label: &str, lbtc: &str) -> Result<String> {
    match label {
        "BTC" | "L-BTC" | "LBTC" => Ok(lbtc.to_string()),
        name => assets.get(name).cloned()
            .ok_or_else(|| anyhow!("unknown asset '{name}' — fund a wallet to issue it")),
    }
}

fn api_make_offer_testnet(req: &mut Request, state: &Mutex<AppState>) -> Result<Value> {
    let b = body(req);
    // Testnet: L-BTC only. amount_b is what the maker receives. amount_a stored
    // for display only — the covenant UTXO is funded by the faucet (~100k sats).
    let amount_b = b.get("amount_b").and_then(Value::as_u64).ok_or_else(|| anyhow!("amount_b"))?;
    let amount_a = b.get("amount_a").and_then(Value::as_u64).unwrap_or(amount_b);
    let timeout  = b.get("timeout").and_then(Value::as_u64).unwrap_or(500) as u32;

    let tessera = tn::demo_tessera(amount_b, timeout, demo_maker_pk())?;
    let compiled = tessera.compile()?;
    let covenant_address = compiled.address()?.to_string();

    // Hit the faucet on the covenant address.
    let faucet = tn::Faucet::testnet();
    let faucet_txid = faucet.request_lbtc(&covenant_address)?;

    // The funding tx's covenant output is at vout 0 by convention on testnet
    // faucet payouts. The offer's outpoint reflects that.
    let outpoint = format!("{faucet_txid}:0");

    let offer = Offer {
        outpoint:      outpoint.clone(),
        asset_a:       tn::LBTC_TESTNET_DISPLAY.to_string(),
        amount_a,
        tessera,
        maker_address: tn::DEMO_MAKER_ADDRESS.to_string(),
    };
    let tracked = tn::TestnetOffer {
        offer: offer.clone(),
        faucet_txid: faucet_txid.clone(),
        covenant_address: covenant_address.clone(),
    };

    let mut st = state.lock().unwrap();
    let index = st.testnet_offers.len();
    st.testnet_offers.push(tracked);

    Ok(json!({
        "ok": true,
        "index": index,
        "outpoint": outpoint,
        "covenant_address": covenant_address,
        "faucet_txid": faucet_txid,
        "maker_address": tn::DEMO_MAKER_ADDRESS,
        "status": "pending_faucet",
        "note": "Faucet tx broadcast — ~1 min to confirm. Refresh the page to watch.",
    }))
}

// ── /api/take-offer ──────────────────────────────────────────────────────────

fn api_take_offer(req: &mut Request, state: &Mutex<AppState>) -> Result<Value> {
    match network() {
        Network::Regtest => api_take_offer_regtest(req, state),
        Network::Testnet => api_take_offer_testnet(req, state),
    }
}

fn api_take_offer_regtest(req: &mut Request, state: &Mutex<AppState>) -> Result<Value> {
    let b = body(req);
    let index = b.get("index").and_then(Value::as_u64).ok_or_else(|| anyhow!("index"))? as usize;
    let cheat  = Cheat::parse(b.get("cheat").and_then(Value::as_str).unwrap_or("none"));
    let offer = { state.lock().unwrap().offers.get(index).cloned()
        .ok_or_else(|| anyhow!("no offer #{index}"))? };

    let result = MosaikTaker::new(taker_rpc()).settle(&offer, cheat);
    match (cheat, result) {
        (Cheat::None, Ok(txid)) => {
            treasury().generate(1)?;
            let mut st = state.lock().unwrap();
            if index < st.offers.len() { st.offers.remove(index); }
            Ok(json!({ "ok": true, "txid": txid }))
        }
        (Cheat::None, Err(e)) => Err(e),
        (_, Err(e)) => Ok(json!({ "ok": true, "rejected": true, "reason": e.to_string() })),
        (_, Ok(txid)) => {
            treasury().generate(1)?;
            Ok(json!({ "ok": false, "covenant_bug": true, "txid": txid,
                        "reason": "a fraudulent settlement was accepted on-chain" }))
        }
    }
}

fn api_take_offer_testnet(req: &mut Request, state: &Mutex<AppState>) -> Result<Value> {
    let b = body(req);
    let index = b.get("index").and_then(Value::as_u64).ok_or_else(|| anyhow!("index"))? as usize;
    let cheat = Cheat::parse(b.get("cheat").and_then(Value::as_str).unwrap_or("none"));

    let tracked = { state.lock().unwrap().testnet_offers.get(index).cloned()
        .ok_or_else(|| anyhow!("no offer #{index}"))? };

    let esplora = tn::Esplora::testnet();
    let s = tn::probe_offer(&tracked.offer, &tracked.faucet_txid, &esplora);
    if s.status != tn::OfferStatus::Ready {
        anyhow::bail!("offer not ready yet — faucet tx still pending");
    }
    let value_sats = s.value_sats.ok_or_else(|| anyhow!("covenant UTXO value unknown"))?;

    let res = if cheat == Cheat::None {
        tn::settle_via_hal(&tracked.offer, value_sats, &esplora)
    } else {
        tn::settle_cheat_via_hal(&tracked.offer, value_sats, cheat, &esplora)
    };

    match (cheat, res) {
        (Cheat::None, Ok(txid)) => {
            let mut st = state.lock().unwrap();
            if index < st.testnet_offers.len() { st.testnet_offers.remove(index); }
            Ok(json!({ "ok": true, "txid": txid,
                        "explorer": format!("https://blockstream.info/liquidtestnet/tx/{txid}") }))
        }
        (Cheat::None, Err(e)) => Err(e),
        (_, Err(e)) => Ok(json!({ "ok": true, "rejected": true, "reason": e.to_string() })),
        (_, Ok(txid)) => Ok(json!({
            "ok": false, "covenant_bug": true, "txid": txid,
            "reason": "a fraudulent settlement was accepted on-chain",
        })),
    }
}

// ── /api/reclaim ─────────────────────────────────────────────────────────────

fn api_reclaim(req: &mut Request, state: &Mutex<AppState>) -> Result<Value> {
    let b = body(req);
    let index = b.get("index").and_then(Value::as_u64).ok_or_else(|| anyhow!("index"))? as usize;

    match network() {
        Network::Regtest => {
            let offer = { state.lock().unwrap().offers.get(index).cloned()
                .ok_or_else(|| anyhow!("no offer #{index}"))? };
            let txid = MosaikMaker::new(maker_rpc()).reclaim(&offer, &DEMO_MAKER_SECRET)?;
            treasury().generate(1)?;
            let mut st = state.lock().unwrap();
            if index < st.offers.len() { st.offers.remove(index); }
            Ok(json!({ "ok": true, "txid": txid }))
        }
        Network::Testnet => {
            let tracked = { state.lock().unwrap().testnet_offers.get(index).cloned()
                .ok_or_else(|| anyhow!("no offer #{index}"))? };
            let esplora = tn::Esplora::testnet();
            let s = tn::probe_offer(&tracked.offer, &tracked.faucet_txid, &esplora);
            let value_sats = s.value_sats.ok_or_else(|| anyhow!("covenant UTXO value unknown"))?;
            let txid = tn::reclaim_via_esplora(&tracked.offer, value_sats, &DEMO_MAKER_SECRET, &esplora)?;
            let mut st = state.lock().unwrap();
            if index < st.testnet_offers.len() { st.testnet_offers.remove(index); }
            Ok(json!({ "ok": true, "txid": txid,
                        "explorer": format!("https://blockstream.info/liquidtestnet/tx/{txid}") }))
        }
    }
}

// ── /api/contract ────────────────────────────────────────────────────────────

fn api_contract(req: &mut Request, state: &Mutex<AppState>) -> Result<Value> {
    let b = body(req);
    let index = b.get("index").and_then(Value::as_u64).ok_or_else(|| anyhow!("index"))? as usize;
    let offer = match network() {
        Network::Regtest => state.lock().unwrap().offers.get(index).cloned()
            .ok_or_else(|| anyhow!("no offer #{index}"))?,
        Network::Testnet => state.lock().unwrap().testnet_offers.get(index).cloned()
            .ok_or_else(|| anyhow!("no offer #{index}"))?.offer,
    };
    let compiled = offer.tessera.compile()?;
    Ok(json!({
        "source":  offer.tessera.render(),
        "cmr":     compiled.cmr_hex(),
        "address": compiled.address()?.to_string(),
        "settle":  "Taker path: spends the UTXO if output 0 pays the maker exactly \
                    `amount_b` of `asset_b`.",
        "refund":  "Maker path: once the chain reaches the refund block height the \
                    maker reclaims the locked asset with a BIP-340 signature.",
    }))
}

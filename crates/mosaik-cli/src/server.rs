//! Mosaik wallet UI — a tiny HTTP server that drives the regtest covenant DEX.
//!
//! The single-page app in `webapp/index.html` behaves like a normal exchange:
//! you pick **one trading pair** and every panel — order book, depth, the trade
//! tape, the open orders — re-renders for that pair.
//!
//! The UI has an **active-wallet switcher**: you act as any of four wallets
//! (`mosaik-user` plus three market makers). With whichever wallet is active
//! you can post a new covenant offer (act as a maker) *or* fill an existing
//! one (act as a taker), so cross-user buys and sells are testable end to
//! end. The seeded book shows liquidity from several distinct makers —
//! exactly what a permissionless covenant orderbook looks like.
//!
//! Every offer is also published as a Nostr addressable event (kind 30050); the
//! UI surfaces the real event id, pubkey, and tags so discovery-over-Nostr is
//! visible alongside the on-chain covenant.
//!
//! The covenant never inspects the *locked* asset (it only enforces the maker's
//! counter-payment), so any combination works: L-BTC/asset, asset/L-BTC, and
//! asset/asset, in either direction.
//!
//! Wallets on the Elements node:
//!   * `mosaik`         — the regtest treasury (holds the swept freecoins)
//!   * `mosaik-user`    — the UI wallet (makes *and* takes)
//!   * `mosaik-mm-{a,b,c}` — independent market makers that seed the book

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use mosaik_core::rpc::ElementsRpc;
use mosaik_core::{
    lbtc_tessera, tessera_for, Cheat, MakeOffer, MosaikMaker, MosaikTaker, Offer, ReclaimOffer,
};
use mosaik_relay::{Keys, OfferStatus, TesseraOffer};
use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server};

/// The single-page wallet UI, baked into the binary.
const INDEX_HTML: &str = include_str!("../../../webapp/index.html");

const BASE: &str = "http://127.0.0.1:7040";
const RPC_USER: &str = "user";
const RPC_PASS: &str = "pass";

/// The two test Liquid assets the demo issues, so asset/asset swaps are real.
const TEST_ASSETS: [&str; 2] = ["USDT", "EURx"];

/// Nostr relay the demo points at (informational — the event is built locally).
const RELAY_URL: &str = "ws://127.0.0.1:7777";

/// Wallets known to the demo: `(label, wallet name, display name)`.
///
/// Any of them can be the active trader in the UI; the three `mm-*` wallets
/// also seed the order book so it shows liquidity from several distinct parties.
const MAKERS: [(&str, &str, &str); 4] = [
    ("user", "mosaik-user", "Mosaik User"),
    ("mm-a", "mosaik-mm-a", "Helix MM"),
    ("mm-b", "mosaik-mm-b", "Aurora Desk"),
    ("mm-c", "mosaik-mm-c", "Tessera LP"),
];

/// An offer registered this session, plus its maker and Nostr metadata.
#[derive(Clone)]
struct ListedOffer {
    offer: Offer,
    /// Maker label (`user` | `mm-a` | …).
    maker: &'static str,
    /// Human-readable maker name.
    maker_name: &'static str,
    /// Nostr addressable order id (the `d` tag).
    order_id: String,
    /// Real Nostr event id for the published offer (kind 30050).
    event_id: String,
    /// Maker's Nostr pubkey.
    pubkey: String,
    /// Event timestamp (unix seconds).
    created_at: u64,
}

/// In-memory server state: the offers made this session and the test assets
/// (name -> RPC display-order id).
struct AppState {
    offers: Vec<ListedOffer>,
    assets: BTreeMap<String, String>,
    /// Settled swaps this session — the real trade tape.
    trades: Vec<Value>,
}

fn treasury() -> ElementsRpc {
    ElementsRpc::wallet(BASE, "mosaik", RPC_USER, RPC_PASS)
}
fn wallet_rpc(name: &str) -> ElementsRpc {
    ElementsRpc::wallet(BASE, name, RPC_USER, RPC_PASS)
}
fn node_rpc() -> ElementsRpc {
    ElementsRpc::node(BASE, RPC_USER, RPC_PASS)
}

/// Resolve a maker label to its `(label, wallet, display name)` entry.
fn maker_entry(label: &str) -> Result<(&'static str, &'static str, &'static str)> {
    MAKERS
        .iter()
        .copied()
        .find(|(l, _, _)| *l == label)
        .ok_or_else(|| anyhow!("unknown maker '{label}'"))
}

/// Run the wallet UI server until the process is killed.
pub fn run(port: u16) -> Result<()> {
    let node = node_rpc();
    for (_, wallet, _) in MAKERS {
        node.ensure_wallet(wallet)?;
    }

    let server = Server::http(("127.0.0.1", port))
        .map_err(|e| anyhow!("could not bind 127.0.0.1:{port}: {e}"))?;
    let state = Mutex::new(AppState {
        offers: Vec::new(),
        assets: BTreeMap::new(),
        trades: Vec::new(),
    });

    println!("Mosaik wallet UI  ->  http://127.0.0.1:{port}");
    println!("Wallets: mosaik (treasury), mosaik-user, mosaik-mm-{{a,b,c}}");
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
        (Method::Post, "/api/fund") => api_fund(req, state),
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

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

// ---- endpoints -------------------------------------------------------------

/// A `{ address, lbtc_sats, assets:{name:raw} }` snapshot of one wallet.
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

/// The user wallet, chain height, the test assets, the open offers, the tape.
fn api_state(state: &Mutex<AppState>) -> Result<Value> {
    let lbtc = node_rpc().policy_asset()?;
    let (offers, assets, trades) = {
        let st = state.lock().unwrap();
        (st.offers.clone(), st.assets.clone(), st.trades.clone())
    };

    let offers: Vec<Value> = offers
        .iter()
        .enumerate()
        .map(|(i, lo)| {
            let o = &lo.offer;
            let compiled = o.tessera.compile().ok();
            // asset_b is stored internal-order; reverse for the display id.
            let mut ab = o.tessera.asset_b.to_vec();
            ab.reverse();
            let asset_b = hex::encode(ab);
            let lock_name = id_to_name(&assets, &o.asset_a, &lbtc);
            let want_name = id_to_name(&assets, &asset_b, &lbtc);
            json!({
                "index": i,
                "outpoint": o.outpoint,
                "amount_a": o.amount_a,
                "amount_b": o.tessera.amount_b,
                "lock_name": lock_name,
                "want_name": want_name,
                "timeout": o.tessera.timeout,
                "maker_address": o.maker_address,
                "maker": lo.maker,
                "maker_name": lo.maker_name,
                "covenant_address": compiled.as_ref().and_then(|c| c.address().ok())
                    .map(|a| a.to_string()),
                "cmr": compiled.as_ref().map(|c| c.cmr_hex()),
                "nostr": {
                    "kind": mosaik_relay::TESSERA_OFFER_KIND,
                    "event_id": lo.event_id,
                    "pubkey": lo.pubkey,
                    "order_id": lo.order_id,
                    "created_at": lo.created_at,
                    "relay": RELAY_URL,
                },
            })
        })
        .collect();

    // A snapshot of every wallet, so the UI can switch the active trader and
    // test cross-user buys and sells.
    let mut wallets = serde_json::Map::new();
    for (label, wname, _) in MAKERS {
        wallets.insert(label.to_string(), wallet_snapshot(&wallet_rpc(wname), &assets)?);
    }

    Ok(json!({
        "block_count": node_rpc().block_count()?,
        "assets": assets.keys().cloned().collect::<Vec<_>>(),
        "makers": MAKERS.iter().map(|(l, _, n)| json!({ "label": l, "name": n }))
            .collect::<Vec<_>>(),
        "wallets": wallets,
        "offers": offers,
        "trades": trades,
        "relay": RELAY_URL,
    }))
}

/// Treasury -> wallet: top up with 1 L-BTC and 100 of every test asset, so the
/// wallet can be a maker or a taker for any pair. Body: `{ "wallet": "user" }`.
fn api_fund(req: &mut Request, state: &Mutex<AppState>) -> Result<Value> {
    let b = body(req);
    let label = b.get("wallet").and_then(Value::as_str).unwrap_or("user");
    let (_, wallet, name) = maker_entry(label)?;

    let assets = ensure_assets(state)?;
    let treasury = treasury();
    let target = wallet_rpc(wallet);

    let addr = target.new_unconfidential_address()?;
    let lbtc_txid = treasury.send_to_address(&addr, 1.0)?;

    // Explicit (unconfidential) UTXOs — a covenant settlement cannot spend a
    // blinded input.
    for id in assets.values() {
        let addr = target.new_unconfidential_address()?;
        treasury.send_asset_to(&addr, 100.0, id)?;
    }
    treasury.generate(1)?;
    Ok(json!({
        "ok": true,
        "txid": lbtc_txid,
        "wallet": label,
        "funded": format!("{name} funded — 1 L-BTC + 100 of each asset"),
    }))
}

/// Maker: fund a covenant UTXO and register the offer.
///
/// Body: `{ amount_a, amount_b, lock, want, timeout, maker? }`. `maker`
/// defaults to `"user"` — the UI wallet — but the seed script passes `mm-a`,
/// `mm-b`, `mm-c` so the book carries liquidity from several distinct makers.
fn api_make_offer(req: &mut Request, state: &Mutex<AppState>) -> Result<Value> {
    let b = body(req);
    let amount_a = b.get("amount_a").and_then(Value::as_u64).ok_or_else(|| anyhow!("amount_a"))?;
    let amount_b = b.get("amount_b").and_then(Value::as_u64).ok_or_else(|| anyhow!("amount_b"))?;
    let timeout = b.get("timeout").and_then(Value::as_u64).unwrap_or(500) as u32;
    let lock = b.get("lock").and_then(Value::as_str).unwrap_or("BTC");
    let want = b.get("want").and_then(Value::as_str).unwrap_or("USDT");
    let maker_label = b.get("maker").and_then(Value::as_str).unwrap_or("user");
    if lock == want {
        anyhow::bail!("lock and want assets must differ");
    }
    let (maker, wallet, maker_name) = maker_entry(maker_label)?;

    let rpc = wallet_rpc(wallet);
    let lbtc = rpc.policy_asset()?;
    let assets = { state.lock().unwrap().assets.clone() };
    let maker_address = rpc.new_unconfidential_address()?;

    // The covenant enforces the maker's counter-payment in the `want` asset.
    let want_id = label_to_id(&assets, want, &lbtc)?;
    let tessera = if want_id == lbtc {
        lbtc_tessera(&rpc, &maker_address, amount_b, timeout)?
    } else {
        tessera_for(&rpc, &maker_address, &want_id, amount_b, timeout)?
    };

    // `lock` is what the maker locks in the covenant UTXO.
    let lock_label =
        if lock == "BTC" { "BTC".to_string() } else { label_to_id(&assets, lock, &lbtc)? };
    let offer = MosaikMaker::new(rpc).make_offer(&lock_label, amount_a, &tessera, &maker_address)?;

    // Publish the offer as a real Nostr event (kind 30050) so discovery over
    // Nostr is visible in the UI. The event is built locally — no relay needed.
    let index = { state.lock().unwrap().offers.len() };
    let order_id = format!("mosaik-{maker}-{index}");
    let created_at = now_secs();
    let tessera_offer = TesseraOffer {
        order_id: order_id.clone(),
        offer: offer.clone(),
        expiry: created_at + 3600,
        status: OfferStatus::Active,
    };
    let keys = Keys::generate();
    let event = tessera_offer.to_event(&keys)?;
    let event_id = event.id.to_hex();
    let pubkey = keys.public_key().to_hex();

    let mut st = state.lock().unwrap();
    let index = st.offers.len();
    st.offers.push(ListedOffer {
        offer: offer.clone(),
        maker,
        maker_name,
        order_id,
        event_id: event_id.clone(),
        pubkey,
        created_at,
    });

    Ok(json!({
        "ok": true,
        "index": index,
        "outpoint": offer.outpoint,
        "maker_address": offer.maker_address,
        "maker": maker,
        "event_id": event_id,
    }))
}

/// Taker: fill a registered offer — the node executes the covenant.
///
/// The `taker` field picks which wallet fills the offer (`user` | `mm-a` |
/// `mm-b` | `mm-c`), so the UI can switch the active trader and test
/// cross-user buys and sells. Defaults to `user`.
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
    let taker_label = b.get("taker").and_then(Value::as_str).unwrap_or("user");
    let (_, taker_wallet, _) = maker_entry(taker_label)?;

    let listed = {
        let st = state.lock().unwrap();
        st.offers.get(index).cloned().ok_or_else(|| anyhow!("no offer #{index}"))?
    };
    let offer = listed.offer.clone();

    let result = MosaikTaker::new(wallet_rpc(taker_wallet)).settle(&offer, cheat);

    match (cheat, result) {
        // Honest fill that succeeded: confirm it, record the trade, drop the offer.
        (Cheat::None, Ok(txid)) => {
            treasury().generate(1)?;
            let lbtc = node_rpc().policy_asset()?;
            let block = node_rpc().block_count()?;
            let mut ab = offer.tessera.asset_b.to_vec();
            ab.reverse();
            let asset_b = hex::encode(ab);
            let amount_a = offer.amount_a;
            let amount_b = offer.tessera.amount_b;
            let rate = if amount_a > 0 { amount_b as f64 / amount_a as f64 } else { 0.0 };

            let mut st = state.lock().unwrap();
            let lock_name = id_to_name(&st.assets, &offer.asset_a, &lbtc);
            let want_name = id_to_name(&st.assets, &asset_b, &lbtc);
            // Newest first — the real trade tape.
            st.trades.insert(
                0,
                json!({
                    "pair": format!("{lock_name}/{want_name}"),
                    "lock_name": lock_name,
                    "want_name": want_name,
                    "amount_a": amount_a,
                    "amount_b": amount_b,
                    "rate": rate,
                    "txid": txid,
                    "block": block,
                    "maker_name": listed.maker_name,
                }),
            );
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

/// Reclaim an unfilled offer via the covenant's keyless REFUND path.
///
/// The path is keyless — *any* wallet may sweep it, and the covenant still
/// forces the locked asset back to the maker. The `wallet` field picks which
/// wallet broadcasts the sweep; it defaults to the offer's own maker.
fn api_reclaim(req: &mut Request, state: &Mutex<AppState>) -> Result<Value> {
    let b = body(req);
    let index = b.get("index").and_then(Value::as_u64).ok_or_else(|| anyhow!("index"))? as usize;

    let listed = {
        let st = state.lock().unwrap();
        st.offers.get(index).cloned().ok_or_else(|| anyhow!("no offer #{index}"))?
    };
    let sweeper = b.get("wallet").and_then(Value::as_str).unwrap_or(listed.maker);
    let (_, wallet, _) = maker_entry(sweeper)?;

    let txid = MosaikMaker::new(wallet_rpc(wallet)).reclaim(&listed.offer)?;
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

    let listed = {
        let st = state.lock().unwrap();
        st.offers.get(index).cloned().ok_or_else(|| anyhow!("no offer #{index}"))?
    };
    let offer = &listed.offer;
    let compiled = offer.tessera.compile()?;

    Ok(json!({
        "source": offer.tessera.render(),
        "cmr": compiled.cmr_hex(),
        "address": compiled.address()?.to_string(),
        "settle": "Taker path: spends the UTXO if output 0 pays the maker exactly \
            `amount_b` of `asset_b`. The node runs the covenant and rejects any \
            transaction that underpays. The locked asset is never inspected.",
        "refund": "Keyless path: once the chain reaches the refund block height \
            ANYONE may sweep the coin — but the covenant only accepts a spend \
            that returns the locked asset to the maker, so the worst a griefer \
            can do is pay the network fee to hand the maker their coin back. \
            No signature, no maker key.",
        "nostr": {
            "kind": mosaik_relay::TESSERA_OFFER_KIND,
            "event_id": listed.event_id,
            "pubkey": listed.pubkey,
            "order_id": listed.order_id,
            "created_at": listed.created_at,
            "relay": RELAY_URL,
        },
    }))
}

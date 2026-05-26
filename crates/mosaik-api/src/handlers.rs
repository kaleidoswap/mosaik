//! Mosaik HTTP API handlers — Liquid covenant DEX endpoints.
//!
//! These were ported verbatim from the original `mosaik-cli/src/server.rs`
//! (which was tightly coupled to `tiny_http`). Each handler is a pure
//! synchronous function: it takes a pre-parsed JSON body, mutates the shared
//! `AppState` under its `Mutex`, and returns `Result<Value>`. The axum layer
//! in `lib.rs` wraps each handler in `tokio::task::spawn_blocking` so the
//! blocking Elements RPC calls never starve the runtime.
//!
//! All RPC plumbing, wallet roster, and demo behaviour is unchanged. The
//! webapp baked into the CLI binary is unaffected — `lib.rs` still serves
//! `webapp/index.html` at `/`.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use mosaik_banco::{BancoClient, BancoMaker, BancoTaker, OfferTerms};
use mosaik_bridge::{
    LegRef, Preimage, RefundReason, SwapEnvelope, SwapStateMachine, SwapTimelocks,
};
use mosaik_core::rpc::ElementsRpc;
use mosaik_core::{
    lbtc_tessera, tessera_for, Cheat, MakeOffer, MosaikMaker, MosaikTaker, Offer, OfferKind,
    ReclaimOffer,
};
use mosaik_relay::{Keys, MakerPair, MakerProfile, OfferStatus, TesseraOffer};
use serde_json::{json, Value};

const BASE: &str = "http://127.0.0.1:7040";
const RPC_USER: &str = "user";
const RPC_PASS: &str = "pass";

/// The two test Liquid assets the demo issues, so asset/asset swaps are real.
const TEST_ASSETS: [&str; 2] = ["USDT", "EURx"];

/// Nostr relay the demo points at (informational — the event is built locally).
const RELAY_URL: &str = "ws://127.0.0.1:7777";

/// banco-proxy gateway URL — read at call time so the API can be repointed
/// without rebuilds. The default targets the docker-compose service name.
fn banco_proxy_url() -> String {
    std::env::var("BANCO_PROXY_URL").unwrap_or_else(|_| "http://127.0.0.1:3001".to_string())
}

/// Wallets known to the demo: `(label, wallet name, display name)`.
const MAKERS: [(&str, &str, &str); 4] = [
    ("user", "mosaik-user", "Mosaik User"),
    ("mm-a", "mosaik-mm-a", "Helix MM"),
    ("mm-b", "mosaik-mm-b", "Aurora Desk"),
    ("mm-c", "mosaik-mm-c", "Tessera LP"),
];

/// An offer registered this session, plus its maker and Nostr metadata.
#[derive(Clone)]
pub struct ListedOffer {
    pub offer: Offer,
    pub maker: &'static str,
    pub maker_name: &'static str,
    pub order_id: String,
    pub event_id: String,
    pub pubkey: String,
    pub created_at: u64,
}

/// A cross-chain swap in flight. Lives in memory; the [`Preimage`] is
/// never serialised out of the API — only its hash (already in the
/// envelope) crosses the network.
pub struct LocalSwap {
    pub envelope: SwapEnvelope,
    pub preimage: Preimage,
}

/// In-memory server state.
pub struct AppState {
    pub offers: Vec<ListedOffer>,
    pub assets: BTreeMap<String, String>,
    pub trades: Vec<Value>,
    /// Cross-chain swap envelopes the API has drafted, keyed by `swap_id`.
    /// Drives `mosaik_bridge`'s state machine in mock mode — see
    /// `docs/CROSS_CHAIN_SWAP.md` §"Primitive gaps" for the real-orchestrator
    /// runway.
    pub swaps: BTreeMap<String, LocalSwap>,
    /// Maker profiles (kind-30051), keyed by maker `label`. Seeded with one
    /// profile per known maker at boot; clients can replace via
    /// `POST /api/maker-profiles`.
    pub maker_profiles: BTreeMap<String, MakerProfile>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            offers: Vec::new(),
            assets: BTreeMap::new(),
            trades: Vec::new(),
            swaps: BTreeMap::new(),
            maker_profiles: default_maker_profiles(),
        }
    }
}

/// Seed one MakerProfile per [`MAKERS`] entry so the UI has something to
/// render before any maker explicitly publishes one. These match the
/// demo's "house" market makers' personalities — replaceable via the API.
fn default_maker_profiles() -> BTreeMap<String, MakerProfile> {
    fn pairs(backends: &[&str]) -> Vec<MakerPair> {
        let mut out = Vec::new();
        for b in backends {
            for (lock, want) in [("BTC", "USDT"), ("BTC", "EURx")] {
                out.push(MakerPair {
                    backend: (*b).to_string(),
                    lock: lock.to_string(),
                    want: want.to_string(),
                });
            }
        }
        out
    }
    let mut m = BTreeMap::new();
    m.insert(
        "user".to_string(),
        MakerProfile {
            d_tag: "mosaik-user".into(),
            name: "Mosaik User".into(),
            about: Some("UI demo wallet — make + take from the dashboard.".into()),
            contact: None,
            pairs: pairs(&["liquid"]),
            fill_latency_secs: None,
            min_cancel_delay: None,
            badges: vec![],
        },
    );
    m.insert(
        "mm-a".to_string(),
        MakerProfile {
            d_tag: "helix".into(),
            name: "Helix MM".into(),
            about: Some("Liquid + Arkade liquidity, 24/5.".into()),
            contact: Some("helix@example.com".into()),
            pairs: pairs(&["liquid", "banco"]),
            fill_latency_secs: Some(45),
            min_cancel_delay: Some(120),
            badges: vec!["audited".into()],
        },
    );
    m.insert(
        "mm-b".to_string(),
        MakerProfile {
            d_tag: "aurora".into(),
            name: "Aurora Desk".into(),
            about: Some("Asset/asset specialist — tight EURx spreads.".into()),
            contact: None,
            pairs: pairs(&["liquid"]),
            fill_latency_secs: Some(180),
            min_cancel_delay: Some(60),
            badges: vec!["kyc-optional".into()],
        },
    );
    m.insert(
        "mm-c".to_string(),
        MakerProfile {
            d_tag: "tessera-lp".into(),
            name: "Tessera LP".into(),
            about: Some("Covenant-only LP — proudly keyless settlement.".into()),
            contact: None,
            pairs: pairs(&["liquid", "banco"]),
            fill_latency_secs: Some(30),
            min_cancel_delay: Some(240),
            badges: vec!["audited".into(), "open-source".into()],
        },
    );
    m
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
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

/// Ensure every demo wallet exists on the node. Called once at boot.
pub fn init_wallets() -> Result<()> {
    let node = node_rpc();
    for (_, wallet, _) in MAKERS {
        node.ensure_wallet(wallet)?;
    }
    Ok(())
}

fn maker_entry(label: &str) -> Result<(&'static str, &'static str, &'static str)> {
    MAKERS
        .iter()
        .copied()
        .find(|(l, _, _)| *l == label)
        .ok_or_else(|| anyhow!("unknown maker '{label}'"))
}

fn raw_balance(balances: &Value, asset: &str) -> u64 {
    balances
        .get(asset)
        .and_then(Value::as_f64)
        .map(|v| (v * 1e8).round() as u64)
        .unwrap_or(0)
}

fn ensure_assets(state: &Mutex<AppState>) -> Result<BTreeMap<String, String>> {
    let mut st = state.lock().unwrap();
    if st.assets.is_empty() {
        let treasury = treasury();
        for name in TEST_ASSETS {
            let (id, _) = treasury.issue_asset(100_000.0, 0.0)?;
            st.assets.insert(name.to_string(), id);
        }
    }
    Ok(st.assets.clone())
}

fn label_to_id(assets: &BTreeMap<String, String>, label: &str, lbtc: &str) -> Result<String> {
    match label {
        "BTC" | "L-BTC" | "LBTC" => Ok(lbtc.to_string()),
        name => assets
            .get(name)
            .cloned()
            .ok_or_else(|| anyhow!("unknown asset '{name}' — fund a wallet to issue it")),
    }
}

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
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

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

pub fn api_state(state: &Mutex<AppState>) -> Result<Value> {
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
            let backend = o.backend();

            // Common envelope fields.
            let lock_name = id_to_name(&assets, &o.asset_a, &lbtc);
            let nostr = json!({
                "kind": mosaik_relay::TESSERA_OFFER_KIND,
                "event_id": lo.event_id,
                "pubkey": lo.pubkey,
                "order_id": lo.order_id,
                "created_at": lo.created_at,
                "relay": RELAY_URL,
            });

            // Backend-specific fields. Both backends end up in the same
            // table-row shape (amount_b / want_name / timeout) so the SPA's
            // OrderBook stays uniform.
            match &o.kind {
                OfferKind::Liquid(tessera) => {
                    let compiled = tessera.compile().ok();
                    let mut ab = tessera.asset_b.to_vec();
                    ab.reverse();
                    let asset_b = hex::encode(ab);
                    let want_name = id_to_name(&assets, &asset_b, &lbtc);
                    json!({
                        "backend": backend,
                        "index": i,
                        "outpoint": o.outpoint,
                        "amount_a": o.amount_a,
                        "amount_b": tessera.amount_b,
                        "lock_name": lock_name,
                        "want_name": want_name,
                        "timeout": tessera.timeout,
                        "maker_address": o.maker_address,
                        "maker": lo.maker,
                        "maker_name": lo.maker_name,
                        "covenant_address": compiled.as_ref().and_then(|c| c.address().ok())
                            .map(|a| a.to_string()),
                        "cmr": compiled.as_ref().map(|c| c.cmr_hex()),
                        "nostr": nostr,
                    })
                }
                OfferKind::Banco(banco) => {
                    let want_name = banco
                        .want_asset
                        .clone()
                        .unwrap_or_else(|| "BTC".to_string());
                    json!({
                        "backend": backend,
                        "index": i,
                        "outpoint": o.outpoint,
                        "amount_a": o.amount_a,
                        "amount_b": banco.want_amount,
                        "lock_name": lock_name,
                        "want_name": want_name,
                        // banco offers use a unix-seconds cancel_delay; the SPA
                        // renders `timeout` regardless of backend, so we surface
                        // it here under the same key.
                        "timeout": banco.cancel_delay,
                        "maker_address": o.maker_address,
                        "maker": lo.maker,
                        "maker_name": lo.maker_name,
                        "covenant_address": Value::Null,
                        "cmr": Value::Null,
                        "swap_pk_script": banco.swap_pk_script.clone(),
                        "partial_fill": banco.partial_fill,
                        "nostr": nostr,
                    })
                }
            }
        })
        .collect();

    let mut wallets = serde_json::Map::new();
    for (label, wname, _) in MAKERS {
        wallets.insert(
            label.to_string(),
            wallet_snapshot(&wallet_rpc(wname), &assets)?,
        );
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

pub fn api_fund(body: &Value, state: &Mutex<AppState>) -> Result<Value> {
    let label = body.get("wallet").and_then(Value::as_str).unwrap_or("user");
    let (_, wallet, name) = maker_entry(label)?;

    let assets = ensure_assets(state)?;
    let treasury = treasury();
    let target = wallet_rpc(wallet);

    let addr = target.new_unconfidential_address()?;
    let lbtc_txid = treasury.send_to_address(&addr, 1.0)?;

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

pub fn api_make_offer(body: &Value, state: &Mutex<AppState>) -> Result<Value> {
    let backend = body
        .get("backend")
        .and_then(Value::as_str)
        .unwrap_or("liquid");
    if backend == "banco" {
        return api_make_offer_banco(body, state);
    }

    let amount_a = body
        .get("amount_a")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("amount_a"))?;
    let amount_b = body
        .get("amount_b")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("amount_b"))?;
    let timeout = body.get("timeout").and_then(Value::as_u64).unwrap_or(500) as u32;
    let lock = body.get("lock").and_then(Value::as_str).unwrap_or("BTC");
    let want = body.get("want").and_then(Value::as_str).unwrap_or("USDT");
    let maker_label = body.get("maker").and_then(Value::as_str).unwrap_or("user");
    if lock == want {
        anyhow::bail!("lock and want assets must differ");
    }
    let (maker, wallet, maker_name) = maker_entry(maker_label)?;

    let rpc = wallet_rpc(wallet);
    let lbtc = rpc.policy_asset()?;
    let assets = { state.lock().unwrap().assets.clone() };
    let maker_address = rpc.new_unconfidential_address()?;

    let want_id = label_to_id(&assets, want, &lbtc)?;
    let tessera = if want_id == lbtc {
        lbtc_tessera(&rpc, &maker_address, amount_b, timeout)?
    } else {
        tessera_for(&rpc, &maker_address, &want_id, amount_b, timeout)?
    };

    let lock_label = if lock == "BTC" {
        "BTC".to_string()
    } else {
        label_to_id(&assets, lock, &lbtc)?
    };
    let offer =
        MosaikMaker::new(rpc).make_offer(&lock_label, amount_a, &tessera, &maker_address)?;

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

pub fn api_take_offer(body: &Value, state: &Mutex<AppState>) -> Result<Value> {
    let index = body
        .get("index")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("index"))? as usize;
    let cheat = Cheat::parse(body.get("cheat").and_then(Value::as_str).unwrap_or("none"));
    let taker_label = body.get("taker").and_then(Value::as_str).unwrap_or("user");
    let (_, taker_wallet, _) = maker_entry(taker_label)?;

    let listed = {
        let st = state.lock().unwrap();
        st.offers
            .get(index)
            .cloned()
            .ok_or_else(|| anyhow!("no offer #{index}"))?
    };
    let offer = listed.offer.clone();

    // Dispatch based on the offer's backend. Banco offers route through
    // banco-proxy; Liquid offers go through the local Simplicity covenant.
    if matches!(offer.kind, OfferKind::Banco(_)) {
        if cheat != Cheat::None {
            anyhow::bail!(
                "cheat modes are Liquid-only — the banco covenant has its own enforcement"
            );
        }
        return api_take_offer_banco(taker_label, index, body, state);
    }

    let result = MosaikTaker::new(wallet_rpc(taker_wallet)).settle(&offer, cheat);

    match (cheat, result) {
        (Cheat::None, Ok(txid)) => {
            treasury().generate(1)?;
            let lbtc = node_rpc().policy_asset()?;
            let block = node_rpc().block_count()?;
            let tessera = offer.tessera()?;
            let mut ab = tessera.asset_b.to_vec();
            ab.reverse();
            let asset_b = hex::encode(ab);
            let amount_a = offer.amount_a;
            let amount_b = tessera.amount_b;
            let rate = if amount_a > 0 {
                amount_b as f64 / amount_a as f64
            } else {
                0.0
            };

            let mut st = state.lock().unwrap();
            let lock_name = id_to_name(&st.assets, &offer.asset_a, &lbtc);
            let want_name = id_to_name(&st.assets, &asset_b, &lbtc);
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
        (_, Err(e)) => Ok(json!({
            "ok": true,
            "rejected": true,
            "reason": e.to_string(),
        })),
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

pub fn api_reclaim(body: &Value, state: &Mutex<AppState>) -> Result<Value> {
    let index = body
        .get("index")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("index"))? as usize;

    let listed = {
        let st = state.lock().unwrap();
        st.offers
            .get(index)
            .cloned()
            .ok_or_else(|| anyhow!("no offer #{index}"))?
    };

    // Banco offers route through banco-proxy's maker/cancel.
    if matches!(listed.offer.kind, OfferKind::Banco(_)) {
        return api_reclaim_banco(index, &listed, state);
    }

    let sweeper = body
        .get("wallet")
        .and_then(Value::as_str)
        .unwrap_or(listed.maker);
    let (_, wallet, _) = maker_entry(sweeper)?;

    let txid = MosaikMaker::new(wallet_rpc(wallet)).reclaim(&listed.offer)?;
    treasury().generate(1)?;

    let mut st = state.lock().unwrap();
    if index < st.offers.len() {
        st.offers.remove(index);
    }
    Ok(json!({ "ok": true, "txid": txid }))
}

pub fn api_contract(body: &Value, state: &Mutex<AppState>) -> Result<Value> {
    let index = body
        .get("index")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("index"))? as usize;

    let listed = {
        let st = state.lock().unwrap();
        st.offers
            .get(index)
            .cloned()
            .ok_or_else(|| anyhow!("no offer #{index}"))?
    };
    let offer = &listed.offer;
    let tessera = offer.tessera()?;
    let compiled = tessera.compile()?;

    Ok(json!({
        "source": tessera.render(),
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

// ─── Cross-chain swap (mosaik-bridge) ────────────────────────────────────────
//
// These wrap `mosaik_bridge`'s typed state machine in the same in-memory
// AppState pattern as offers. The real orchestrator (against live Liquid +
// banco-proxy) lands when the two primitive gaps in
// `docs/CROSS_CHAIN_SWAP.md` close; until then these endpoints walk the
// MockOrchestrator's transitions deterministically, so the SPA can render
// the full lifecycle today.

/// Serialise one swap envelope as the API surface — never the preimage.
fn envelope_json(env: &SwapEnvelope) -> Value {
    serde_json::to_value(env).unwrap_or(Value::Null)
}

/// `GET /api/swaps` — list every in-flight swap.
pub fn api_swap_list(state: &Mutex<AppState>) -> Result<Value> {
    let st = state.lock().unwrap();
    let envelopes: Vec<Value> = st
        .swaps
        .values()
        .map(|s| envelope_json(&s.envelope))
        .collect();
    Ok(json!({ "swaps": envelopes }))
}

/// `GET /api/swaps/:id` — single envelope by id.
pub fn api_swap_get(id: &str, state: &Mutex<AppState>) -> Result<Value> {
    let st = state.lock().unwrap();
    let swap = st
        .swaps
        .get(id)
        .ok_or_else(|| anyhow!("no swap with id '{id}'"))?;
    Ok(envelope_json(&swap.envelope))
}

/// `POST /api/swaps/draft` — generate a preimage, build a fresh envelope.
///
/// Body (all optional):
/// `{ swap_id?, a_refund?, b_refund?, safety_margin? }`.
/// `a_refund` / `b_refund` are unix seconds; defaults are now + 2h / +1h
/// with a 30-min safety margin (the bridge's [`SwapTimelocks::default_from`]).
pub fn api_swap_draft(body: &Value, state: &Mutex<AppState>) -> Result<Value> {
    let swap_id = body
        .get("swap_id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("mosaik-swap-{}", now_secs()));

    let now = now_secs();
    let defaults = SwapTimelocks::default_from(now);
    let timelocks = SwapTimelocks {
        a_refund: body
            .get("a_refund")
            .and_then(Value::as_u64)
            .unwrap_or(defaults.a_refund),
        b_refund: body
            .get("b_refund")
            .and_then(Value::as_u64)
            .unwrap_or(defaults.b_refund),
        safety_margin: body
            .get("safety_margin")
            .and_then(Value::as_u64)
            .unwrap_or(defaults.safety_margin),
    };
    timelocks
        .validate()
        .map_err(|e| anyhow!("invalid timelocks: {e}"))?;

    let preimage = Preimage::random();
    let envelope = SwapEnvelope {
        swap_id: swap_id.clone(),
        preimage_hash: preimage.hash(),
        leg_a: LegRef {
            backend: "liquid".into(),
            outpoint: String::new(),
            event_id: None,
        },
        leg_b: LegRef {
            backend: "banco".into(),
            outpoint: String::new(),
            event_id: None,
        },
        timelocks,
        state: SwapStateMachine::Drafted,
        leg_a_offer: None,
        leg_b_offer: None,
    };

    {
        let mut st = state.lock().unwrap();
        if st.swaps.contains_key(&swap_id) {
            anyhow::bail!("swap_id '{swap_id}' already exists");
        }
        st.swaps.insert(
            swap_id.clone(),
            LocalSwap {
                envelope: envelope.clone(),
                preimage,
            },
        );
    }

    Ok(envelope_json(&envelope))
}

/// `POST /api/swaps/:id/advance` — step the state machine forward.
///
/// Body: `{ action: "publish_leg_a" | "publish_leg_b" | "observe_preimage" | "complete" }`.
/// Each action only fires from the expected state; otherwise returns 400.
pub fn api_swap_advance(id: &str, body: &Value, state: &Mutex<AppState>) -> Result<Value> {
    let action = body
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing 'action'"))?;

    let mut st = state.lock().unwrap();
    let swap = st
        .swaps
        .get_mut(id)
        .ok_or_else(|| anyhow!("no swap with id '{id}'"))?;

    match (action, &swap.envelope.state) {
        ("publish_leg_a", SwapStateMachine::Drafted) => {
            swap.envelope.leg_a.outpoint = format!("mock-leg-a-{id}");
            swap.envelope.state = SwapStateMachine::HalfPublished;
        }
        ("publish_leg_b", SwapStateMachine::HalfPublished) => {
            swap.envelope.leg_b.outpoint = format!("mock-leg-b-{id}");
            swap.envelope.state = SwapStateMachine::BothPublished;
        }
        ("observe_preimage", SwapStateMachine::BothPublished) => {
            swap.envelope.state = SwapStateMachine::Revealed;
        }
        ("complete", SwapStateMachine::Revealed) => {
            swap.envelope.state = SwapStateMachine::Settled;
        }
        (a, s) => {
            anyhow::bail!(
                "invalid action '{a}' for state '{}'; legal transitions are \
                 drafted→publish_leg_a→half_published→publish_leg_b→both_published \
                 →observe_preimage→revealed→complete→settled",
                s.label()
            )
        }
    }

    Ok(envelope_json(&swap.envelope))
}

/// `POST /api/swaps/:id/refund` — terminate via the refund branch.
///
/// Body: `{ reason: "never_published" | "partner_never_published" | "no_taker_arrived" | "maker_missed_claim_window" }`.
pub fn api_swap_refund(id: &str, body: &Value, state: &Mutex<AppState>) -> Result<Value> {
    let reason_str = body
        .get("reason")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing 'reason'"))?;
    let reason: RefundReason = serde_json::from_value(Value::String(reason_str.to_string()))
        .map_err(|_| anyhow!("unknown refund reason '{reason_str}'"))?;

    let mut st = state.lock().unwrap();
    let swap = st
        .swaps
        .get_mut(id)
        .ok_or_else(|| anyhow!("no swap with id '{id}'"))?;
    if matches!(swap.envelope.state, SwapStateMachine::Settled) {
        anyhow::bail!("cannot refund a settled swap");
    }
    swap.envelope.state = SwapStateMachine::Refunded { reason };
    Ok(envelope_json(&swap.envelope))
}

// ─── Maker profile (kind-30051) ──────────────────────────────────────────────

/// `GET /api/maker-profiles` — list every known profile.
///
/// Today returns the seeded "house" profiles plus any client-published ones.
/// Listing is in deterministic key-order (the maker label).
pub fn api_maker_profile_list(state: &Mutex<AppState>) -> Result<Value> {
    let st = state.lock().unwrap();
    let profiles: Vec<Value> = st
        .maker_profiles
        .iter()
        .map(|(label, p)| {
            // Mirror the canonical relay shape and add the maker label the
            // UI keys on. Profiles published independently won't have a
            // "label" — clients should treat it as advisory.
            let mut v = serde_json::to_value(p).unwrap_or(Value::Null);
            if let Some(obj) = v.as_object_mut() {
                obj.insert("label".into(), Value::String(label.clone()));
            }
            v
        })
        .collect();
    Ok(json!({ "profiles": profiles }))
}

/// `POST /api/maker-profiles` — replace the profile for a given maker label.
///
/// Body: `{ label, profile: MakerProfile }`. The Nostr publishing path lands
/// when the API grows a managed signing identity; today the profile is just
/// stored in memory and reflected back to clients via the GET.
pub fn api_maker_profile_publish(body: &Value, state: &Mutex<AppState>) -> Result<Value> {
    let label = body
        .get("label")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing 'label'"))?
        .to_string();
    let profile_v = body
        .get("profile")
        .ok_or_else(|| anyhow!("missing 'profile'"))?;
    let profile: MakerProfile =
        serde_json::from_value(profile_v.clone()).map_err(|e| anyhow!("invalid profile: {e}"))?;

    let mut st = state.lock().unwrap();
    st.maker_profiles.insert(label.clone(), profile.clone());
    Ok(json!({ "ok": true, "label": label, "profile": profile }))
}

// ─── Backend dispatch: banco ─────────────────────────────────────────────────
//
// Each function here mirrors its Liquid sibling above: it reads the same
// request shape, resolves the maker label to a banco-proxy wallet id (1:1
// today — the maker label IS the wallet id), runs the call through
// mosaik-banco, and stores the resulting Offer in the same in-memory
// ListedOffer pattern so the rest of the API (api_state, api_take_offer,
// api_reclaim) handles both backends symmetrically.

/// Map a maker label to the banco-proxy `walletId` to act as. Today the
/// label IS the wallet id; production deployments can plug in a richer
/// table here (KMS lookup, etc.) without touching the handler bodies.
fn banco_wallet_id(maker_label: &str) -> &str {
    maker_label
}

fn api_make_offer_banco(body: &Value, state: &Mutex<AppState>) -> Result<Value> {
    let amount_a = body
        .get("amount_a")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("amount_a"))?;
    let amount_b = body
        .get("amount_b")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("amount_b"))?;
    // Banco's cancel_delay is unix seconds; default to two hours out.
    let cancel_delay = body
        .get("cancel_delay")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| now_secs() + 7200);
    let lock = body.get("lock").and_then(Value::as_str).unwrap_or("BTC");
    let want = body.get("want").and_then(Value::as_str).unwrap_or("BTC");
    let partial_fill = body
        .get("partial_fill")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let maker_label = body.get("maker").and_then(Value::as_str).unwrap_or("user");
    let (maker, _wallet, maker_name) = maker_entry(maker_label)?;

    // `BTC` is the implicit Arkade native asset for the banco SDK (None).
    let sell_asset = if lock == "BTC" {
        None
    } else {
        Some(lock.to_string())
    };
    let want_asset = if want == "BTC" {
        None
    } else {
        Some(want.to_string())
    };

    let client = BancoClient::new(banco_proxy_url(), banco_wallet_id(maker_label));
    let offer = BancoMaker::new(client).make_offer(&OfferTerms {
        sell_amount: amount_a,
        sell_asset,
        want_amount: amount_b,
        want_asset,
        cancel_delay,
        partial_fill,
    })?;

    // Wrap the Offer in our listing record and publish a kind-30050 event
    // the same way the Liquid path does (the event content travels the same
    // OfferKind shape; the relay tag carries the backend label).
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
        "backend": "banco",
        "index": index,
        "outpoint": offer.outpoint,
        "maker": maker,
        "event_id": event_id,
    }))
}

fn api_take_offer_banco(
    taker_label: &str,
    index: usize,
    body: &Value,
    state: &Mutex<AppState>,
) -> Result<Value> {
    let listed = {
        let st = state.lock().unwrap();
        st.offers
            .get(index)
            .cloned()
            .ok_or_else(|| anyhow!("no offer #{index}"))?
    };
    let fill_amount = body.get("fill_amount").and_then(Value::as_u64);

    let client = BancoClient::new(banco_proxy_url(), banco_wallet_id(taker_label));
    let txid = BancoTaker::new(client).fulfill(&listed.offer, fill_amount)?;

    // Record the trade and drop the offer, mirroring the Liquid path so the
    // SPA's trade tape advances the same way.
    let banco = listed.offer.banco()?;
    let amount_a = listed.offer.amount_a;
    let amount_b = banco.want_amount;
    let rate = if amount_a > 0 {
        amount_b as f64 / amount_a as f64
    } else {
        0.0
    };
    let lock_name = listed.offer.asset_a.clone();
    let want_name = banco
        .want_asset
        .clone()
        .unwrap_or_else(|| "BTC".to_string());

    let mut st = state.lock().unwrap();
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
            // banco fulfilment is a single arkd transaction; we don't have an
            // Elements block height to surface, so block stays 0.
            "block": 0,
            "maker_name": listed.maker_name,
            "backend": "banco",
        }),
    );
    if index < st.offers.len() {
        st.offers.remove(index);
    }
    Ok(json!({ "ok": true, "backend": "banco", "txid": txid }))
}

fn api_reclaim_banco(index: usize, listed: &ListedOffer, state: &Mutex<AppState>) -> Result<Value> {
    let client = BancoClient::new(banco_proxy_url(), banco_wallet_id(listed.maker));
    let txid = BancoMaker::new(client).cancel(&listed.offer)?;

    let mut st = state.lock().unwrap();
    if index < st.offers.len() {
        st.offers.remove(index);
    }
    Ok(json!({ "ok": true, "backend": "banco", "txid": txid }))
}

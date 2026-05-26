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
use mosaik_bridge::{
    LegRef, Preimage, RefundReason, SwapEnvelope, SwapStateMachine, SwapTimelocks,
};
use mosaik_core::rpc::ElementsRpc;
use mosaik_core::{
    lbtc_tessera, tessera_for, Cheat, MakeOffer, MosaikMaker, MosaikTaker, Offer, ReclaimOffer,
};
use mosaik_relay::{Keys, OfferStatus, TesseraOffer};
use serde_json::{json, Value};

const BASE: &str = "http://127.0.0.1:7040";
const RPC_USER: &str = "user";
const RPC_PASS: &str = "pass";

/// The two test Liquid assets the demo issues, so asset/asset swaps are real.
const TEST_ASSETS: [&str; 2] = ["USDT", "EURx"];

/// Nostr relay the demo points at (informational — the event is built locally).
const RELAY_URL: &str = "ws://127.0.0.1:7777";

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
}

impl AppState {
    pub fn new() -> Self {
        Self {
            offers: Vec::new(),
            assets: BTreeMap::new(),
            trades: Vec::new(),
            swaps: BTreeMap::new(),
        }
    }
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
            // The webapp is Liquid-only; non-Liquid offers shouldn't reach here.
            let tessera = o
                .tessera()
                .expect("mosaik-api state currently only renders Liquid offers");
            let compiled = tessera.compile().ok();
            let mut ab = tessera.asset_b.to_vec();
            ab.reverse();
            let asset_b = hex::encode(ab);
            let lock_name = id_to_name(&assets, &o.asset_a, &lbtc);
            let want_name = id_to_name(&assets, &asset_b, &lbtc);
            json!({
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

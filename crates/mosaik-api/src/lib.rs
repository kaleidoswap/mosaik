//! Mosaik HTTP API — a long-running axum daemon exposing the covenant DEX.
//!
//! This crate is the home of the API the webapp talks to. The handlers
//! themselves live in [`handlers`] as plain synchronous functions over a
//! shared [`AppState`]; this module bolts an axum router on top, dispatches
//! every blocking call through `tokio::task::spawn_blocking`, and serves the
//! single-page webapp at `/`.
//!
//! The HTTP shape is stable across the future banco / cross-chain backends:
//! a backend selector will eventually live in each request body, but the
//! routes themselves do not need to change.
//!
//! ## Entry points
//! - [`run`] — used from the CLI's `serve-wallet` command.
//! - The `mosaik-api` binary in `src/bin/mosaik-api.rs` — standalone daemon.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use mosaik_relay::{Keys, MosaikRelay};
use serde_json::{json, Value};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

pub mod handlers;
pub use handlers::AppState;

/// Where to publish the API's outbound events. Override via env so a
/// downstream deployment can repoint without rebuilds.
fn relay_url() -> String {
    std::env::var("MOSAIK_RELAY_URL").unwrap_or_else(|_| "ws://127.0.0.1:7777".to_string())
}

type SharedState = Arc<Mutex<AppState>>;

/// Build the axum router. Exposed so tests can mount it without binding a port.
///
/// `/` returns a small JSON service descriptor — the SPA frontend lives in its
/// own image (`mosaik-webapp`, served by nginx) and reverse-proxies `/api/*`
/// back through to this daemon. There is no embedded webapp here any more.
pub fn router(state: SharedState) -> Router {
    Router::new()
        .route("/", get(service_descriptor))
        .route("/healthz", get(healthz))
        .route("/api/state", get(route_state))
        .route("/api/fund", post(route_fund))
        .route("/api/make-offer", post(route_make_offer))
        .route("/api/take-offer", post(route_take_offer))
        .route("/api/reclaim", post(route_reclaim))
        .route("/api/contract", post(route_contract))
        .route("/api/swaps", get(route_swaps_list))
        .route("/api/swaps/draft", post(route_swaps_draft))
        .route("/api/swaps/:id", get(route_swaps_get))
        .route("/api/swaps/:id/advance", post(route_swaps_advance))
        .route("/api/swaps/:id/refund", post(route_swaps_refund))
        .route("/api/maker-profiles", get(route_maker_profile_list))
        .route("/api/maker-profiles", post(route_maker_profile_publish))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Bring the API up on `127.0.0.1:port` and serve until the process exits.
pub async fn run(port: u16) -> Result<()> {
    // Best-effort wallet bootstrap. We log and continue on failure so the API
    // still boots when Elements isn't up yet — `/healthz` works either way,
    // and any handler that needs RPC will surface its own error.
    if let Err(e) = handlers::init_wallets() {
        tracing::warn!("init_wallets failed (Elements not reachable?): {e}");
        eprintln!("warning: init_wallets failed: {e}");
    }

    // Stable signing identity for this process. Production deployments
    // should seed this from a persisted nsec; for the demo we generate
    // fresh on every boot (and the relay deduplicates by (pubkey, kind, d)
    // so old events stick around under the previous identity).
    let keys = Keys::generate();
    eprintln!("mosaik-api signer pubkey: {}", keys.public_key().to_hex());

    // Best-effort relay connection. None if MOSAIK_RELAY_URL is unreachable;
    // handlers gracefully no-op on relay publishes when the client is missing.
    let relay_url = relay_url();
    let relay = match MosaikRelay::connect(keys.clone(), &[relay_url.as_str()]).await {
        Ok(c) => {
            tracing::info!(url = %relay_url, "connected to Mosaik relay");
            eprintln!("connected to Mosaik relay at {relay_url}");
            Some(Arc::new(c))
        }
        Err(e) => {
            tracing::warn!(url = %relay_url, err = %e, "relay connect failed; publishes will no-op");
            eprintln!("warning: relay connect to {relay_url} failed: {e}");
            None
        }
    };

    let state: SharedState = Arc::new(Mutex::new(AppState::with_relay(
        relay,
        Some(tokio::runtime::Handle::current()),
        keys,
    )));

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("mosaik-api listening on http://{addr}");
    eprintln!("mosaik-api listening on http://{addr}");

    axum::serve(listener, router(state)).await?;
    Ok(())
}

// ─── route adapters ──────────────────────────────────────────────────────────

/// Tiny service descriptor at `/`. Useful for ops tools, container probes,
/// and anyone curling the daemon to confirm what they're talking to.
async fn service_descriptor() -> Json<Value> {
    Json(json!({
        "service": "mosaik-api",
        "version": env!("CARGO_PKG_VERSION"),
        "docs":    "https://github.com/kaleidoswap/mosaik",
        "endpoints": {
            "health":  "/healthz",
            "api":     "/api/{state,fund,make-offer,take-offer,reclaim,contract}",
            "webapp":  "served by the mosaik-webapp image (nginx on :5173)"
        }
    }))
}

async fn healthz() -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        r#"{"status":"ok"}"#,
    )
        .into_response()
}

async fn route_state(State(state): State<SharedState>) -> Response {
    blocking(move || handlers::api_state(&state)).await
}

async fn route_fund(State(state): State<SharedState>, body: Bytes) -> Response {
    let b = parse_body(body);
    blocking(move || handlers::api_fund(&b, &state)).await
}

async fn route_make_offer(State(state): State<SharedState>, body: Bytes) -> Response {
    let b = parse_body(body);
    blocking(move || handlers::api_make_offer(&b, &state)).await
}

async fn route_take_offer(State(state): State<SharedState>, body: Bytes) -> Response {
    let b = parse_body(body);
    blocking(move || handlers::api_take_offer(&b, &state)).await
}

async fn route_reclaim(State(state): State<SharedState>, body: Bytes) -> Response {
    let b = parse_body(body);
    blocking(move || handlers::api_reclaim(&b, &state)).await
}

async fn route_contract(State(state): State<SharedState>, body: Bytes) -> Response {
    let b = parse_body(body);
    blocking(move || handlers::api_contract(&b, &state)).await
}

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Empty / invalid bodies collapse to `Value::Null`. Handlers tolerate this —
/// they reach for fields with `.get(...)` and default missing ones.
fn parse_body(bytes: Bytes) -> Value {
    if bytes.is_empty() {
        return Value::Null;
    }
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

/// Run a blocking handler on the rayon-style blocking pool, map its
/// `Result<Value>` into a JSON response, and turn errors into a clean
/// `400` with `{"error": ...}`.
async fn blocking<F>(f: F) -> Response
where
    F: FnOnce() -> Result<Value> + Send + 'static,
{
    match tokio::task::spawn_blocking(f).await {
        Ok(Ok(v)) => (StatusCode::OK, Json(v)).into_response(),
        Ok(Err(e)) => (
            StatusCode::BAD_REQUEST,
            // `{:#}` walks the anyhow context chain so callers see the actual
            // root cause (e.g. the banco-proxy body), not just the outermost
            // `with_context("POST /banco/maker/offer")` wrapper.
            Json(json!({ "error": format!("{e:#}") })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("blocking task panicked: {e}") })),
        )
            .into_response(),
    }
}

// ─── Swap routes ─────────────────────────────────────────────────────────────

async fn route_swaps_list(State(state): State<SharedState>) -> Response {
    blocking(move || handlers::api_swap_list(&state)).await
}

async fn route_swaps_draft(State(state): State<SharedState>, body: Bytes) -> Response {
    let b = parse_body(body);
    blocking(move || handlers::api_swap_draft(&b, &state)).await
}

async fn route_swaps_get(State(state): State<SharedState>, Path(id): Path<String>) -> Response {
    blocking(move || handlers::api_swap_get(&id, &state)).await
}

async fn route_swaps_advance(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    let b = parse_body(body);
    blocking(move || handlers::api_swap_advance(&id, &b, &state)).await
}

async fn route_swaps_refund(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    let b = parse_body(body);
    blocking(move || handlers::api_swap_refund(&id, &b, &state)).await
}

// ─── Maker-profile routes ────────────────────────────────────────────────────

async fn route_maker_profile_list(State(state): State<SharedState>) -> Response {
    blocking(move || handlers::api_maker_profile_list(&state)).await
}

async fn route_maker_profile_publish(State(state): State<SharedState>, body: Bytes) -> Response {
    let b = parse_body(body);
    blocking(move || handlers::api_maker_profile_publish(&b, &state)).await
}

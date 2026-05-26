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
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

pub mod handlers;
pub use handlers::AppState;

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
    let state: SharedState = Arc::new(Mutex::new(AppState::new()));

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
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("blocking task panicked: {e}") })),
        )
            .into_response(),
    }
}

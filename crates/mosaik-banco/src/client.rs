//! Blocking HTTP client for the `banco-proxy` gateway.
//!
//! Arkade has no Rust SDK; banco-proxy (TypeScript, sibling `services/`) wraps
//! `@arkade-os/banco`'s `Maker` and `Taker` as a small REST API and we drive
//! it from Rust with `ureq`. The endpoint shape is documented in the
//! banco-proxy README; this module is the canonical Rust-side view of it.
//!
//! Endpoints, all under `/api/v1`:
//!   POST /auth/token              exchange a wallet id for a JWT
//!   GET  /banco/identity          this wallet's pubkey + ark/introspector info
//!   POST /banco/maker/offer       build, fund and publish a banco offer
//!   POST /banco/maker/cancel      cancel an offer (after cancel_delay)
//!   GET  /banco/maker/offers      list this wallet's funded swap VTXOs
//!   POST /banco/taker/fulfill     fulfil an offer by offer hex
//!   POST /banco/taker/fulfill-by-txid  fulfil by funding txid (discovery)

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Maker + arkd-server + introspector pubkeys discovered from `/banco/identity`.
#[derive(Debug, Clone, Deserialize)]
pub struct Identity {
    #[serde(rename = "walletPubkey")]
    pub wallet_pubkey: String,
    #[serde(rename = "arkServerPubkey")]
    pub ark_server_pubkey: String,
    #[serde(rename = "introspectorPubkey")]
    pub introspector_pubkey: String,
}

/// Terms a maker passes to `POST /banco/maker/offer`.
#[derive(Debug, Clone, Serialize)]
pub struct CreateOfferRequest<'a> {
    /// Raw units of the asset the maker wants paid.
    #[serde(rename = "wantAmount")]
    pub want_amount: u64,
    /// Asset id (None = native BTC on Arkade).
    #[serde(rename = "wantAsset", skip_serializing_if = "Option::is_none")]
    pub want_asset: Option<&'a str>,
    /// Raw units of the asset the maker is selling (locked in the swap VTXO).
    #[serde(rename = "sellAmount")]
    pub sell_amount: u64,
    #[serde(rename = "sellAsset", skip_serializing_if = "Option::is_none")]
    pub sell_asset: Option<&'a str>,
    /// Unix-second timestamp after which the maker may cancel.
    #[serde(rename = "cancelDelay")]
    pub cancel_delay: u64,
    /// Whether to accept partial fills. `false` = all-or-nothing.
    #[serde(rename = "partialFill", default)]
    pub partial_fill: bool,
}

/// Response from `POST /banco/maker/offer`. Mirrors the @arkade-os/banco
/// `createOffer()` return shape, with the funding txid the proxy broadcast.
#[derive(Debug, Clone, Deserialize)]
pub struct BuiltOffer {
    #[serde(rename = "offerHex")]
    pub offer_hex: String,
    #[serde(rename = "swapPkScript")]
    pub swap_pk_script: String,
    #[serde(rename = "fundingTxid")]
    pub funding_txid: String,
    /// Funding vout (the swap VTXO inside the funding tx).
    #[serde(rename = "fundingVout")]
    pub funding_vout: u32,
}

/// A funded maker-side swap VTXO returned by `GET /banco/maker/offers`.
#[derive(Debug, Clone, Deserialize)]
pub struct MakerOffer {
    pub txid: String,
    pub vout: u32,
    pub value: u64,
    #[serde(default)]
    pub spendable: bool,
}

/// A blocking client for one banco-proxy wallet.
pub struct BancoClient {
    base_url: String,
    wallet_id: String,
    agent: ureq::Agent,
    token: std::cell::RefCell<Option<String>>,
}

impl BancoClient {
    /// `base_url` — the banco-proxy root (e.g. `http://127.0.0.1:3001`).
    /// `wallet_id` — selects which proxy-managed wallet to act as.
    pub fn new(base_url: impl Into<String>, wallet_id: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            wallet_id: wallet_id.into(),
            agent: ureq::agent(),
            token: std::cell::RefCell::new(None),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/api/v1{}", self.base_url, path)
    }

    /// Fetch (and cache) a JWT via `POST /auth/token`.
    fn token(&self) -> Result<String> {
        if let Some(t) = self.token.borrow().clone() {
            return Ok(t);
        }
        let resp: Value = self
            .agent
            .post(&self.url("/auth/token"))
            .send_json(json!({ "walletId": self.wallet_id }))
            .map_err(map_err)
            .context("requesting banco-proxy auth token")?
            .into_json()?;
        let token = resp
            .get("token")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("auth/token: no token in response"))?
            .to_string();
        *self.token.borrow_mut() = Some(token.clone());
        Ok(token)
    }

    fn get_json(&self, path: &str) -> Result<Value> {
        let token = self.token()?;
        self.agent
            .get(&self.url(path))
            .set("Authorization", &format!("Bearer {token}"))
            .call()
            .map_err(map_err)
            .with_context(|| format!("GET {path}"))?
            .into_json()
            .map_err(Into::into)
    }

    fn post_json(&self, path: &str, body: Value) -> Result<Value> {
        let token = self.token()?;
        self.agent
            .post(&self.url(path))
            .set("Authorization", &format!("Bearer {token}"))
            .send_json(body)
            .map_err(map_err)
            .with_context(|| format!("POST {path}"))?
            .into_json()
            .map_err(Into::into)
    }

    /// `GET /banco/identity` — this wallet's pubkey + ark server + introspector pubkeys.
    pub fn identity(&self) -> Result<Identity> {
        Ok(serde_json::from_value(self.get_json("/banco/identity")?)?)
    }

    /// `POST /banco/maker/offer` — build, fund, and publish a banco offer.
    pub fn create_offer(&self, req: &CreateOfferRequest) -> Result<BuiltOffer> {
        let body = serde_json::to_value(req)?;
        Ok(serde_json::from_value(
            self.post_json("/banco/maker/offer", body)?,
        )?)
    }

    /// `POST /banco/maker/cancel` — cancel an offer (after its `cancel_delay`).
    /// Returns the cancel txid.
    pub fn cancel(&self, offer_hex: &str) -> Result<String> {
        let resp = self.post_json("/banco/maker/cancel", json!({ "offerHex": offer_hex }))?;
        Ok(resp
            .get("txid")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("maker/cancel: no txid in response"))?
            .to_string())
    }

    /// `GET /banco/maker/offers?swapPkScript=…` — list this wallet's offer VTXOs.
    pub fn list_maker_offers(&self, swap_pk_script: &str) -> Result<Vec<MakerOffer>> {
        let v = self.get_json(&format!(
            "/banco/maker/offers?swapPkScript={swap_pk_script}"
        ))?;
        Ok(serde_json::from_value(v)?)
    }

    /// `POST /banco/taker/fulfill` — fulfil an offer by offer hex.
    /// `fill_amount`: pass `Some(n)` for a partial fill, `None` for full.
    /// Returns the settlement txid.
    pub fn fulfill(&self, offer_hex: &str, fill_amount: Option<u64>) -> Result<String> {
        let mut body = json!({ "offerHex": offer_hex });
        if let Some(n) = fill_amount {
            body["fillAmount"] = json!(n);
        }
        let resp = self.post_json("/banco/taker/fulfill", body)?;
        Ok(resp
            .get("txid")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("taker/fulfill: no txid in response"))?
            .to_string())
    }

    /// `POST /banco/taker/fulfill-by-txid` — discover-and-fulfil by funding txid.
    /// Useful when the offer hex was published only on-chain (no Nostr relay).
    pub fn fulfill_by_txid(&self, funding_txid: &str, fill_amount: Option<u64>) -> Result<String> {
        let mut body = json!({ "txid": funding_txid });
        if let Some(n) = fill_amount {
            body["fillAmount"] = json!(n);
        }
        let resp = self.post_json("/banco/taker/fulfill-by-txid", body)?;
        Ok(resp
            .get("txid")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("taker/fulfill-by-txid: no txid in response"))?
            .to_string())
    }
}

/// Map a `ureq` error to a clean `anyhow` error, surfacing the proxy's JSON
/// `{ "error": ... }` body on a 4xx/5xx so failures are legible.
fn map_err(e: ureq::Error) -> anyhow::Error {
    match e {
        ureq::Error::Status(code, resp) => {
            let body = resp
                .into_json::<Value>()
                .ok()
                .and_then(|v| v.get("error").and_then(Value::as_str).map(str::to_string))
                .unwrap_or_else(|| "<no body>".into());
            anyhow!("banco-proxy returned HTTP {code}: {body}")
        }
        ureq::Error::Transport(t) => anyhow!("banco-proxy transport error: {t}"),
    }
}

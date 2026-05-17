//! Minimal JSON-RPC client for an Elements / Liquid node.
//!
//! Mosaik talks to `elementsd` for the things a covenant swap needs: funding a
//! Tessera UTXO, issuing test assets, scanning, and broadcasting. Transaction
//! *construction* (the covenant spend) is done with `rust-elements`; this
//! module is only the node interface.
//!
//! Wallet RPCs (`getnewaddress`, `sendtoaddress`, …) live under a
//! `/wallet/<name>` path; node RPCs (`getblockcount`, `sendrawtransaction`, …)
//! at the root. [`ElementsRpc::wallet`] targets a wallet; [`ElementsRpc::node`]
//! the root.

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};

/// A JSON-RPC connection to an Elements node (optionally scoped to a wallet).
#[derive(Debug, Clone)]
pub struct ElementsRpc {
    url: String,
    user: String,
    pass: String,
}

impl ElementsRpc {
    /// Node-level RPC at the root path (no wallet).
    pub fn node(base_url: &str, user: &str, pass: &str) -> Self {
        Self { url: base_url.trim_end_matches('/').to_string(), user: user.into(), pass: pass.into() }
    }

    /// Wallet-scoped RPC — appends `/wallet/<name>` to the base URL.
    pub fn wallet(base_url: &str, wallet: &str, user: &str, pass: &str) -> Self {
        Self {
            url: format!("{}/wallet/{}", base_url.trim_end_matches('/'), wallet),
            user: user.into(),
            pass: pass.into(),
        }
    }

    /// The Mosaik local-regtest defaults (see `scripts/regtest.sh`).
    pub fn regtest_wallet() -> Self {
        Self::wallet("http://127.0.0.1:7040", "mosaik", "user", "pass")
    }

    /// Issue a raw JSON-RPC call and return the `result` field.
    pub fn call(&self, method: &str, params: Value) -> Result<Value> {
        let body = json!({
            "jsonrpc": "1.0",
            "id": "mosaik",
            "method": method,
            "params": params,
        });

        let resp = ureq::post(&self.url)
            .set("Authorization", &basic_auth(&self.user, &self.pass))
            .send_json(body);

        let value: Value = match resp {
            Ok(r) => r.into_json().context("RPC response was not JSON")?,
            // A non-2xx still carries a JSON-RPC error body — read it.
            Err(ureq::Error::Status(_, r)) => {
                r.into_json().context("RPC error response was not JSON")?
            }
            Err(e) => return Err(anyhow!("RPC transport error for {method}: {e}")),
        };

        if let Some(err) = value.get("error").filter(|e| !e.is_null()) {
            return Err(anyhow!("RPC {method} failed: {err}"));
        }
        value
            .get("result")
            .cloned()
            .ok_or_else(|| anyhow!("RPC {method}: response had no result"))
    }

    // ---- node helpers ------------------------------------------------------

    pub fn block_count(&self) -> Result<u64> {
        Ok(self.call("getblockcount", json!([]))?.as_u64().unwrap_or(0))
    }

    pub fn send_raw_transaction(&self, tx_hex: &str) -> Result<String> {
        Ok(self
            .call("sendrawtransaction", json!([tx_hex]))?
            .as_str()
            .unwrap_or_default()
            .to_string())
    }

    pub fn raw_transaction(&self, txid: &str) -> Result<Value> {
        self.call("getrawtransaction", json!([txid, true]))
    }

    /// The chain's genesis block hash — the Simplicity `sig_all` hash is bound
    /// to it, so a signed covenant spend must use the right one.
    pub fn genesis_hash(&self) -> Result<String> {
        Ok(self
            .call("getblockhash", json!([0]))?
            .as_str()
            .unwrap_or_default()
            .to_string())
    }

    /// Make sure wallet `name` exists and is loaded. Idempotent: a fresh node
    /// gets it created, an existing one gets loaded, an already-loaded one is
    /// left alone. Errors from any of those races are swallowed on purpose.
    pub fn ensure_wallet(&self, name: &str) -> Result<()> {
        if self.call("createwallet", json!([name])).is_ok() {
            return Ok(());
        }
        let _ = self.call("loadwallet", json!([name]));
        Ok(())
    }

    // ---- wallet helpers ----------------------------------------------------

    pub fn new_address(&self) -> Result<String> {
        Ok(self
            .call("getnewaddress", json!([]))?
            .as_str()
            .unwrap_or_default()
            .to_string())
    }

    /// A fresh address in unconfidential form — paying it produces an explicit
    /// (unblinded) output, which the Tessera covenant requires.
    pub fn new_unconfidential_address(&self) -> Result<String> {
        let addr = self.new_address()?;
        let info = self.call("getaddressinfo", json!([addr]))?;
        Ok(info
            .get("unconfidential")
            .and_then(Value::as_str)
            .unwrap_or(&addr)
            .to_string())
    }

    /// Mine `n` blocks to a fresh wallet address; returns the block hashes.
    pub fn generate(&self, n: u64) -> Result<Vec<String>> {
        let addr = self.new_address()?;
        let res = self.call("generatetoaddress", json!([n, addr]))?;
        Ok(res
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default())
    }

    /// Send L-BTC to `address`; returns the funding txid.
    pub fn send_to_address(&self, address: &str, amount_btc: f64) -> Result<String> {
        Ok(self
            .call("sendtoaddress", json!([address, amount_btc]))?
            .as_str()
            .unwrap_or_default()
            .to_string())
    }

    /// Send `amount` (whole units) of `asset` to `address`; returns the txid.
    /// Paying an unconfidential address yields an explicit (unblinded) output.
    pub fn send_asset_to(&self, address: &str, amount: f64, asset: &str) -> Result<String> {
        Ok(self
            .call(
                "sendtoaddress",
                json!([address, amount, "", "", false, false, 1, "unset", false, asset]),
            )?
            .as_str()
            .unwrap_or_default()
            .to_string())
    }

    /// Issue a new Liquid asset; returns `(asset_id, issuance_txid)`.
    pub fn issue_asset(&self, asset_amount: f64, token_amount: f64) -> Result<(String, String)> {
        let res = self.call("issueasset", json!([asset_amount, token_amount]))?;
        let asset = res.get("asset").and_then(Value::as_str).unwrap_or_default().to_string();
        let txid = res.get("txid").and_then(Value::as_str).unwrap_or_default().to_string();
        Ok((asset, txid))
    }

    /// Confirmed unspent outputs for this wallet.
    pub fn list_unspent(&self) -> Result<Value> {
        self.call("listunspent", json!([]))
    }

    /// Balance map `{asset_id_or_label: amount}` for this wallet.
    pub fn balances(&self) -> Result<Value> {
        self.call("getbalance", json!([]))
    }

    /// The network's L-BTC (policy) asset id, in RPC display order.
    pub fn policy_asset(&self) -> Result<String> {
        Ok(self
            .call("dumpassetlabels", json!([]))?
            .get("bitcoin")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("no bitcoin asset label"))?
            .to_string())
    }

    /// Sign this wallet's inputs of a PSET; returns the updated PSET.
    pub fn wallet_process_psbt(&self, pset: &str) -> Result<String> {
        Ok(self
            .call("walletprocesspsbt", json!([pset]))?
            .get("psbt")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("walletprocesspsbt: no psbt in result"))?
            .to_string())
    }

    /// The first confirmed *explicit* (unblinded) unspent output of `asset` with
    /// at least `min` units (raw). Returns `(txid, vout, amount_raw)`.
    ///
    /// Blinded (confidential) outputs are skipped: a Tessera settlement pays the
    /// maker with explicit asset-B outputs, and mixing a confidential input with
    /// explicit outputs unbalances the transaction's blinding factors.
    pub fn unspent_of_asset(&self, asset: &str, min: u64) -> Result<Option<(String, u64, u64)>> {
        let unspent = self.list_unspent()?;
        Ok(unspent.as_array().and_then(|arr| {
            arr.iter().find_map(|u| {
                if u.get("asset")?.as_str()? != asset {
                    return None;
                }
                // Skip confidential outputs — only explicit ones can fund a
                // covenant settlement without rebalancing blinders.
                if u.get("amountcommitment").is_some() {
                    return None;
                }
                let raw = (u.get("amount")?.as_f64()? * 1e8).round() as u64;
                if raw < min {
                    return None;
                }
                Some((
                    u.get("txid")?.as_str()?.to_string(),
                    u.get("vout")?.as_u64()?,
                    raw,
                ))
            })
        }))
    }

    /// The scriptPubKey (hex) of an address.
    pub fn address_script_pubkey(&self, address: &str) -> Result<String> {
        Ok(self
            .call("getaddressinfo", json!([address]))?
            .get("scriptPubKey")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("no scriptPubKey for {address}"))?
            .to_string())
    }
}

fn basic_auth(user: &str, pass: &str) -> String {
    use std::fmt::Write;
    // Base64 of "user:pass" — small, no extra crate.
    let raw = format!("{user}:{pass}");
    const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::from("Basic ");
    for chunk in raw.as_bytes().chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        let _ = write!(out, "{}", B64[(n >> 18 & 63) as usize] as char);
        let _ = write!(out, "{}", B64[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 { B64[(n >> 6 & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { B64[(n & 63) as usize] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_auth_encodes_correctly() {
        // "user:pass" -> dXNlcjpwYXNz
        assert_eq!(basic_auth("user", "pass"), "Basic dXNlcjpwYXNz");
    }

    #[test]
    fn wallet_url_is_built() {
        let rpc = ElementsRpc::wallet("http://127.0.0.1:7040/", "mosaik", "u", "p");
        assert_eq!(rpc.url, "http://127.0.0.1:7040/wallet/mosaik");
    }
}

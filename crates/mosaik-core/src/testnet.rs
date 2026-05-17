//! Liquid testnet backend — Esplora + the public faucet + hal-simplicity.
//!
//! No local `elementsd`. Mirrors the Blockstream simplicity-codespace pattern:
//!   * Public faucet (`liquidtestnet.com/faucet`) funds the covenant address.
//!   * Public Esplora (`blockstream.info/liquidtestnet/api`) reads chain state
//!     and broadcasts raw transactions.
//!   * `hal-simplicity` builds and finalises PSETs.
//!   * Reclaim uses `tessera::build_refund_tx` (no node needed).
//!
//! The maker has no on-chain "wallet" — they own a fixed address and a private
//! key only used for the REFUND path. The taker plays no signing role for an
//! L-BTC→L-BTC SETTLE (the covenant UTXO covers the fee and the maker payout).

use std::collections::BTreeMap;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::Offer;

pub const ESPLORA_DEFAULT: &str = "https://blockstream.info/liquidtestnet/api";
pub const FAUCET_DEFAULT: &str = "https://liquidtestnet.com/faucet";

/// Liquid testnet L-BTC asset id, RPC display order.
pub const LBTC_TESTNET_DISPLAY: &str =
    "144c654344aa716d6f3abcc1ca90e5641e4e2a7f633bc09fe3baf64585819a49";

/// L-BTC testnet asset id in internal (tx/jet) byte order — the reverse.
pub const LBTC_TESTNET_INTERNAL: [u8; 32] = [
    0x49, 0x9a, 0x81, 0x85, 0x45, 0xf6, 0xba, 0xe3,
    0xf5, 0xf6, 0x03, 0xb6, 0x37, 0xf2, 0xa4, 0xe1,
    0xe6, 0x4e, 0x59, 0x0c, 0xac, 0x1b, 0xc3, 0xa6,
    0xf6, 0xd7, 0x1a, 0xa4, 0x44, 0x36, 0x54, 0xc1,
];

/// Demo maker address — also the Liquid testnet faucet return address.
/// SHA-256 of its scriptPubKey is what the Tessera `maker_spk_hash` commits to.
pub const DEMO_MAKER_ADDRESS: &str = "tex1qkkxzy9glfws4nc392an5w2kgjym7sxpshuwkjy";
pub const DEMO_MAKER_SPK_HASH_HEX: &str =
    "bcfbe70502021903755bb406a7c4681817be317affc7d1120de2041a9e06cfc5";

/// A second fixed demo address — used as the "taker" address in the UI.
/// On testnet the taker plays no signing role for an L-BTC→L-BTC SETTLE, so this
/// is mostly decorative. It's a well-formed bech32 testnet address.
pub const DEMO_TAKER_ADDRESS: &str = "tex1q4f9q57v6e0vc4l0qy7gha2akz6kw2ekfk22dum";

/// BIP-341 NUMS unspendable internal key (same as the tessera crate).
pub const NUMS_INTERNAL_KEY_HEX: &str =
    "50929b74c1a04954b78b4b6035e97a5e078a5a0f28ec96d547bfee9ace803ac0";

/// Default per-tx fee for testnet demos (sats).
pub const DEFAULT_FEE_SATS: u64 = 500;

// ── Esplora ──────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct Esplora {
    base: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EsploraUtxo {
    pub txid: String,
    pub vout: u64,
    /// Sats — absent for confidential outputs, only present for explicit ones.
    #[serde(default)]
    pub value: Option<u64>,
    /// Asset id in display order (only present on Liquid for explicit outputs).
    #[serde(default)]
    pub asset: Option<String>,
    pub status: EsploraStatus,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EsploraStatus {
    pub confirmed: bool,
    #[serde(default)]
    pub block_height: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EsploraVout {
    pub scriptpubkey: String,
    #[serde(default)]
    pub asset: Option<String>,
    #[serde(default)]
    pub value: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EsploraTx {
    pub txid: String,
    pub vout: Vec<EsploraVout>,
    pub status: EsploraStatus,
}

impl Esplora {
    pub fn new(base: &str) -> Self {
        Self { base: base.trim_end_matches('/').to_string() }
    }

    pub fn testnet() -> Self {
        Self::new(ESPLORA_DEFAULT)
    }

    fn get_json(&self, path: &str) -> Result<Value> {
        let url = format!("{}{}", self.base, path);
        let resp = ureq::get(&url).call()
            .map_err(|e| anyhow!("esplora GET {url}: {e}"))?;
        resp.into_json::<Value>().context("esplora response not JSON")
    }

    fn get_text(&self, path: &str) -> Result<String> {
        let url = format!("{}{}", self.base, path);
        let resp = ureq::get(&url).call()
            .map_err(|e| anyhow!("esplora GET {url}: {e}"))?;
        resp.into_string().context("esplora response not text")
    }

    pub fn block_height(&self) -> Result<u64> {
        self.get_text("/blocks/tip/height")?
            .trim().parse()
            .context("parse block height")
    }

    pub fn genesis_hash(&self) -> Result<String> {
        self.get_text("/block-height/0").map(|s| s.trim().to_string())
    }

    /// Sum of explicit (unblinded) L-BTC UTXOs for an address, in sats.
    ///
    /// Liquid's `/address/{a}` endpoint returns tx counts, not amounts, because
    /// most UTXOs are confidential. We list the address's UTXOs and sum any
    /// that are explicit L-BTC — confidential ones are skipped silently.
    pub fn address_lbtc_balance(&self, address: &str) -> Result<u64> {
        let utxos = self.address_utxos(address)?;
        Ok(utxos.iter()
            .filter(|u| u.asset.as_deref() == Some(LBTC_TESTNET_DISPLAY))
            .filter_map(|u| u.value)
            .sum())
    }

    /// Unspent outputs for an address.
    pub fn address_utxos(&self, address: &str) -> Result<Vec<EsploraUtxo>> {
        let v = self.get_json(&format!("/address/{address}/utxo"))?;
        serde_json::from_value(v).context("decode utxo list")
    }

    pub fn tx(&self, txid: &str) -> Result<EsploraTx> {
        let v = self.get_json(&format!("/tx/{txid}"))?;
        serde_json::from_value(v).context("decode tx")
    }

    /// Broadcast a raw tx hex; returns the txid.
    pub fn broadcast(&self, tx_hex: &str) -> Result<String> {
        let url = format!("{}/tx", self.base);
        let resp = ureq::post(&url)
            .set("Content-Type", "text/plain")
            .send_string(tx_hex);
        match resp {
            Ok(r) => Ok(r.into_string().context("broadcast response")?.trim().to_string()),
            Err(ureq::Error::Status(_, r)) => {
                let body = r.into_string().unwrap_or_default();
                Err(anyhow!("broadcast rejected: {body}"))
            }
            Err(e) => Err(anyhow!("broadcast transport error: {e}")),
        }
    }
}

// ── Faucet ───────────────────────────────────────────────────────────────────

pub struct Faucet {
    base: String,
}

impl Faucet {
    pub fn new(base: &str) -> Self {
        Self { base: base.trim_end_matches('/').to_string() }
    }

    pub fn testnet() -> Self {
        Self::new(FAUCET_DEFAULT)
    }

    /// Request L-BTC from the faucet for `address`; returns the funding txid.
    pub fn request_lbtc(&self, address: &str) -> Result<String> {
        let url = format!("{}?address={}&action=lbtc", self.base, address);
        let resp = ureq::get(&url).call();
        let text = match resp {
            Ok(r) => r.into_string().context("faucet response")?,
            Err(ureq::Error::Status(_, r)) => {
                let body = r.into_string().unwrap_or_default();
                return Err(anyhow!("faucet rejected: {body}"));
            }
            Err(e) => return Err(anyhow!("faucet transport: {e}")),
        };
        // Try JSON first.
        if let Ok(v) = serde_json::from_str::<Value>(&text) {
            if let Some(txid) = v.get("txid").or_else(|| v.get("tx_hash")).and_then(Value::as_str) {
                return Ok(txid.to_string());
            }
            if let Some(err) = v.get("error").and_then(Value::as_str) {
                return Err(anyhow!("faucet error: {err}"));
            }
        }
        // Fall back to HTML scrape — find a hex64 token.
        let re_match = text
            .as_bytes()
            .windows(64)
            .find(|w| w.iter().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')));
        re_match
            .and_then(|w| std::str::from_utf8(w).ok())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow!("no txid in faucet response: {text}"))
    }
}

// ── Tessera helpers for testnet (do not need a node) ─────────────────────────

/// Build a Tessera for the demo maker address, L-BTC→L-BTC, `amount_b` sats.
pub fn demo_tessera(amount_b: u64, timeout: u32, maker_pk: [u8; 32]) -> Result<crate::Tessera> {
    let mut spk_hash = [0u8; 32];
    spk_hash.copy_from_slice(&hex::decode(DEMO_MAKER_SPK_HASH_HEX)?);
    Ok(crate::Tessera {
        asset_b: LBTC_TESTNET_INTERNAL,
        amount_b,
        maker_spk_hash: spk_hash,
        timeout,
        maker_pk,
    })
}

// ── Settlement (SETTLE path) ─────────────────────────────────────────────────

/// Build, finalise and broadcast a SETTLE settlement for an offer funded on
/// Liquid testnet. Returns the broadcast txid.
///
/// Assumes the offer was created with `demo_tessera` (L-BTC→L-BTC). The
/// covenant UTXO holds `value_sats`; the maker is paid exactly `amount_b_sats`
/// at output 0; any surplus (value − amount_b − fee) becomes a second output
/// also to the maker; fee is `DEFAULT_FEE_SATS`.
pub fn settle_via_hal(
    offer: &Offer,
    value_sats: u64,
    esplora: &Esplora,
) -> Result<String> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};

    let (txid, vout_str) = offer.outpoint.split_once(':')
        .ok_or_else(|| anyhow!("bad outpoint: {}", offer.outpoint))?;
    let covenant_vout: u32 = vout_str.parse()?;

    let amount_b = offer.tessera.amount_b;
    let fee = DEFAULT_FEE_SATS;
    if value_sats < amount_b + fee {
        anyhow::bail!(
            "covenant UTXO ({value_sats} sats) too small for amount_b ({amount_b}) + fee ({fee})"
        );
    }
    let change_sats = value_sats - amount_b - fee;

    let btc = |s: u64| s as f64 / 1e8;
    let maker_pay_btc = btc(amount_b);
    let fee_btc       = btc(fee);
    let change_btc    = btc(change_sats);

    // Outputs: vout 0 → maker (covenant-checked), optional change to maker, fee.
    let mut outputs = vec![json!({
        "address": offer.maker_address,
        "asset":   LBTC_TESTNET_DISPLAY,
        "amount":  maker_pay_btc,
    })];
    if change_sats > 0 {
        outputs.push(json!({
            "address": offer.maker_address,
            "asset":   LBTC_TESTNET_DISPLAY,
            "amount":  change_btc,
        }));
    }
    outputs.push(json!({ "fee": fee_btc }));

    let inputs = json!([{ "txid": txid, "vout": covenant_vout }]);
    let outputs_json = Value::Array(outputs);

    // 1. pset create
    let pset1 = hal_pset(&[
        "simplicity", "pset", "create", "--liquid",
        &inputs.to_string(),
        &outputs_json.to_string(),
    ])?;

    // 2. pset update-input — covenant SPK comes from compiling the Tessera.
    let compiled = offer.tessera.compile()?;
    let covenant_spk = hex::encode(compiled.address()?.script_pubkey().as_bytes());
    let cmr = compiled.cmr_hex();
    let utxo_arg = format!("{covenant_spk}:{}:{:.8}", LBTC_TESTNET_DISPLAY, btc(value_sats));
    let pset2 = hal_pset(&[
        "simplicity", "pset", "update-input", "--liquid",
        &pset1, "0",
        "-i", &utxo_arg,
        "-c", &cmr,
        "-p", NUMS_INTERNAL_KEY_HEX,
    ])?;

    // 3. settle witness
    let w = offer.tessera.settle_witness(0)?;
    let program_b64 = B64.encode(&w.program);
    let witness_hex = hex::encode(&w.witness);

    // 4. pset finalize
    let pset3 = hal_pset(&[
        "simplicity", "pset", "finalize", "--liquid",
        &pset2, "0", &program_b64, &witness_hex,
    ])?;

    // 5. pset extract → raw tx
    let raw_tx = hal_extract(&pset3)?;

    // 6. broadcast
    esplora.broadcast(&raw_tx)
}

/// Build a deliberately-broken settlement to demonstrate covenant rejection.
/// `cheat` picks the attack; `value_sats` is the covenant UTXO's value.
pub fn settle_cheat_via_hal(
    offer: &Offer,
    value_sats: u64,
    cheat: crate::Cheat,
    esplora: &Esplora,
) -> Result<String> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};

    let (txid, vout_str) = offer.outpoint.split_once(':')
        .ok_or_else(|| anyhow!("bad outpoint: {}", offer.outpoint))?;
    let covenant_vout: u32 = vout_str.parse()?;

    let amount_b = offer.tessera.amount_b;
    let fee = DEFAULT_FEE_SATS;
    if value_sats < amount_b + fee {
        anyhow::bail!("covenant UTXO too small for amount_b + fee");
    }
    let change_sats = value_sats - amount_b - fee;
    let btc = |s: u64| s as f64 / 1e8;

    // Bend each cheat into the output structure.
    let pay_amount = match cheat {
        crate::Cheat::Underpay => amount_b.saturating_sub(1),
        _ => amount_b,
    };
    let recipient = match cheat {
        crate::Cheat::WrongRecipient => DEMO_TAKER_ADDRESS,
        _ => offer.maker_address.as_str(),
    };

    let mut outputs: Vec<Value> = Vec::new();
    let maker_out = json!({
        "address": recipient,
        "asset":   LBTC_TESTNET_DISPLAY,
        "amount":  btc(pay_amount),
    });
    let change_out = if change_sats > 0 {
        Some(json!({
            "address": offer.maker_address,
            "asset":   LBTC_TESTNET_DISPLAY,
            "amount":  btc(change_sats),
        }))
    } else {
        None
    };
    // WrongIndex: put the "settle" output at index 1 instead of 0.
    if cheat == crate::Cheat::WrongIndex && change_out.is_some() {
        outputs.push(change_out.clone().unwrap());
        outputs.push(maker_out);
    } else {
        outputs.push(maker_out);
        if let Some(c) = change_out {
            outputs.push(c);
        }
    }
    outputs.push(json!({ "fee": btc(fee) }));

    let inputs = json!([{ "txid": txid, "vout": covenant_vout }]);
    let outputs_json = Value::Array(outputs);

    let pset1 = hal_pset(&[
        "simplicity", "pset", "create", "--liquid",
        &inputs.to_string(), &outputs_json.to_string(),
    ])?;

    let compiled = offer.tessera.compile()?;
    let covenant_spk = hex::encode(compiled.address()?.script_pubkey().as_bytes());
    let utxo_arg = format!("{covenant_spk}:{}:{:.8}", LBTC_TESTNET_DISPLAY, btc(value_sats));
    let pset2 = hal_pset(&[
        "simplicity", "pset", "update-input", "--liquid",
        &pset1, "0",
        "-i", &utxo_arg,
        "-c", &compiled.cmr_hex(),
        "-p", NUMS_INTERNAL_KEY_HEX,
    ])?;

    let w = offer.tessera.settle_witness(0)?;
    let pset3 = hal_pset(&[
        "simplicity", "pset", "finalize", "--liquid",
        &pset2, "0", &B64.encode(&w.program), &hex::encode(&w.witness),
    ])?;

    let raw_tx = hal_extract(&pset3)?;
    esplora.broadcast(&raw_tx)
}

// ── REFUND (uses tessera::build_refund_tx; no hal-simplicity needed) ──────────

/// Reclaim an unfilled offer on testnet via the REFUND path.
/// Returns the broadcast txid.
pub fn reclaim_via_esplora(
    offer: &Offer,
    value_sats: u64,
    maker_secret: &[u8; 32],
    esplora: &Esplora,
) -> Result<String> {
    let (txid, vout_str) = offer.outpoint.split_once(':')
        .ok_or_else(|| anyhow!("bad outpoint: {}", offer.outpoint))?;
    let covenant_vout: u32 = vout_str.parse()?;

    let fee = DEFAULT_FEE_SATS;
    if value_sats <= fee {
        anyhow::bail!("covenant UTXO ({value_sats}) too small to cover fee ({fee})");
    }
    let amount_to_maker = value_sats - fee;
    let timeout = offer.tessera.timeout;

    let height = esplora.block_height()?;
    if (height as u32) < timeout {
        anyhow::bail!("refund locked until height {timeout}; chain is at {height}");
    }

    // Build the unsigned refund tx with rust-elements (the same crate the
    // tessera::build_refund_tx already uses for the covenant witness).
    let raw_hex = build_unsigned_refund_hex(
        txid, covenant_vout, &offer.maker_address, amount_to_maker, fee, timeout,
    )?;

    let compiled = offer.tessera.compile()?;
    let spk_bytes = compiled.address()?.script_pubkey().as_bytes().to_vec();
    let genesis = esplora.genesis_hash()?;

    let raw_tx = offer.tessera.build_refund_tx(
        &raw_hex, 0, &spk_bytes, LBTC_TESTNET_DISPLAY, value_sats,
        &genesis, maker_secret,
    )?;

    esplora.broadcast(&raw_tx)
}

/// Construct an unsigned Liquid testnet refund tx (one covenant input → maker +
/// fee), `nLockTime = timeout`, sequence ≠ 0xfffffffe so locktime is enforced.
fn build_unsigned_refund_hex(
    in_txid: &str,
    in_vout: u32,
    maker_address: &str,
    amount_to_maker: u64,
    fee: u64,
    locktime: u32,
) -> Result<String> {
    use std::str::FromStr;
    use simplicityhl::elements::{
        confidential, encode::serialize_hex, AssetId, Address, OutPoint, Script,
        Sequence, Transaction, TxIn, TxInWitness, TxOut, TxOutWitness, Txid,
    };

    let prev_txid = Txid::from_str(in_txid).map_err(|e| anyhow!("txid: {e}"))?;
    let asset = AssetId::from_str(LBTC_TESTNET_DISPLAY).map_err(|e| anyhow!("asset: {e}"))?;

    let input = TxIn {
        previous_output: OutPoint::new(prev_txid, in_vout),
        is_pegin: false,
        script_sig: Script::new(),
        sequence: Sequence::from_consensus(0xfffffffe),
        asset_issuance: Default::default(),
        witness: TxInWitness::default(),
    };

    let maker_addr = Address::from_str(maker_address).map_err(|e| anyhow!("maker addr: {e}"))?;
    let maker_out = TxOut {
        asset: confidential::Asset::Explicit(asset),
        value: confidential::Value::Explicit(amount_to_maker),
        nonce: confidential::Nonce::Null,
        script_pubkey: maker_addr.script_pubkey(),
        witness: TxOutWitness::default(),
    };
    let fee_out = TxOut::new_fee(fee, asset);

    let tx = Transaction {
        version: 2,
        lock_time: simplicityhl::elements::LockTime::from_height(locktime)
            .map_err(|e| anyhow!("lock_time: {e}"))?,
        input: vec![input],
        output: vec![maker_out, fee_out],
    };
    Ok(serialize_hex(&tx))
}

// ── hal-simplicity shell-out helpers ─────────────────────────────────────────

fn hal_run(args: &[&str]) -> Result<String> {
    let bin = std::env::var("HAL_SIMPLICITY").unwrap_or_else(|_| "hal-simplicity".into());
    let out = std::process::Command::new(&bin).args(args).output()
        .map_err(|e| anyhow!("running {bin}: {e}"))?;
    if !out.status.success() {
        anyhow::bail!(
            "hal-simplicity {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn hal_pset(args: &[&str]) -> Result<String> {
    let output = hal_run(args)?;
    let v: Value = serde_json::from_str(&output)
        .map_err(|e| anyhow!("hal-simplicity output not JSON: {e}\n{output}"))?;
    if let Some(err) = v.get("error").filter(|e| !e.is_null()) {
        anyhow::bail!("hal-simplicity error: {err}");
    }
    Ok(v.get("pset")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("hal-simplicity: no pset in output"))?
        .to_string())
}

fn hal_extract(pset: &str) -> Result<String> {
    let output = hal_run(&["simplicity", "pset", "extract", "--liquid", pset])?;
    let v: Value = serde_json::from_str(&output)
        .map_err(|e| anyhow!("hal extract not JSON: {e}\n{output}"))?;
    if let Some(s) = v.get("transaction_hex").and_then(Value::as_str) {
        return Ok(s.to_string());
    }
    if let Some(s) = v.get("hex").and_then(Value::as_str) {
        return Ok(s.to_string());
    }
    // Some hal-simplicity versions wrap the raw tx in another shape.
    if let Some(s) = v.as_str() {
        return Ok(s.to_string());
    }
    Err(anyhow!("hal extract: unknown shape: {output}"))
}

// ── Offer state tracking ─────────────────────────────────────────────────────

/// What the UI shows about a pending testnet offer.
#[derive(Debug, Clone, Serialize)]
pub struct OfferState {
    pub status: OfferStatus,
    /// Confirmed value of the covenant UTXO, if known.
    pub value_sats: Option<u64>,
    /// Block height the funding tx confirmed at, if any.
    pub confirmed_at: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OfferStatus {
    /// Faucet hit; waiting for the funding tx to appear and confirm.
    PendingFaucet,
    /// Funding tx confirmed; the offer can be settled.
    Ready,
    /// Settlement broadcast; UI can stop tracking.
    Settled,
}

/// Probe Esplora for an offer's funding tx and report its state.
pub fn probe_offer(offer: &Offer, faucet_txid: &str, esplora: &Esplora) -> OfferState {
    match esplora.tx(faucet_txid) {
        Ok(tx) => {
            // Find the covenant vout in the funding tx.
            let (_, vout_str) = match offer.outpoint.split_once(':') {
                Some(p) => p, None => return OfferState {
                    status: OfferStatus::PendingFaucet, value_sats: None, confirmed_at: None,
                },
            };
            let vout: usize = vout_str.parse().unwrap_or(0);
            let value = tx.vout.get(vout).and_then(|v| v.value);
            let status = if tx.status.confirmed {
                OfferStatus::Ready
            } else {
                OfferStatus::PendingFaucet
            };
            OfferState {
                status,
                value_sats: value,
                confirmed_at: tx.status.block_height,
            }
        }
        Err(_) => OfferState {
            status: OfferStatus::PendingFaucet, value_sats: None, confirmed_at: None,
        },
    }
}

// ── Tracked offer table (server-side state) ──────────────────────────────────

/// A testnet offer, augmented with the faucet tx that funded it.
#[derive(Debug, Clone)]
pub struct TestnetOffer {
    pub offer: Offer,
    pub faucet_txid: String,
    pub covenant_address: String,
}

/// Map of an address → its current L-BTC balance (sats), with a short cache.
pub fn lbtc_balances(esplora: &Esplora, addrs: &[&str]) -> BTreeMap<String, u64> {
    let mut out = BTreeMap::new();
    for a in addrs {
        let v = esplora.address_lbtc_balance(a).unwrap_or(0);
        out.insert((*a).to_string(), v);
    }
    out
}

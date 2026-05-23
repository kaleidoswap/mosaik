//! Mosaik core: offers, and the Liquid PSET / key plumbing.
//!
//! This crate turns a [`tessera::Tessera`] into a live offer (a funded covenant
//! UTXO) and builds the transactions that fill or reclaim it, against a real
//! Elements node over RPC. See `docs/DESIGN.md` for the full protocol.

use anyhow::Result;
use serde::{Deserialize, Serialize};

pub mod lwk;
pub mod rpc;

pub use tessera::Tessera;

/// A published offer: a funded covenant UTXO plus the terms needed to fill it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Offer {
    /// The covenant UTXO, as an Elements outpoint string `"txid:vout"`.
    pub outpoint: String,
    /// Asset id (hex) locked in the UTXO — the asset the maker is selling.
    pub asset_a: String,
    /// Amount of `asset_a` locked.
    pub amount_a: u64,
    /// The Tessera — what the maker wants in return, and the refund conditions.
    pub tessera: Tessera,
    /// The exact covenant address that received the funded output.
    pub covenant_address: String,
    /// The (unconfidential) address the counter-payment must go to. The
    /// covenant commits to its scriptPubKey hash via `tessera.maker_spk_hash`.
    pub maker_address: String,
}

/// Maker side — publish an offer.
pub trait MakeOffer {
    /// Fund a covenant UTXO with `amount_a` of `asset_a` and return the [`Offer`].
    ///
    /// `maker_address` is the unconfidential address the counter-payment must
    /// reach — the covenant commits to its scriptPubKey hash.
    fn make_offer(
        &self,
        asset_a: &str,
        amount_a: u64,
        tessera: &Tessera,
        maker_address: &str,
    ) -> Result<Offer>;
}

/// Build a [`Tessera`] from the maker scriptPubKey and terms.
///
/// `maker_spk` is the raw scriptPubKey bytes; `asset_b_display` is the asset id
/// in RPC display order (reversed internally).
pub fn build_tessera(
    maker_spk: &[u8],
    asset_b_display: &str,
    amount_b: u64,
    timeout: u32,
) -> Result<Tessera> {
    use sha2::{Digest, Sha256};
    let maker_spk_hash: [u8; 32] = Sha256::digest(maker_spk).into();
    let mut asset_b = hex::decode(asset_b_display)?;
    asset_b.reverse();
    let asset_b: [u8; 32] = asset_b
        .try_into()
        .map_err(|_| anyhow::anyhow!("asset_b id is not 32 bytes"))?;
    Ok(Tessera { asset_b, amount_b, maker_spk_hash, timeout })
}

/// Build a Tessera whose terms match an L-BTC payment of `amount_b` to
/// `maker_address`, so a Simplicity node accepts the settlement.
pub fn lbtc_tessera(
    rpc: &rpc::ElementsRpc,
    maker_address: &str,
    amount_b: u64,
    timeout: u32,
) -> Result<Tessera> {
    let lbtc = rpc.policy_asset()?;
    let spk = hex::decode(rpc.address_script_pubkey(maker_address)?)?;
    build_tessera(&spk, &lbtc, amount_b, timeout)
}

/// Build a [`Tessera`] whose SETTLE path requires `amount_b` of `asset_b_display`
/// (an asset id in RPC display order — e.g. an issued USDT) paid to the maker.
pub fn tessera_for(
    rpc: &rpc::ElementsRpc,
    maker_address: &str,
    asset_b_display: &str,
    amount_b: u64,
    timeout: u32,
) -> Result<Tessera> {
    let spk = hex::decode(rpc.address_script_pubkey(maker_address)?)?;
    build_tessera(&spk, asset_b_display, amount_b, timeout)
}

/// A maker that publishes offers against an Elements node.
pub struct MosaikMaker {
    rpc: rpc::ElementsRpc,
}

impl MosaikMaker {
    pub fn new(rpc: rpc::ElementsRpc) -> Self {
        Self { rpc }
    }

    /// A maker wired to the local Mosaik regtest (see `scripts/regtest.sh`).
    pub fn regtest() -> Self {
        Self::new(rpc::ElementsRpc::regtest_wallet())
    }
}

/// Resolve an asset label to its RPC display-order id. `"BTC"` / `"LBTC"` /
/// `"L-BTC"` map to the network policy asset; anything else is taken to be a
/// hex asset id already in display order.
fn resolve_asset(rpc: &rpc::ElementsRpc, label: &str) -> Result<String> {
    match label.to_ascii_uppercase().as_str() {
        "BTC" | "LBTC" | "L-BTC" => rpc.policy_asset(),
        _ => Ok(label.to_string()),
    }
}

impl MakeOffer for MosaikMaker {
    /// Compile the Tessera, derive its covenant address, fund it on-chain, and
    /// return the published [`Offer`].
    ///
    /// `asset_a` is the asset the maker locks — `"BTC"` for L-BTC, or any Liquid
    /// asset id. The covenant itself never inspects the locked asset (it only
    /// enforces the maker's counter-payment), so a Tessera works for L-BTC/asset
    /// and asset/asset offers in either direction. Funding any P2TR address is a
    /// normal payment, so this runs on stock `elementsd`; only *spending* the
    /// covenant needs a Simplicity-capable node.
    fn make_offer(
        &self,
        asset_a: &str,
        amount_a: u64,
        tessera: &Tessera,
        maker_address: &str,
    ) -> Result<Offer> {
        let lbtc = self.rpc.policy_asset()?;
        let asset_a_id = resolve_asset(&self.rpc, asset_a)?;

        // Compile the covenant and derive its Taproot address. The bech32
        // prefix (ert1p / tex1p / lq1p) must match the chain the node is on,
        // otherwise sendtoaddress rejects it as "Invalid Bitcoin address".
        let compiled = tessera.compile()?;
        let params = address_params_for(&self.rpc)?;
        let address = compiled.address_for(params)?;
        let spk_hex = hex::encode(address.script_pubkey().as_bytes());

        // Fund the covenant UTXO with `asset_a` and confirm it. A P2TR address
        // has no blinding key, so the funding output is explicit either way.
        let amount = amount_a as f64 / 1e8;
        let txid = if asset_a_id == lbtc {
            self.rpc.send_to_address(&address.to_string(), amount)?
        } else {
            self.rpc.send_asset_to(&address.to_string(), amount, &asset_a_id)?
        };
        // Mine a block to confirm the funding tx (regtest). On testnet this
        // fails gracefully — the tx is visible in the mempool immediately.
        let _ = self.rpc.generate(1);

        // Locate the funding output among the transaction's vouts.
        let tx = self.rpc.raw_transaction(&txid)?;
        let vout = find_output_index(&tx, &spk_hex)
            .ok_or_else(|| anyhow::anyhow!("funding output for {txid} not found"))?;

        Ok(Offer {
            outpoint: format!("{txid}:{vout}"),
            asset_a: asset_a_id,
            amount_a,
            tessera: tessera.clone(),
            covenant_address: address.to_string(),
            maker_address: maker_address.to_string(),
        })
    }
}

/// Pick the right Liquid address params from the node's reported chain.
/// Falls back to elementsregtest (`ert…`) if the chain is unknown.
fn address_params_for(rpc: &rpc::ElementsRpc) -> Result<&'static simplicityhl::elements::AddressParams> {
    use simplicityhl::elements::AddressParams;
    let info = rpc.call("getblockchaininfo", serde_json::json!([]))?;
    Ok(match info.get("chain").and_then(serde_json::Value::as_str) {
        Some("liquidtestnet") => &AddressParams::LIQUID_TESTNET,
        Some("liquidv1") => &AddressParams::LIQUID,
        _ => &AddressParams::ELEMENTS,
    })
}

/// Find the index of the output whose scriptPubKey hex matches `spk_hex`.
pub fn find_output_index(tx: &serde_json::Value, spk_hex: &str) -> Option<u64> {
    tx.get("vout")?.as_array()?.iter().find_map(|out| {
        let hex = out.get("scriptPubKey")?.get("hex")?.as_str()?;
        (hex == spk_hex).then(|| out.get("n")?.as_u64()).flatten()
    })
}

/// Taker side — fill an offer.
pub trait TakeOffer {
    /// Build, finalise and broadcast the transaction that fills `offer` via the
    /// covenant's SETTLE path. Returns the settlement txid.
    fn take_offer(&self, offer: &Offer) -> Result<String>;
}

/// Network fee for the settlement transaction (satoshis).
pub const SETTLE_FEE_SATS: u64 = 1_000;

/// A taker that fills offers against an Elements node.
pub struct MosaikTaker {
    rpc: rpc::ElementsRpc,
}

impl MosaikTaker {
    pub fn new(rpc: rpc::ElementsRpc) -> Self {
        Self { rpc }
    }

    /// A taker wired to the local Mosaik regtest (see `scripts/regtest.sh`).
    pub fn regtest() -> Self {
        Self::new(rpc::ElementsRpc::regtest_wallet())
    }
}

/// The BIP-341 NUMS internal key the Tessera covenant uses (no key-path spend).
pub const NUMS_INTERNAL_KEY: &str =
    "50929b74c1a04954b78b4b6035e97a5e078a5a0f28ec96d547bfee9ace803ac0";

/// A deliberately-broken settlement, used to demonstrate that the covenant
/// rejects bad fills. [`Cheat::None`] is an honest fill.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cheat {
    /// Honest fill — pays the maker exactly.
    None,
    /// Pay the maker one unit short of `amount_b` (covenant: amount check).
    Underpay,
    /// Send the maker's payment to the taker instead (covenant: script check).
    WrongRecipient,
    /// Put the maker's output at index 1, not 0 (covenant checks index 0).
    WrongIndex,
}

impl Cheat {
    /// Parse a UI/CLI label into a [`Cheat`] mode.
    pub fn parse(label: &str) -> Self {
        match label {
            "underpay" => Cheat::Underpay,
            "wrong_recipient" => Cheat::WrongRecipient,
            "wrong_index" => Cheat::WrongIndex,
            _ => Cheat::None,
        }
    }
}

impl TakeOffer for MosaikTaker {
    /// Fill an offer honestly via the covenant's SETTLE path.
    fn take_offer(&self, offer: &Offer) -> Result<String> {
        self.settle(offer, Cheat::None)
    }
}

impl MosaikTaker {
    /// Build, finalise and broadcast a settlement for `offer`.
    ///
    /// With [`Cheat::None`] this is an honest fill: the covenant UTXO (asset A)
    /// plus a taker input of asset B; the maker's counter-payment in asset B as
    /// output 0 (what the covenant checks), asset A to the taker, change, and a
    /// fee. The covenant input's Simplicity witness is assembled with
    /// `hal-simplicity`; the taker input is signed by the wallet.
    ///
    /// With any other [`Cheat`] the settlement is deliberately broken — the
    /// transaction is still well-formed and balanced, so the *node* would
    /// accept it, but the *covenant* must reject it. A non-`None` cheat that
    /// succeeds in broadcasting is a covenant bug.
    pub fn settle(&self, offer: &Offer, cheat: Cheat) -> Result<String> {
        use std::collections::BTreeMap;

        let (txid, vout_str) = offer
            .outpoint
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("bad outpoint: {}", offer.outpoint))?;
        let covenant_vout: u64 = vout_str.parse()?;

        let amount_a = offer.amount_a;
        let amount_b = offer.tessera.amount_b;
        let fee = SETTLE_FEE_SATS;

        let lbtc = self.rpc.policy_asset()?;
        // `asset_a` is stored on the offer in RPC display order already.
        let asset_a = offer.asset_a.clone();
        // `asset_b` lives in tx/jet (internal) order in the covenant; the RPC
        // wants display order, which is the reverse.
        let mut asset_b = offer.tessera.asset_b.to_vec();
        asset_b.reverse();
        let asset_b = hex::encode(asset_b);
        if asset_a == asset_b {
            anyhow::bail!("asset_a and asset_b must differ");
        }

        // The taker must hold an asset-B input covering the maker payment.
        let (b_txid, b_vout, b_amount) = self
            .rpc
            .unspent_of_asset(&asset_b, amount_b)?
            .ok_or_else(|| {
                anyhow::anyhow!("taker has no {asset_b} UTXO of at least {amount_b}")
            })?;

        // Per-asset inflow ledger: the covenant input (asset A) and the taker's
        // asset-B input. The settlement balances each asset exactly.
        let mut inflow: BTreeMap<String, u64> = BTreeMap::new();
        *inflow.entry(asset_a.clone()).or_default() += amount_a;
        *inflow.entry(asset_b.clone()).or_default() += b_amount;

        let mut inputs = vec![
            serde_json::json!({ "txid": txid, "vout": covenant_vout }),
            serde_json::json!({ "txid": b_txid, "vout": b_vout }),
        ];

        // The network fee is always L-BTC. If neither leg of the swap is L-BTC,
        // the taker adds a small L-BTC input to cover it.
        let lbtc_in = inflow.get(&lbtc).copied().unwrap_or(0);
        if lbtc_in < fee {
            let (l_txid, l_vout, l_amount) = self
                .rpc
                .unspent_of_asset(&lbtc, fee - lbtc_in)?
                .ok_or_else(|| anyhow::anyhow!("taker has no L-BTC UTXO for the {fee}-sat fee"))?;
            inputs.push(serde_json::json!({ "txid": l_txid, "vout": l_vout }));
            *inflow.entry(lbtc.clone()).or_default() += l_amount;
        }

        let taker = self.rpc.new_unconfidential_address()?;
        let btc = |s: u64| s as f64 / 1e8;

        // Output 0 pays the maker — the only output the covenant checks. A
        // cheat under- or mis-pays it; the transaction still balances per asset,
        // so only the covenant can catch the fraud.
        let maker_pay = if cheat == Cheat::Underpay {
            amount_b.saturating_sub(1)
        } else {
            amount_b
        };
        // WrongRecipient pays a fresh address (not the maker's, and distinct
        // from the taker's change address so the RPC does not reject it).
        let cheat_addr = if cheat == Cheat::WrongRecipient {
            self.rpc.new_unconfidential_address()?
        } else {
            String::new()
        };
        let maker_recipient: &str = if cheat == Cheat::WrongRecipient {
            &cheat_addr
        } else {
            &offer.maker_address
        };

        let mut outputs =
            vec![serde_json::json!({ maker_recipient: btc(maker_pay), "asset": asset_b })];
        for (asset, total) in &inflow {
            let mut to_taker = *total;
            if asset == &asset_b {
                to_taker = to_taker.saturating_sub(maker_pay);
            }
            if asset == &lbtc {
                to_taker = to_taker.saturating_sub(fee);
            }
            if to_taker > 0 {
                outputs.push(serde_json::json!({ &taker: btc(to_taker), "asset": asset }));
            }
        }
        outputs.push(serde_json::json!({ "fee": btc(fee) }));

        // WrongIndex: shift the maker's payment off output 0 — the covenant
        // re-checks index 0 and sees the taker's output instead.
        if cheat == Cheat::WrongIndex && outputs.len() >= 2 {
            outputs.swap(0, 1);
        }

        let inputs = serde_json::Value::Array(inputs);
        let raw_hex = self
            .rpc
            .call("createrawtransaction", serde_json::json!([inputs, outputs]))?
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("createrawtransaction: no hex"))?
            .to_string();

        // To a PSET, then the wallet signs its own (taker) input.
        let pset = self
            .rpc
            .call("converttopsbt", serde_json::json!([raw_hex]))?
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("converttopsbt: no pset"))?
            .to_string();
        let pset = self.rpc.wallet_process_psbt(&pset)?;

        // Attach the covenant program + SETTLE witness to input 0 (the covenant).
        let compiled = offer.tessera.compile()?;
        let covenant_spk = hex::encode(compiled.address()?.script_pubkey().as_bytes());
        let input_utxo = format!("{covenant_spk}:{asset_a}:{}", btc(amount_a));
        let pset = hal_pset(&[
            "simplicity", "pset", "update-input", "-r", &pset, "0",
            "-i", &input_utxo, "-c", &compiled.cmr_hex(), "-p", NUMS_INTERNAL_KEY,
        ])?;

        let w = offer.tessera.settle_witness(0)?;
        let b64 = |bytes: &[u8]| {
            base64::engine::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes)
        };
        let pset = hal_pset(&[
            "simplicity", "pset", "finalize", "-r", &pset, "0",
            &b64(&w.program), &b64(&w.witness),
        ])?;
        let raw_tx = hal_run(&["simplicity", "pset", "extract", "-r", &pset])?;
        let raw_tx = raw_tx.trim().trim_matches('"').to_string();

        // Broadcast — the node executes and enforces the covenant.
        if std::env::var("MOSAIK_DEBUG_TX").is_ok() {
            eprintln!("RAWTX {raw_tx}");
        }
        self.rpc.send_raw_transaction(&raw_tx)
    }
}

/// Run `hal-simplicity` with `args`, returning trimmed stdout.
pub fn hal_run(args: &[&str]) -> Result<String> {
    let bin = std::env::var("HAL_SIMPLICITY").unwrap_or_else(|_| "hal-simplicity".into());
    let out = std::process::Command::new(&bin)
        .args(args)
        .output()
        .map_err(|e| anyhow::anyhow!("running {bin}: {e}"))?;
    if !out.status.success() {
        anyhow::bail!(
            "hal-simplicity {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Run a `hal-simplicity` PSET command and return the `pset` field of its JSON.
pub fn hal_pset(args: &[&str]) -> Result<String> {
    let output = hal_run(args)?;
    let v: serde_json::Value = serde_json::from_str(&output)
        .map_err(|e| anyhow::anyhow!("hal-simplicity output not JSON: {e}\n{output}"))?;
    if let Some(err) = v.get("error").filter(|e| !e.is_null()) {
        anyhow::bail!("hal-simplicity error: {err}");
    }
    Ok(v.get("pset")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("hal-simplicity: no pset in output"))?
        .to_string())
}

/// Maker side — reclaim an unfilled offer after its timeout.
pub trait ReclaimOffer {
    /// Spend the covenant UTXO back to the maker via the keyless REFUND path.
    /// No key needed — the covenant only requires the sweep to return the
    /// locked asset to the maker. Returns the reclaim txid.
    fn reclaim(&self, offer: &Offer) -> Result<String>;
}

impl ReclaimOffer for MosaikMaker {
    /// Build, finalise and broadcast the keyless REFUND transaction.
    ///
    /// REFUND sweeps the covenant UTXO back to the maker once the chain is at
    /// or past `tessera.timeout`. The transaction sets `nLockTime = timeout`
    /// and pays the locked L-BTC (less the fee) to the maker. No signature —
    /// the covenant enforces the destination and amount itself. L-BTC-locked
    /// offers only.
    fn reclaim(&self, offer: &Offer) -> Result<String> {
        let lbtc = self.rpc.policy_asset()?;
        if offer.asset_a != lbtc {
            anyhow::bail!(
                "reclaim currently supports L-BTC-locked offers; this offer locks {}",
                offer.asset_a
            );
        }

        let (txid, vout_str) = offer
            .outpoint
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("bad outpoint: {}", offer.outpoint))?;
        let covenant_vout: u64 = vout_str.parse()?;

        let amount_a = offer.amount_a;
        let fee = SETTLE_FEE_SATS;
        if amount_a <= fee {
            anyhow::bail!("offer locks too little to cover the fee");
        }
        let timeout = offer.tessera.timeout;

        // Surface a clear error before the covenant would reject the spend.
        let height = self.rpc.block_count()?;
        if (height as u32) < timeout {
            anyhow::bail!("refund is locked until height {timeout}; chain is at {height}");
        }

        let btc = |s: u64| s as f64 / 1e8;

        // The covenant input's sequence must enable `nLockTime`.
        let inputs = serde_json::json!([
            { "txid": txid, "vout": covenant_vout, "sequence": 4_294_967_294u64 }
        ]);
        let outputs = serde_json::json!([
            { &offer.maker_address: btc(amount_a - fee), "asset": lbtc },
            { "fee": btc(fee) },
        ]);
        let raw_hex = self
            .rpc
            .call("createrawtransaction", serde_json::json!([inputs, outputs, timeout]))?
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("createrawtransaction: no hex"))?
            .to_string();

        let compiled = offer.tessera.compile()?;
        let covenant_spk_bytes = compiled.address()?.script_pubkey().as_bytes().to_vec();

        // Keyless: prune the covenant against this exact transaction and drop
        // the witness onto the single covenant input — no signing, no PSET.
        // The maker output is at index 0.
        let raw_tx = offer.tessera.build_refund_tx(
            &raw_hex,
            0,
            0,
            &covenant_spk_bytes,
            &lbtc,
            amount_a,
        )?;

        if std::env::var("MOSAIK_DEBUG_TX").is_ok() {
            eprintln!("RECLAIM raw_hex (unsigned): {raw_hex}");
            eprintln!("RECLAIM raw_tx  (final):    {raw_tx}");
        }

        self.rpc.send_raw_transaction(&raw_tx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offer_roundtrips_json() {
        let offer = Offer {
            outpoint: "0000000000000000000000000000000000000000000000000000000000000001:0"
                .into(),
            asset_a: "aa".repeat(32),
            amount_a: 100_000,
            tessera: Tessera {
                asset_b: [0x11; 32],
                amount_b: 50_000,
                maker_spk_hash: [0x22; 32],
                timeout: 200,
            },
            covenant_address: "ert1pexampleexampleexampleexampleexampleex".into(),
            maker_address: "ert1qexampleexampleexampleexampleexampleex".into(),
        };
        let json = serde_json::to_string(&offer).unwrap();
        let back: Offer = serde_json::from_str(&json).unwrap();
        assert_eq!(back.amount_a, 100_000);
        assert_eq!(back.tessera.amount_b, 50_000);
        assert_eq!(back.maker_address, offer.maker_address);
    }
}

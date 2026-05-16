//! Mosaik core: offers, and the Liquid PSET / key plumbing.
//!
//! This crate turns a [`tessera::Tessera`] into a live offer (a funded covenant
//! UTXO) and builds the transactions that fill or reclaim it.
//!
//! The Elements/LWK calls are TODOs at this stage — the types and the flow are
//! defined so the hackathon work is pure fill-in. See `docs/DESIGN.md` §5.

use anyhow::Result;
use serde::{Deserialize, Serialize};

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

impl MakeOffer for MosaikMaker {
    /// Compile the Tessera, derive its covenant address, fund it on-chain, and
    /// return the published [`Offer`].
    ///
    /// Funds L-BTC offers (`asset_a` = "BTC"); the maker locks `amount_a`
    /// satoshis into the covenant UTXO. Funding any P2TR address is a normal
    /// payment, so this works on stock `elementsd` — only *spending* the
    /// covenant needs a Simplicity-capable node.
    fn make_offer(
        &self,
        asset_a: &str,
        amount_a: u64,
        tessera: &Tessera,
        maker_address: &str,
    ) -> Result<Offer> {
        if !matches!(asset_a.to_ascii_uppercase().as_str(), "BTC" | "LBTC" | "L-BTC") {
            anyhow::bail!("make_offer funds L-BTC offers only; got asset_a={asset_a}");
        }

        // Compile the covenant and derive its Taproot address.
        let compiled = tessera.compile()?;
        let address = compiled.address()?;
        let spk_hex = hex::encode(address.script_pubkey().as_bytes());

        // Fund the covenant UTXO and confirm it.
        let amount_btc = amount_a as f64 / 1e8;
        let txid = self.rpc.send_to_address(&address.to_string(), amount_btc)?;
        self.rpc.generate(1)?;

        // Locate the funding output among the transaction's vouts.
        let tx = self.rpc.raw_transaction(&txid)?;
        let vout = find_output_index(&tx, &spk_hex)
            .ok_or_else(|| anyhow::anyhow!("funding output for {txid} not found"))?;

        Ok(Offer {
            outpoint: format!("{txid}:{vout}"),
            asset_a: asset_a.to_string(),
            amount_a,
            tessera: tessera.clone(),
            maker_address: maker_address.to_string(),
        })
    }
}

/// Find the index of the output whose scriptPubKey hex matches `spk_hex`.
fn find_output_index(tx: &serde_json::Value, spk_hex: &str) -> Option<u64> {
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
const SETTLE_FEE_SATS: u64 = 1_000;

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
const NUMS_INTERNAL_KEY: &str =
    "50929b74c1a04954b78b4b6035e97a5e078a5a0f28ec96d547bfee9ace803ac0";

impl TakeOffer for MosaikTaker {
    /// Fill an offer: spend the covenant UTXO via SETTLE, paying the maker.
    ///
    /// Builds a single transaction — covenant UTXO in; the maker's
    /// counter-payment as output 0 (what the covenant checks), the remainder
    /// to the taker, and a fee. The covenant input's Simplicity witness is
    /// assembled with `hal-simplicity` (the PSET tool): the node executes the
    /// covenant and the spend is only accepted if the maker is paid exactly.
    ///
    /// This is the L-BTC settlement — the offer locks L-BTC and the maker is
    /// paid L-BTC, so no taker inputs are needed.
    fn take_offer(&self, offer: &Offer) -> Result<String> {
        let (txid, vout_str) = offer
            .outpoint
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("bad outpoint: {}", offer.outpoint))?;
        let covenant_vout: u64 = vout_str.parse()?;

        let amount_b = offer.tessera.amount_b;
        if offer.amount_a <= amount_b + SETTLE_FEE_SATS {
            anyhow::bail!(
                "offer locks {} sats — too little for a {} payment + {} fee",
                offer.amount_a,
                amount_b,
                SETTLE_FEE_SATS
            );
        }
        let taker_amount = offer.amount_a - amount_b - SETTLE_FEE_SATS;
        let taker_address = self.rpc.new_unconfidential_address()?;
        let btc = |s: u64| format!("{:.8}", s as f64 / 1e8);

        // 1. Skeleton transaction: output 0 = maker (the SETTLE target),
        //    1 = taker, 2 = fee. The node handles asset ids and the fee output.
        let inputs = serde_json::json!([{ "txid": txid, "vout": covenant_vout }]);
        let outputs = serde_json::json!([
            { &offer.maker_address: btc(amount_b).parse::<f64>().unwrap() },
            { taker_address: btc(taker_amount).parse::<f64>().unwrap() },
            { "fee": btc(SETTLE_FEE_SATS).parse::<f64>().unwrap() },
        ]);
        let raw_hex = self
            .rpc
            .call("createrawtransaction", serde_json::json!([inputs, outputs]))?
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("createrawtransaction: no hex"))?
            .to_string();

        // 2. Convert to a PSET.
        let pset = self
            .rpc
            .call("converttopsbt", serde_json::json!([raw_hex]))?
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("converttopsbt: no pset"))?
            .to_string();

        // 3. Covenant data for the input.
        let compiled = offer.tessera.compile()?;
        let covenant_spk = hex::encode(compiled.address()?.script_pubkey().as_bytes());
        let lbtc = self.rpc.policy_asset()?;
        let input_utxo = format!("{covenant_spk}:{lbtc}:{}", btc(offer.amount_a));

        // 4. Attach the covenant UTXO data to PSET input 0.
        let pset = hal_pset(&[
            "simplicity", "pset", "update-input", "-r", &pset, "0",
            "-i", &input_utxo, "-c", &compiled.cmr_hex(), "-p", NUMS_INTERNAL_KEY,
        ])?;

        // 5. Attach the Simplicity program + SETTLE witness, then extract the tx.
        //    Both are standard base64, matching what `simc --json` emits.
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

        // 6. Broadcast — the node executes and enforces the covenant.
        self.rpc.send_raw_transaction(&raw_tx)
    }
}

/// Run `hal-simplicity` with `args`, returning trimmed stdout.
fn hal_run(args: &[&str]) -> Result<String> {
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
fn hal_pset(args: &[&str]) -> Result<String> {
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
    /// Spend the covenant UTXO back to the maker via the REFUND path.
    ///
    /// TODO(hackathon): build a tx spending the covenant UTXO, set the
    /// Simplicity witness to the REFUND path with the maker's BIP-340
    /// signature, set `nLockTime = tessera.timeout`, broadcast.
    fn reclaim(&self, offer: &Offer) -> Result<String>;
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
                maker_pk: [0x33; 32],
            },
            maker_address: "ert1qexampleexampleexampleexampleexampleex".into(),
        };
        let json = serde_json::to_string(&offer).unwrap();
        let back: Offer = serde_json::from_str(&json).unwrap();
        assert_eq!(back.amount_a, 100_000);
        assert_eq!(back.tessera.amount_b, 50_000);
        assert_eq!(back.maker_address, offer.maker_address);
    }
}

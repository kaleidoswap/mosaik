//! LWK-based backend for Liquid testnet — no local elementsd required.

use anyhow::{anyhow, Context, Result};
use lwk_common::Signer;
use lwk_signer::SwSigner;
use lwk_wollet::{
    clients::blocking::{BlockchainBackend, EsploraClient},
    elements::{
        confidential::{self, AssetBlindingFactor, ValueBlindingFactor},
        encode::serialize,
        pset::PartiallySignedTransaction,
        AssetId, OutPoint, Script, Txid, TxOutWitness,
    },
    ElementsNetwork, ExternalUtxo, UnvalidatedRecipient, Wollet, WolletBuilder, WolletDescriptor,
};
use serde_json::{json, Value};
use std::str::FromStr;
use std::sync::{Arc, Mutex};

const TESTNET_LBTC: &str = "144c654344aa716d6f3abcc1ca90e5641e4e2a7f633bc09fe3baf64585819a49";
const TESTNET_GENESIS: &str = "9f87eb580b9e5f14dc794e4c723c5348b4e58c65e0bf5d2d74a6e5a1d991dd48";
const ESPLORA_URL: &str = "https://blockstream.info/liquidtestnet/api";

const TREASURY_MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
const MAKER_MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon acid";
const TAKER_MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon ads";

// ── LwkWallet ────────────────────────────────────────────────────────────────

pub struct LwkWallet {
    pub name: String,
    pub signer: SwSigner,
    pub wollet: Arc<Mutex<Wollet>>,
    pub descriptor: String,
}

impl LwkWallet {
    pub fn new(name: &str, mnemonic: &str, network: ElementsNetwork) -> Result<Self> {
        let is_mainnet = network == ElementsNetwork::Liquid;
        let signer = SwSigner::new(mnemonic, is_mainnet)
            .map_err(|e| anyhow!("failed to create signer: {e}"))?;
        let descriptor = signer
            .wpkh_slip77_descriptor()
            .map_err(|e| anyhow!("failed to derive descriptor: {e}"))?;
        let wd: WolletDescriptor = descriptor
            .parse()
            .map_err(|e: lwk_wollet::Error| anyhow!("invalid descriptor: {e}"))?;
        let wollet = WolletBuilder::new(network, wd)
            .build()
            .map_err(|e| anyhow!("failed to build wollet: {e}"))?;
        Ok(Self {
            name: name.to_string(), signer, wollet: Arc::new(Mutex::new(wollet)), descriptor,
        })
    }

    pub fn address(&self) -> Result<String> {
        let w = self.wollet.lock().unwrap();
        let addr = w.address(None).map_err(|e| anyhow!("address: {e}"))?;
        Ok(addr.address().to_string())
    }

    pub fn unconfidential_address(&self) -> Result<String> {
        let w = self.wollet.lock().unwrap();
        let addr = w.address(None).map_err(|e| anyhow!("address: {e}"))?;
        let unconf = addr.address().to_unconfidential();
        Ok(unconf.to_string())
    }

    pub fn balances(&self) -> Result<Value> {
        let w = self.wollet.lock().unwrap();
        let bal = w.balance().map_err(|e| anyhow!("balance: {e}"))?;
        let mut map = serde_json::Map::new();
        for (asset_id, sats) in bal.iter() {
            map.insert(asset_id.to_string(), json!(*sats as f64 / 1e8));
        }
        Ok(Value::Object(map))
    }

    pub fn list_unspent(&self) -> Result<Value> {
        let w = self.wollet.lock().unwrap();
        let utxos = w.utxos().map_err(|e| anyhow!("utxos: {e}"))?;
        let arr: Vec<Value> = utxos.into_iter().map(|u| {
            json!({
                "txid": u.outpoint.txid.to_string(),
                "vout": u.outpoint.vout,
                "asset": u.unblinded.asset.to_string(),
                "amount": u.unblinded.value as f64 / 1e8,
                "address": u.address.to_string(),
            })
        }).collect();
        Ok(Value::Array(arr))
    }

    pub fn unspent_of_asset(&self, asset: &str, min: u64) -> Result<Option<(String, u64, u64)>> {
        let w = self.wollet.lock().unwrap();
        let utxos = w.utxos().map_err(|e| anyhow!("utxos: {e}"))?;
        let asset_id = AssetId::from_str(asset).map_err(|e| anyhow!("bad asset: {e}"))?;
        Ok(utxos.into_iter().find_map(|u| {
            if u.unblinded.asset != asset_id { return None; }
            if u.unblinded.value < min { return None; }
            Some((u.outpoint.txid.to_string(), u.outpoint.vout as u64, u.unblinded.value))
        }))
    }

    /// Build, sign, finalize and broadcast a payment.
    pub fn send_to_address(&self, address: &str, amount_btc: f64, asset: Option<&str>) -> Result<String> {
        let sats = (amount_btc * 1e8).round() as u64;
        let recipient = UnvalidatedRecipient {
            satoshi: sats,
            address: address.to_string(),
            asset: asset.unwrap_or(TESTNET_LBTC).to_string(),
        };
        let w = self.wollet.lock().unwrap();
        let mut pset = w.tx_builder()
            .add_unvalidated_recipient(&recipient).map_err(|e| anyhow!("tx builder: {e}"))?
            .finish().map_err(|e| anyhow!("finish: {e}"))?;
        drop(w);
        self.signer.sign(&mut pset).map_err(|e| anyhow!("sign: {e}"))?;
        let w = self.wollet.lock().unwrap();
        w.finalize(&mut pset).map_err(|e| anyhow!("finalize: {e}"))?;
        let tx = pset.extract_tx().map_err(|e| anyhow!("extract: {e}"))?;
        let client = EsploraClient::new(ESPLORA_URL, ElementsNetwork::LiquidTestnet)
            .map_err(|e| anyhow!("esplora: {e}"))?;
        let txid = client.broadcast(&tx).map_err(|e| anyhow!("broadcast: {e}"))?;
        Ok(txid.to_string())
    }

    /// Issue a new asset; returns `(asset_id, token_id, txid)`.
    pub fn issue_asset(&self, asset_sats: u64, token_sats: u64,
                       contract: &lwk_wollet::Contract) -> Result<(String, String, String)> {
        let w = self.wollet.lock().unwrap();
        let mut pset = w.tx_builder()
            .issue_asset(asset_sats, None, token_sats, None, Some(contract.clone()))
            .map_err(|e| anyhow!("issue_asset: {e}"))?
            .finish().map_err(|e| anyhow!("finish: {e}"))?;
        drop(w);
        self.signer.sign(&mut pset).map_err(|e| anyhow!("sign: {e}"))?;
        let w = self.wollet.lock().unwrap();
        w.finalize(&mut pset).map_err(|e| anyhow!("finalize: {e}"))?;
        let asset_id = pset.inputs()[0].issuance_ids().0.to_string();
        let token_id = pset.inputs()[0].issuance_ids().1.to_string();
        let tx = pset.extract_tx().map_err(|e| anyhow!("extract: {e}"))?;
        let client = EsploraClient::new(ESPLORA_URL, ElementsNetwork::LiquidTestnet)
            .map_err(|e| anyhow!("esplora: {e}"))?;
        let txid = client.broadcast(&tx).map_err(|e| anyhow!("broadcast: {e}"))?;
        Ok((asset_id, token_id, txid.to_string()))
    }

    pub fn sign_pset(&self, pset_b64: &str) -> Result<String> {
        let mut pset: PartiallySignedTransaction = pset_b64.parse()
            .map_err(|e| anyhow!("pset parse: {e}"))?;
        self.signer.sign(&mut pset).map_err(|e| anyhow!("sign: {e}"))?;
        Ok(pset.to_string())
    }
}

// ── LwkNode ──────────────────────────────────────────────────────────────────

pub struct LwkNode {
    pub network: ElementsNetwork,
    pub treasury: LwkWallet,
    pub maker: LwkWallet,
    pub taker: LwkWallet,
}

impl LwkNode {
    pub fn testnet() -> Result<Self> {
        let network = ElementsNetwork::LiquidTestnet;
        Ok(Self {
            network,
            treasury: LwkWallet::new("treasury", TREASURY_MNEMONIC, network)?,
            maker: LwkWallet::new("maker", MAKER_MNEMONIC, network)?,
            taker: LwkWallet::new("taker", TAKER_MNEMONIC, network)?,
        })
    }

    pub fn sync(&self) -> Result<()> {
        let mut client = EsploraClient::new(ESPLORA_URL, self.network)
            .map_err(|e| anyhow!("esplora: {e}"))?;
        for wallet in [&self.treasury, &self.maker, &self.taker] {
            let mut w = wallet.wollet.lock().unwrap();
            if let Some(update) = client.full_scan(&*w).map_err(|e| anyhow!("scan: {e}"))? {
                w.apply_update(update).map_err(|e| anyhow!("apply update: {e}"))?;
            }
        }
        Ok(())
    }

    pub fn block_count(&self) -> Result<u64> {
        let url = format!("{ESPLORA_URL}/blocks/tip/height");
        let resp = ureq::get(&url).call()?;
        let text = resp.into_string()?;
        text.trim().parse().context("block_count parse")
    }

    pub fn policy_asset(&self) -> Result<String> { Ok(TESTNET_LBTC.to_string()) }
    pub fn genesis_hash(&self) -> Result<String> { Ok(TESTNET_GENESIS.to_string()) }

    pub fn send_raw_transaction(&self, tx_hex: &str) -> Result<String> {
        let url = format!("{ESPLORA_URL}/tx");
        let resp = ureq::post(&url).send_string(tx_hex)?;
        Ok(resp.into_string()?.trim().to_string())
    }

    pub fn raw_transaction(&self, txid: &str) -> Result<Value> {
        let url = format!("{ESPLORA_URL}/tx/{txid}");
        let resp = ureq::get(&url).call()?;
        Ok(resp.into_json()?)
    }
}

// ── LwkMaker / LwkTaker ─────────────────────────────────────────────────────

use crate::{MakeOffer, Offer, ReclaimOffer, TakeOffer, Tessera, Cheat, SETTLE_FEE_SATS};
use simplicityhl::elements::AddressParams;

pub struct LwkMaker<'a> { pub wallet: &'a LwkWallet, pub node: &'a LwkNode }
impl<'a> LwkMaker<'a> { pub fn new(wallet: &'a LwkWallet, node: &'a LwkNode) -> Self { Self { wallet, node } } }

impl<'a> MakeOffer for LwkMaker<'a> {
    fn make_offer(&self, asset_a: &str, amount_a: u64, tessera: &Tessera, maker_address: &str) -> Result<Offer> {
        let lbtc = self.node.policy_asset()?;
        let asset_a_id = if asset_a == "BTC" || asset_a == "LBTC" || asset_a == "L-BTC" { lbtc.clone() } else { asset_a.to_string() };
        let compiled = tessera.compile()?;
        let address = compiled.address_for(&AddressParams::LIQUID_TESTNET)?;
        let spk_hex = hex::encode(address.script_pubkey().as_bytes());
        let amount = amount_a as f64 / 1e8;
        let txid = if asset_a_id == lbtc {
            self.wallet.send_to_address(&address.to_string(), amount, None)?
        } else {
            self.wallet.send_to_address(&address.to_string(), amount, Some(&asset_a_id))?
        };
        let tx = self.node.raw_transaction(&txid)?;
        let vout = find_output_index(&tx, &spk_hex)
            .ok_or_else(|| anyhow!("funding output for {txid} not found"))?;
        Ok(Offer { outpoint: format!("{txid}:{vout}"), asset_a: asset_a_id, amount_a, tessera: tessera.clone(), maker_address: maker_address.to_string() })
    }
}

impl<'a> ReclaimOffer for LwkMaker<'a> {
    fn reclaim(&self, offer: &Offer, maker_secret: &[u8; 32]) -> Result<String> {
        let lbtc = self.node.policy_asset()?;
        if offer.asset_a != lbtc { anyhow::bail!("reclaim supports L-BTC only; offer locks {}", offer.asset_a); }
        let (txid, vout_str) = offer.outpoint.split_once(':').ok_or_else(|| anyhow!("bad outpoint"))?;
        let covenant_vout: u64 = vout_str.parse()?;
        let amount_a = offer.amount_a;
        let fee = SETTLE_FEE_SATS;
        if amount_a <= fee { anyhow::bail!("offer locks too little"); }
        let timeout = offer.tessera.timeout;
        let height = self.node.block_count()?;
        if (height as u32) < timeout { anyhow::bail!("refund locked until {timeout}; chain at {height}"); }

        use lwk_wollet::elements::{Transaction, TxIn, TxOut, TxInWitness, TxOutWitness,
            confidential, LockTime, Sequence};
        let asset_id = AssetId::from_str(&lbtc)?;
        let maker_addr: lwk_wollet::elements::Address = offer.maker_address.parse()?;
        let tx = Transaction {
            version: 2,
            lock_time: LockTime::from_consensus(timeout),
            input: vec![TxIn {
                previous_output: OutPoint::new(Txid::from_str(txid)?, covenant_vout as u32),
                script_sig: Script::new(),
                sequence: Sequence(4_294_967_294),
                is_pegin: false,
                asset_issuance: Default::default(),
                witness: TxInWitness::default(),
            }],
            output: vec![
                TxOut { asset: confidential::Asset::Explicit(asset_id), value: confidential::Value::Explicit(amount_a - fee), nonce: confidential::Nonce::Null, script_pubkey: maker_addr.script_pubkey(), witness: TxOutWitness::default() },
                TxOut { asset: confidential::Asset::Explicit(asset_id), value: confidential::Value::Explicit(fee), nonce: confidential::Nonce::Null, script_pubkey: Script::new(), witness: TxOutWitness::default() },
            ],
        };
        let raw_hex = hex::encode(serialize(&tx));
        let compiled = offer.tessera.compile()?;
        let covenant_spk_bytes = compiled.address()?.script_pubkey().as_bytes().to_vec();
        let genesis = self.node.genesis_hash()?;
        let raw_tx = offer.tessera.build_refund_tx(&raw_hex, 0, &covenant_spk_bytes, &lbtc, amount_a, &genesis, maker_secret)?;
        self.node.send_raw_transaction(&raw_tx)
    }
}

pub struct LwkTaker<'a> { pub wallet: &'a LwkWallet, pub node: &'a LwkNode }
impl<'a> LwkTaker<'a> { pub fn new(wallet: &'a LwkWallet, node: &'a LwkNode) -> Self { Self { wallet, node } } }

impl<'a> TakeOffer for LwkTaker<'a> {
    fn take_offer(&self, offer: &Offer) -> Result<String> { self.settle(offer, Cheat::None) }
}

impl<'a> LwkTaker<'a> {
    pub fn settle(&self, offer: &Offer, cheat: Cheat) -> Result<String> {
        let (txid, vout_str) = offer.outpoint.split_once(':').ok_or_else(|| anyhow!("bad outpoint"))?;
        let covenant_vout: u64 = vout_str.parse()?;
        let amount_a = offer.amount_a;
        let amount_b = offer.tessera.amount_b;
        let fee = SETTLE_FEE_SATS;
        let lbtc = self.node.policy_asset()?;
        let asset_a = offer.asset_a.clone();
        let mut asset_b = offer.tessera.asset_b.to_vec(); asset_b.reverse();
        let asset_b = hex::encode(asset_b);
        if asset_a == asset_b { anyhow::bail!("asset_a and asset_b must differ"); }

        let (b_txid, b_vout, b_amount) = self.wallet.unspent_of_asset(&asset_b, amount_b)?
            .ok_or_else(|| anyhow!("taker has no {asset_b} UTXO of at least {amount_b}"))?;

        let lbtc_in = if asset_a == lbtc { amount_a } else { 0 } + if asset_b == lbtc { b_amount } else { 0 };
        let mut wallet_utxos = vec![OutPoint::new(Txid::from_str(&b_txid)?, b_vout as u32)];
        let mut lbtc_utxo = None;
        if lbtc_in < fee || (asset_a != lbtc && asset_b != lbtc) {
            let need = if lbtc_in < fee { fee - lbtc_in } else { fee };
            let (l_txid, l_vout, _) = self.wallet.unspent_of_asset(&lbtc, need)?
                .ok_or_else(|| anyhow!("taker has no L-BTC for fee"))?;
            wallet_utxos.push(OutPoint::new(Txid::from_str(&l_txid)?, l_vout as u32));
            lbtc_utxo = Some((l_txid, l_vout));
        }

        let taker = self.wallet.unconfidential_address()?;
        let maker_pay = if cheat == Cheat::Underpay { amount_b.saturating_sub(1) } else { amount_b };
        let cheat_addr = if cheat == Cheat::WrongRecipient { self.wallet.unconfidential_address()? } else { String::new() };
        let maker_recipient: &str = if cheat == Cheat::WrongRecipient { &cheat_addr } else { &offer.maker_address };

        let asset_b_id = AssetId::from_str(&asset_b)?;
        let asset_a_id = AssetId::from_str(&asset_a)?;
        let lbtc_id = AssetId::from_str(&lbtc)?;

        let compiled = offer.tessera.compile()?;
        let covenant_spk = compiled.address()?.script_pubkey();
        let covenant_utxo = ExternalUtxo {
            outpoint: OutPoint::new(Txid::from_str(txid)?, covenant_vout as u32),
            txout: lwk_wollet::elements::TxOut {
                asset: confidential::Asset::Explicit(asset_a_id),
                value: confidential::Value::Explicit(amount_a),
                nonce: confidential::Nonce::Null,
                script_pubkey: covenant_spk.clone(),
                witness: TxOutWitness::default(),
            },
            tx: None,
            unblinded: lwk_wollet::elements::TxOutSecrets {
                asset: asset_a_id, asset_bf: AssetBlindingFactor::zero(),
                value: amount_a, value_bf: ValueBlindingFactor::zero(),
            },
            max_weight_to_satisfy: 0,
        };

        let w = self.wallet.wollet.lock().unwrap();
        let mut builder = w.tx_builder()
            .add_external_utxos(vec![covenant_utxo]).map_err(|e| anyhow!("external: {e}"))?
            .set_wallet_utxos(wallet_utxos);

        let maker_addr: lwk_wollet::elements::Address = maker_recipient.parse()?;
        builder = builder.add_recipient(&maker_addr, maker_pay, asset_b_id)
            .map_err(|e| anyhow!("maker recipient: {e}"))?;

        let taker_addr: lwk_wollet::elements::Address = taker.parse()?;
        builder = builder.add_recipient(&taker_addr, amount_a, asset_a_id)
            .map_err(|e| anyhow!("taker a: {e}"))?;

        let b_change = b_amount.saturating_sub(maker_pay);
        if b_change > 0 {
            builder = builder.add_recipient(&taker_addr, b_change, asset_b_id)
                .map_err(|e| anyhow!("taker b: {e}"))?;
        }
        if let Some((_, _)) = lbtc_utxo {
            let lbtc_change = lbtc_in.saturating_sub(fee);
            if lbtc_change > 0 {
                builder = builder.add_recipient(&taker_addr, lbtc_change, lbtc_id)
                    .map_err(|e| anyhow!("taker lbtc: {e}"))?;
            }
        }

        let mut pset = builder.finish().map_err(|e| anyhow!("finish: {e}"))?;
        drop(w);

        self.wallet.signer.sign(&mut pset).map_err(|e| anyhow!("sign: {e}"))?;

        let covenant_spk_hex = hex::encode(covenant_spk.as_bytes());
        let input_utxo = format!("{covenant_spk_hex}:{asset_a}:{}" , amount_a as f64 / 1e8);
        let pset_b64 = pset.to_string();
        let pset_b64 = crate::hal_pset(&[
            "simplicity", "pset", "update-input", "-r", &pset_b64, "0",
            "-i", &input_utxo, "-c", &compiled.cmr_hex(), "-p", crate::NUMS_INTERNAL_KEY,
        ])?;

        let w = offer.tessera.settle_witness(0)?;
        let b64 = |bytes: &[u8]| base64::engine::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes);
        let pset_b64 = crate::hal_pset(&[
            "simplicity", "pset", "finalize", "-r", &pset_b64, "0",
            &b64(&w.program), &b64(&w.witness),
        ])?;

        let raw_tx = crate::hal_run(&["simplicity", "pset", "extract", "-r", &pset_b64])?;
        let raw_tx = raw_tx.trim().trim_matches('"').to_string();

        if std::env::var("MOSAIK_DEBUG_TX").is_ok() { eprintln!("RAWTX {raw_tx}"); }
        self.node.send_raw_transaction(&raw_tx)
    }
}

fn find_output_index(tx: &Value, spk_hex: &str) -> Option<u64> {
    tx.get("vout")?.as_array()?.iter().find_map(|out| {
        let hex = out.get("scriptpubkey")?.as_str()?;
        (hex == spk_hex).then(|| out.get("vout")?.as_u64()).flatten()
    })
}

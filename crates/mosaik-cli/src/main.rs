//! Mosaik demo CLI.
//!
//! The covenant-DEX flow, Phase 1 (quote / RFQ settlement):
//!
//!   mosaik make-offer ...   maker funds a covenant UTXO (price-less)
//!   mosaik quote      ...   maker signs a per-fill price quote
//!   mosaik take-offer ...   taker fills it at a quote (SETTLE path)
//!   mosaik reclaim    ...   maker reclaims it (REFUND path)

use anyhow::Result;
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

mod server;

#[derive(Parser)]
#[command(name = "mosaik")]
#[command(about = "Mosaik — a DEX on Liquid where every order is a Tessera, a self-enforcing Simplicity covenant")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Maker: fund a covenant UTXO on the regtest and print the offer JSON.
    MakeOffer {
        /// Sats of L-BTC the maker locks in the covenant.
        #[arg(long)]
        amount_a: u64,
        /// Asset id (RPC display order) the maker wants to be paid in.
        /// Defaults to L-BTC (the network policy asset).
        #[arg(long)]
        asset_b: Option<String>,
        /// Block height after which the maker may reclaim (REFUND).
        #[arg(long, default_value_t = 500)]
        timeout: u32,
    },
    /// Maker: sign a per-fill price quote for an offer; print the quote JSON.
    Quote {
        /// Path to the offer JSON produced by `make-offer`.
        #[arg(long)]
        offer: String,
        /// Raw units of `asset_b` the taker must pay the maker.
        #[arg(long)]
        amount_b: u64,
        /// Block height the quote is anchored to (the freshness floor).
        /// Defaults to the current chain tip.
        #[arg(long)]
        valid_height: Option<u32>,
    },
    /// Taker: fill a published offer at a maker-signed quote (SETTLE path).
    TakeOffer {
        /// Path to the offer JSON produced by `make-offer`.
        #[arg(long)]
        offer: String,
        /// Path to the signed quote JSON produced by `quote`.
        #[arg(long)]
        quote: String,
    },
    /// Maker: reclaim an unfilled offer after its timeout (REFUND path).
    Reclaim {
        /// Path to the offer JSON.
        #[arg(long)]
        offer: String,
    },
    /// Maker: publish a funded offer to the Nostr orderbook.
    PublishOffer {
        /// Path to the offer JSON (as printed by `make-offer`).
        #[arg(long)]
        offer: String,
        /// Nostr relay URL.
        #[arg(long, default_value = "ws://127.0.0.1:7777")]
        relay: String,
        /// Addressable order id (updates and cancellation reuse it).
        #[arg(long, default_value = "mosaik-offer")]
        order_id: String,
        /// Seconds from now until the offer is treated as stale.
        #[arg(long, default_value_t = 3600)]
        ttl: u64,
    },
    /// Taker: browse Tessera offers on the Nostr orderbook.
    BrowseOffers {
        /// Nostr relay URL.
        #[arg(long, default_value = "ws://127.0.0.1:7777")]
        relay: String,
    },
    /// Run a local Nostr relay for the orderbook (self-contained demo).
    ServeRelay {
        #[arg(long, default_value_t = 7777)]
        port: u16,
    },
    /// Serve the browser wallet UI for funding, quoting, and taking offers.
    ServeWallet {
        #[arg(long, default_value_t = 8080)]
        port: u16,
    },
}

/// A maker-signed quote, as written by `quote` and read by `take-offer`.
#[derive(Serialize, Deserialize)]
struct SignedQuote {
    quote: mosaik_core::Quote,
    /// The maker's 64-byte BIP-340 signature over the quote, hex.
    sig: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::MakeOffer { amount_a, asset_b, timeout } => {
            make_offer(amount_a, asset_b.as_deref(), timeout)
        }
        Command::Quote { offer, amount_b, valid_height } => {
            quote(&offer, amount_b, valid_height)
        }
        Command::TakeOffer { offer, quote } => take_offer(&offer, &quote),
        Command::Reclaim { offer } => reclaim(&offer),
        Command::PublishOffer { offer, relay, order_id, ttl } => {
            publish_offer(&offer, &relay, &order_id, ttl)
        }
        Command::BrowseOffers { relay } => browse_offers(&relay),
        Command::ServeRelay { port } => block_on(mosaik_relay::run_local_relay(port)),
        Command::ServeWallet { port } => server::run(port),
    }
}

/// Maker: fund a price-less covenant UTXO, print the offer JSON to stdout.
fn make_offer(amount_a: u64, asset_b: Option<&str>, timeout: u32) -> Result<()> {
    use mosaik_core::{
        lbtc_quote_tessera, quote_tessera_for, rpc::ElementsRpc, MakeOffer, MosaikMaker,
    };

    let rpc = ElementsRpc::regtest_wallet();
    let maker_address = rpc.new_unconfidential_address()?;
    let maker_pk = mosaik_core::demo_maker_pk();
    let tessera = match asset_b {
        Some(asset) => quote_tessera_for(&rpc, &maker_address, asset, timeout, maker_pk)?,
        None => lbtc_quote_tessera(&rpc, &maker_address, timeout, maker_pk)?,
    };
    let offer = MosaikMaker::regtest().make_offer("BTC", amount_a, &tessera, &maker_address)?;

    eprintln!("Funded a covenant offer:");
    eprintln!("  covenant UTXO : {}", offer.outpoint);
    eprintln!("  locked        : {} sats L-BTC", offer.amount_a);
    eprintln!("  maker paid to : {}", offer.maker_address);
    eprintln!("  price         : set per fill — see `mosaik quote`");
    println!("{}", serde_json::to_string(&offer)?);
    Ok(())
}

/// Maker: sign a per-fill price quote for an offer; print the quote JSON.
fn quote(offer_path: &str, amount_b: u64, valid_height: Option<u32>) -> Result<()> {
    use mosaik_core::{rpc::ElementsRpc, MosaikMaker, Offer, Quote, DEMO_MAKER_SECRET};

    let offer: Offer = serde_json::from_str(
        &std::fs::read_to_string(offer_path)
            .map_err(|e| anyhow::anyhow!("reading {offer_path}: {e}"))?,
    )?;

    let valid_height = match valid_height {
        Some(h) => h,
        None => ElementsRpc::regtest_wallet().block_count()? as u32,
    };
    let quote = Quote { amount_b, valid_height };
    let sig = MosaikMaker::regtest().sign_quote(&offer, &quote, &DEMO_MAKER_SECRET)?;

    eprintln!("Signed a quote for covenant {}:", offer.outpoint);
    eprintln!("  taker pays   : {amount_b} units of asset_b");
    eprintln!("  valid from   : height {valid_height}");
    println!("{}", serde_json::to_string(&SignedQuote { quote, sig: hex::encode(sig) })?);
    Ok(())
}

/// Taker: fill an offer at a maker-signed quote — the node enforces the covenant.
fn take_offer(offer_path: &str, quote_path: &str) -> Result<()> {
    use mosaik_core::{MosaikTaker, Offer, TakeOffer};

    let offer: Offer = serde_json::from_str(
        &std::fs::read_to_string(offer_path)
            .map_err(|e| anyhow::anyhow!("reading {offer_path}: {e}"))?,
    )?;
    let signed: SignedQuote = serde_json::from_str(
        &std::fs::read_to_string(quote_path)
            .map_err(|e| anyhow::anyhow!("reading {quote_path}: {e}"))?,
    )?;
    let sig: [u8; 64] = hex::decode(&signed.sig)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("quote signature must be 64 bytes of hex"))?;

    let txid = MosaikTaker::regtest().take_offer(&offer, &signed.quote, &sig)?;
    println!("Settled. The Liquid node executed the Tessera covenant and accepted the spend.");
    println!("  settlement txid: {txid}");
    Ok(())
}

/// Maker: reclaim an unfilled offer via the covenant's REFUND path.
fn reclaim(offer_path: &str) -> Result<()> {
    use mosaik_core::{MosaikMaker, Offer, ReclaimOffer, DEMO_MAKER_SECRET};

    let offer: Offer = serde_json::from_str(
        &std::fs::read_to_string(offer_path)
            .map_err(|e| anyhow::anyhow!("reading {offer_path}: {e}"))?,
    )?;

    let txid = MosaikMaker::regtest().reclaim(&offer, &DEMO_MAKER_SECRET)?;
    println!("Reclaimed. The covenant's REFUND path returned the locked asset to the maker.");
    println!("  reclaim txid: {txid}");
    Ok(())
}

/// Run a future on a fresh Tokio runtime (the Nostr client is async).
fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    tokio::runtime::Runtime::new()
        .expect("create Tokio runtime")
        .block_on(fut)
}

fn publish_offer(offer_path: &str, relay: &str, order_id: &str, ttl: u64) -> Result<()> {
    use mosaik_relay::{Keys, MosaikRelay, OfferStatus, TesseraOffer};

    let offer: mosaik_core::Offer = serde_json::from_str(
        &std::fs::read_to_string(offer_path)
            .map_err(|e| anyhow::anyhow!("reading {offer_path}: {e}"))?,
    )?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let tessera_offer = TesseraOffer {
        order_id: order_id.to_string(),
        offer,
        expiry: now + ttl,
        status: OfferStatus::Active,
    };

    block_on(async {
        let keys = Keys::generate();
        let book = MosaikRelay::connect(keys, &[relay]).await?;
        let event_id = book.publish_offer(&tessera_offer).await?;
        println!("Published offer '{order_id}' to {relay}");
        println!("  Nostr event: {event_id}");
        anyhow::Ok(())
    })
}

fn browse_offers(relay: &str) -> Result<()> {
    use mosaik_relay::{Keys, MosaikRelay};
    use std::time::Duration;

    block_on(async {
        let book = MosaikRelay::connect(Keys::generate(), &[relay]).await?;
        let offers = book.fetch_offers(Duration::from_secs(5)).await?;
        if offers.is_empty() {
            println!("No Tessera offers found on {relay}.");
            return anyhow::Ok(());
        }
        println!("Tessera offers on {relay}:\n");
        for o in &offers {
            println!(
                "  [{}] {} {} of {}  ->  wants asset {}  ({})",
                o.order_id,
                if o.is_expired() { "EXPIRED" } else { "active" },
                o.offer.amount_a,
                o.offer.asset_a,
                hex::encode(&o.offer.tessera.asset_b[..4]),
                o.offer.outpoint,
            );
        }
        anyhow::Ok(())
    })
}

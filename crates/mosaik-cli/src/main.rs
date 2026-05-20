//! Mosaik demo CLI.
//!
//! Three commands map onto the protocol in `docs/DESIGN.md` §5:
//!
//!   mosaik make-offer ...   maker funds a covenant UTXO (a Tessera)
//!   mosaik take-offer ...   taker fills it (SETTLE path)
//!   mosaik reclaim    ...   maker reclaims it (REFUND path)
//!

use anyhow::Result;
use clap::{Parser, Subcommand};
use tessera::Tessera;

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
        /// Raw units of `asset_b` the covenant requires the taker to pay the maker.
        #[arg(long)]
        amount_b: u64,
        /// Asset id (RPC display order) the maker wants to be paid in.
        /// Defaults to L-BTC (the network policy asset).
        #[arg(long)]
        asset_b: Option<String>,
        /// Block height after which the maker may reclaim (REFUND).
        #[arg(long, default_value_t = 500)]
        timeout: u32,
    },
    /// Taker: fill a published offer — the node executes the covenant (SETTLE).
    TakeOffer {
        /// Path to the offer JSON produced by `make-offer`.
        #[arg(long)]
        offer: String,
    },
    /// Maker: reclaim an unfilled offer after its timeout (REFUND path).
    Reclaim {
        /// Path to the offer JSON.
        #[arg(long)]
        offer: String,
    },
    /// Print the parameterised SimplicityHL covenant for a Tessera.
    ShowTessera {
        #[arg(long)]
        asset_b: String,
        #[arg(long)]
        amount_b: u64,
        #[arg(long)]
        maker_spk_hash: String,
        #[arg(long)]
        timeout: u32,
    },
    /// Compile a Tessera covenant and print its Commitment Merkle Root.
    CompileTessera {
        #[arg(long)]
        asset_b: String,
        #[arg(long)]
        amount_b: u64,
        #[arg(long)]
        maker_spk_hash: String,
        #[arg(long)]
        timeout: u32,
    },
    /// Build the Taproot SETTLE witness for spending a Tessera covenant.
    SettleWitness {
        #[arg(long)]
        asset_b: String,
        #[arg(long)]
        amount_b: u64,
        #[arg(long)]
        maker_spk_hash: String,
        #[arg(long)]
        timeout: u32,
        /// Index of the output that pays the maker.
        #[arg(long, default_value_t = 0)]
        settle_vout: u32,
    },
    /// Output compiled Tessera as JSON {cmr, program} where program is base64.
    /// Consumed by `hal-simplicity simplicity info` and `pset finalize`.
    TesseraJson {
        #[arg(long)]
        asset_b: String,
        #[arg(long)]
        amount_b: u64,
        #[arg(long)]
        maker_pk: String,
        #[arg(long)]
        maker_spk_hash: String,
        #[arg(long)]
        timeout: u32,
    },
    /// Output the SETTLE witness as JSON {program, witness} for `hal-simplicity pset finalize`.
    /// program is base64; witness is hex.
    SettleJson {
        #[arg(long)]
        asset_b: String,
        #[arg(long)]
        amount_b: u64,
        #[arg(long)]
        maker_pk: String,
        #[arg(long)]
        maker_spk_hash: String,
        #[arg(long)]
        timeout: u32,
        #[arg(long, default_value_t = 0)]
        settle_vout: u32,
    },
    /// Print the demo maker's constants for the Liquid testnet demo script.
    /// Outputs JSON with privkey, maker_pk, maker_address, maker_spk_hash,
    /// and the Liquid testnet L-BTC asset id in internal (tx/jet) byte order.
    TestnetConstants,
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
    /// Serve the browser wallet UI for funding, making, and taking offers.
    ServeWallet {
        #[arg(long, default_value_t = 8080)]
        port: u16,
        /// `regtest` (local node, default) or `testnet` (Liquid testnet node on port 7041).
        #[arg(long, default_value = "regtest")]
        network: String,
    },
}

fn parse_32(label: &str, s: &str) -> Result<[u8; 32]> {
    let bytes = hex::decode(s)?;
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("{label} must be 32 bytes of hex"))?;
    Ok(arr)
}

fn build_tessera(
    asset_b: &str,
    amount_b: u64,
    maker_spk_hash: &str,
    timeout: u32,
) -> Result<Tessera> {
    Ok(Tessera {
        asset_b: parse_32("asset_b", asset_b)?,
        amount_b,
        maker_spk_hash: parse_32("maker_spk_hash", maker_spk_hash)?,
        timeout,
    })
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::MakeOffer {
            amount_a,
            amount_b,
            asset_b,
            timeout,
        } => make_offer(amount_a, amount_b, asset_b.as_deref(), timeout),
        Command::TakeOffer { offer } => take_offer(&offer),
        Command::Reclaim { offer } => reclaim(&offer),
        Command::ShowTessera {
            asset_b,
            amount_b,
            maker_spk_hash,
            timeout,
        } => {
            let tessera = build_tessera(&asset_b, amount_b, &maker_spk_hash, timeout)?;
            println!("{}", tessera.render());
            Ok(())
        }
        Command::CompileTessera {
            asset_b,
            amount_b,
            maker_spk_hash,
            timeout,
        } => {
            let tessera = build_tessera(&asset_b, amount_b, &maker_spk_hash, timeout)?;
            let compiled = tessera.compile()?;
            println!("Tessera covenant compiled.");
            println!("  CMR:     {}", compiled.cmr_hex());
            println!("  Address: {}", compiled.address()?);
            println!("Fund the address to create this offer's covenant UTXO.");
            Ok(())
        }
        Command::SettleWitness {
            asset_b,
            amount_b,
            maker_spk_hash,
            timeout,
            settle_vout,
        } => {
            let tessera = build_tessera(&asset_b, amount_b, &maker_spk_hash, timeout)?;
            let wit = tessera.settle_witness(settle_vout)?;
            println!("Tessera SETTLE witness (Taproot script-path, bottom to top):");
            println!("  program:       {}", hex::encode(&wit.program));
            println!("  witness:       {}", hex::encode(&wit.witness));
            println!("  leaf_script:   {}", hex::encode(&wit.leaf_script));
            println!("  control_block: {}", hex::encode(&wit.control_block));
            Ok(())
        }
        Command::TesseraJson {
            asset_b,
            amount_b,
            maker_pk,
            maker_spk_hash,
            timeout,
        } => {
            let tessera = build_tessera(&asset_b, amount_b, &maker_pk, &maker_spk_hash, timeout)?;
            tessera_json(&tessera)
        }
        Command::SettleJson {
            asset_b,
            amount_b,
            maker_pk,
            maker_spk_hash,
            timeout,
            settle_vout,
        } => {
            let tessera = build_tessera(&asset_b, amount_b, &maker_pk, &maker_spk_hash, timeout)?;
            settle_json(&tessera, settle_vout)
        }
        Command::TestnetConstants => testnet_constants(),
        Command::PublishOffer {
            offer,
            relay,
            order_id,
            ttl,
        } => publish_offer(&offer, &relay, &order_id, ttl),
        Command::BrowseOffers { relay } => browse_offers(&relay),
        Command::ServeRelay { port } => {
            block_on(mosaik_relay::run_local_relay(port))
        }
        Command::ServeWallet { port, network } => {
            use server::Network;
            let net = if network == "testnet" { Network::Testnet } else { Network::Regtest };
            server::run(port, net)
        }
    }
}

fn tessera_json(tessera: &Tessera) -> Result<()> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    // settle_witness(0) gives us the compiled program bytes (same for any settle_vout).
    let wit = tessera.settle_witness(0)?;
    let cmr = tessera.compile()?.cmr_hex();
    println!(
        "{}",
        serde_json::json!({ "cmr": cmr, "program": B64.encode(&wit.program) })
    );
    Ok(())
}

fn settle_json(tessera: &Tessera, settle_vout: u32) -> Result<()> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    let wit = tessera.settle_witness(settle_vout)?;
    println!(
        "{}",
        serde_json::json!({
            "program": B64.encode(&wit.program),
            "witness": hex::encode(&wit.witness),
        })
    );
    Ok(())
}

fn testnet_constants() -> Result<()> {
    use mosaik_core::DEMO_MAKER_SECRET;
    // Liquid testnet L-BTC asset in internal (tx/jet) byte order — display order reversed.
    // Display: 144c654344aa716d6f3abcc1ca90e5641e4e2a7f633bc09fe3baf64585819a49
    const LBTC_INTERNAL: &str =
        "499a818545f6bae39fc03b637f2a4e1e64e590cac1bc3a6f6d71aa4443654c14";
    // Faucet return address (well-known, unconfidential) and its scriptPubKey hash.
    const MAKER_ADDRESS: &str = "tex1qkkxzy9glfws4nc392an5w2kgjym7sxpshuwkjy";
    const MAKER_SPK_HASH: &str =
        "bcfbe70502021903755bb406a7c4681817be317affc7d1120de2041a9e06cfc5";

    let maker_pk = mosaik_core::demo_maker_pk();
    println!(
        "{}",
        serde_json::json!({
            "privkey":         hex::encode(DEMO_MAKER_SECRET),
            "maker_pk":        hex::encode(maker_pk),
            "maker_address":   MAKER_ADDRESS,
            "maker_spk_hash":  MAKER_SPK_HASH,
            "lbtc_asset":      LBTC_INTERNAL,
        })
    );
    Ok(())
}

/// Maker: fund a covenant UTXO on the regtest, print the offer JSON to stdout.
fn make_offer(
    amount_a: u64,
    amount_b: u64,
    asset_b: Option<&str>,
    timeout: u32,
) -> Result<()> {
    use mosaik_core::{lbtc_tessera, rpc::ElementsRpc, tessera_for, MakeOffer, MosaikMaker};

    let rpc = ElementsRpc::regtest_wallet();
    let maker_address = rpc.new_unconfidential_address()?;
    let tessera = match asset_b {
        Some(asset) => tessera_for(&rpc, &maker_address, asset, amount_b, timeout)?,
        None => lbtc_tessera(&rpc, &maker_address, amount_b, timeout)?,
    };
    let offer = MosaikMaker::regtest().make_offer("BTC", amount_a, &tessera, &maker_address)?;

    // Human-readable summary to stderr; the offer JSON to stdout (pipe it).
    eprintln!("Funded a covenant offer:");
    eprintln!("  covenant UTXO : {}", offer.outpoint);
    eprintln!("  locked        : {} sats L-BTC", offer.amount_a);
    eprintln!(
        "  maker wants   : {} sats paid to {}",
        offer.tessera.amount_b, offer.maker_address
    );
    println!("{}", serde_json::to_string(&offer)?);
    Ok(())
}

/// Taker: fill an offer — the Liquid node executes and enforces the covenant.
fn take_offer(offer_path: &str) -> Result<()> {
    use mosaik_core::{MosaikTaker, Offer, TakeOffer};

    let offer_json = std::fs::read_to_string(offer_path)
        .map_err(|e| anyhow::anyhow!("reading {offer_path}: {e}"))?;
    let offer: Offer = serde_json::from_str(&offer_json)?;

    let txid = MosaikTaker::regtest().take_offer(&offer)?;
    println!("Settled. The Liquid node executed the Tessera covenant and accepted the spend.");
    println!("  settlement txid: {txid}");
    Ok(())
}

/// Maker: reclaim an unfilled offer via the covenant's REFUND path.
fn reclaim(offer_path: &str) -> Result<()> {
    use mosaik_core::{MosaikMaker, Offer, ReclaimOffer};

    let offer_json = std::fs::read_to_string(offer_path)
        .map_err(|e| anyhow::anyhow!("reading {offer_path}: {e}"))?;
    let offer: Offer = serde_json::from_str(&offer_json)?;

    let txid = MosaikMaker::regtest().reclaim(&offer)?;
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

    let offer_json = std::fs::read_to_string(offer_path)
        .map_err(|e| anyhow::anyhow!("reading {offer_path}: {e}"))?;
    let offer: mosaik_core::Offer = serde_json::from_str(&offer_json)?;

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
                "  [{}] {} {} of {}  →  wants {} of asset {}  ({})",
                o.order_id,
                if o.is_expired() { "EXPIRED" } else { "active" },
                o.offer.amount_a,
                o.offer.asset_a,
                o.offer.tessera.amount_b,
                hex::encode(&o.offer.tessera.asset_b[..4]),
                o.offer.outpoint,
            );
        }
        anyhow::Ok(())
    })
}

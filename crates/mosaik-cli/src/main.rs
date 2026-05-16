//! Mosaik demo CLI.
//!
//! Three commands map onto the protocol in `docs/DESIGN.md` §5:
//!
//!   mosaik make-offer ...   maker funds a covenant UTXO (a Tessera)
//!   mosaik take-offer ...   taker fills it (SETTLE path)
//!   mosaik reclaim    ...   maker reclaims it (REFUND path)
//!
//! The Liquid side (PSET build/sign/broadcast) is wired during the hackathon —
//! see the `TODO(hackathon)` markers in `mosaik-core`.

use anyhow::Result;
use clap::{Parser, Subcommand};
use tessera::Tessera;

#[derive(Parser)]
#[command(name = "mosaik")]
#[command(about = "Mosaik — a DEX on Liquid where every order is a Tessera, a self-enforcing Simplicity covenant")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Maker: fund a covenant UTXO and print the offer.
    MakeOffer {
        /// Asset id (hex) the maker sells.
        #[arg(long)]
        asset_a: String,
        /// Amount of asset A to lock in the offer.
        #[arg(long)]
        amount_a: u64,
        /// Asset id (hex) the maker wants in return.
        #[arg(long)]
        asset_b: String,
        /// Exact amount of asset B the maker must be paid.
        #[arg(long)]
        amount_b: u64,
        /// Maker x-only pubkey (hex) for the refund path.
        #[arg(long)]
        maker_pk: String,
        /// SHA-256 of the maker scriptPubKey (hex) the counter-payment goes to.
        #[arg(long)]
        maker_spk_hash: String,
        /// Block height after which the maker may reclaim.
        #[arg(long)]
        timeout: u32,
    },
    /// Taker: fill a published offer (SETTLE path).
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
        maker_pk: String,
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
        maker_pk: String,
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
        maker_pk: String,
        #[arg(long)]
        maker_spk_hash: String,
        #[arg(long)]
        timeout: u32,
        /// Index of the output that pays the maker.
        #[arg(long, default_value_t = 0)]
        settle_vout: u32,
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
    maker_pk: &str,
    maker_spk_hash: &str,
    timeout: u32,
) -> Result<Tessera> {
    Ok(Tessera {
        asset_b: parse_32("asset_b", asset_b)?,
        amount_b,
        maker_spk_hash: parse_32("maker_spk_hash", maker_spk_hash)?,
        timeout,
        maker_pk: parse_32("maker_pk", maker_pk)?,
    })
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::MakeOffer {
            asset_a,
            amount_a,
            asset_b,
            amount_b,
            maker_pk,
            maker_spk_hash,
            timeout,
        } => {
            let tessera = build_tessera(&asset_b, amount_b, &maker_pk, &maker_spk_hash, timeout)?;
            println!("Tessera: {}", serde_json::to_string_pretty(&tessera)?);
            println!("Selling {amount_a} of {asset_a}");
            // TODO(hackathon): mosaik_core::MakeOffer::make_offer — derive the
            // covenant address, fund it, broadcast, print the offer JSON.
            anyhow::bail!("make-offer: Liquid funding not wired yet (see mosaik-core)");
        }
        Command::TakeOffer { offer } => {
            println!("Taking offer from {offer}");
            // TODO(hackathon): mosaik_core::TakeOffer::take_offer.
            anyhow::bail!("take-offer: settlement tx not wired yet (see mosaik-core)");
        }
        Command::Reclaim { offer } => {
            println!("Reclaiming offer from {offer}");
            // TODO(hackathon): mosaik_core::ReclaimOffer::reclaim.
            anyhow::bail!("reclaim: refund tx not wired yet (see mosaik-core)");
        }
        Command::ShowTessera {
            asset_b,
            amount_b,
            maker_pk,
            maker_spk_hash,
            timeout,
        } => {
            let tessera = build_tessera(&asset_b, amount_b, &maker_pk, &maker_spk_hash, timeout)?;
            println!("{}", tessera.render());
            Ok(())
        }
        Command::CompileTessera {
            asset_b,
            amount_b,
            maker_pk,
            maker_spk_hash,
            timeout,
        } => {
            let tessera = build_tessera(&asset_b, amount_b, &maker_pk, &maker_spk_hash, timeout)?;
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
            maker_pk,
            maker_spk_hash,
            timeout,
            settle_vout,
        } => {
            let tessera = build_tessera(&asset_b, amount_b, &maker_pk, &maker_spk_hash, timeout)?;
            let wit = tessera.settle_witness(settle_vout)?;
            println!("Tessera SETTLE witness (Taproot script-path, bottom to top):");
            println!("  program:       {}", hex::encode(&wit.program));
            println!("  witness:       {}", hex::encode(&wit.witness));
            println!("  leaf_script:   {}", hex::encode(&wit.leaf_script));
            println!("  control_block: {}", hex::encode(&wit.control_block));
            Ok(())
        }
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
    }
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

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
    }
}

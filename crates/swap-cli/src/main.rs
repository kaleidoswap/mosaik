//! covenant-swap demo CLI.
//!
//! Three commands map onto the protocol in `docs/DESIGN.md` §5:
//!
//!   covenant-swap make-offer ...   maker funds a covenant UTXO
//!   covenant-swap take-offer ...   taker fills it (SETTLE path)
//!   covenant-swap reclaim    ...   maker reclaims it (REFUND path)
//!
//! The Liquid side (PSET build/sign/broadcast) is wired during the hackathon —
//! see the `TODO(hackathon)` markers in `swap-core`.

use anyhow::Result;
use clap::{Parser, Subcommand};
use covenant::SwapTerms;

#[derive(Parser)]
#[command(name = "covenant-swap")]
#[command(about = "A swap offer that lives inside a coin — Simplicity covenant swaps on Liquid")]
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
    /// Print the parameterised SimplicityHL covenant for a set of terms.
    ShowCovenant {
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

fn build_terms(
    asset_b: &str,
    amount_b: u64,
    maker_pk: &str,
    maker_spk_hash: &str,
    timeout: u32,
) -> Result<SwapTerms> {
    Ok(SwapTerms {
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
            let terms = build_terms(&asset_b, amount_b, &maker_pk, &maker_spk_hash, timeout)?;
            println!("Covenant terms: {}", serde_json::to_string_pretty(&terms)?);
            println!("Selling {amount_a} of {asset_a}");
            // TODO(hackathon): swap_core::MakeOffer::make_offer — derive the
            // covenant address, fund it, broadcast, print the offer JSON.
            anyhow::bail!("make-offer: Liquid funding not wired yet (see swap-core)");
        }
        Command::TakeOffer { offer } => {
            println!("Taking offer from {offer}");
            // TODO(hackathon): swap_core::TakeOffer::take_offer.
            anyhow::bail!("take-offer: settlement tx not wired yet (see swap-core)");
        }
        Command::Reclaim { offer } => {
            println!("Reclaiming offer from {offer}");
            // TODO(hackathon): swap_core::ReclaimOffer::reclaim.
            anyhow::bail!("reclaim: refund tx not wired yet (see swap-core)");
        }
        Command::ShowCovenant {
            asset_b,
            amount_b,
            maker_pk,
            maker_spk_hash,
            timeout,
        } => {
            let terms = build_terms(&asset_b, amount_b, &maker_pk, &maker_spk_hash, timeout)?;
            println!("{}", terms.render());
            Ok(())
        }
    }
}

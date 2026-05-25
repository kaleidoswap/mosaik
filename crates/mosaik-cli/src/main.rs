//! Mosaik demo CLI.
//!
//! Three commands map onto the protocol in `docs/DESIGN.md` §5:
//!
//!   mosaik make-offer ...   maker funds a covenant UTXO (a Tessera)
//!   mosaik take-offer ...   taker fills it (SETTLE path)
//!   mosaik reclaim    ...   maker reclaims it (REFUND path)
//!

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use tessera::Tessera;

/// Which settlement backend a command targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum, Default)]
enum Backend {
    /// Liquid Network — a Simplicity-covenant Tessera (the original backend).
    #[default]
    Liquid,
    /// Arkade — a banco covenant VTXO via the banco-proxy gateway.
    Banco,
}

#[derive(Parser)]
#[command(name = "mosaik")]
#[command(
    about = "Mosaik — a DEX on Liquid where every order is a Tessera, a self-enforcing Simplicity covenant"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Maker: publish a covenant offer.
    MakeOffer {
        /// Settlement backend.
        #[arg(long, value_enum, default_value_t = Backend::Liquid)]
        backend: Backend,
        /// Raw units the maker locks (L-BTC for Liquid; BTC-on-Ark for banco).
        #[arg(long)]
        amount_a: u64,
        /// Raw units of `asset_b` the maker requires the taker to pay.
        #[arg(long)]
        amount_b: u64,
        /// (Liquid) Asset id (RPC display order) the maker wants — defaults to L-BTC.
        #[arg(long)]
        asset_b: Option<String>,
        /// (Liquid) Block height after which the maker may reclaim.
        #[arg(long, default_value_t = 500)]
        timeout: u32,
        /// (banco) Unix-second timestamp after which the maker may cancel.
        /// Defaults to two hours from now.
        #[arg(long)]
        cancel_delay: Option<u64>,
        /// (banco) Accept partial fills?
        #[arg(long, default_value_t = false)]
        partial_fill: bool,
        /// (banco) banco-proxy gateway URL.
        #[arg(long, default_value = "http://127.0.0.1:3001")]
        banco_proxy_url: String,
        /// (banco) banco-proxy wallet id to act as.
        #[arg(long, default_value = "maker")]
        wallet_id: String,
    },
    /// Taker: fill a published offer.
    TakeOffer {
        /// Settlement backend.
        #[arg(long, value_enum, default_value_t = Backend::Liquid)]
        backend: Backend,
        /// Path to the offer JSON produced by `make-offer`.
        #[arg(long)]
        offer: String,
        /// (banco) Optional partial-fill amount (raw units of `want_asset`).
        #[arg(long)]
        fill_amount: Option<u64>,
        /// (banco) banco-proxy gateway URL.
        #[arg(long, default_value = "http://127.0.0.1:3001")]
        banco_proxy_url: String,
        /// (banco) banco-proxy wallet id to act as.
        #[arg(long, default_value = "taker")]
        wallet_id: String,
    },
    /// Maker: reclaim/cancel an unfilled offer after its timeout.
    Reclaim {
        /// Settlement backend.
        #[arg(long, value_enum, default_value_t = Backend::Liquid)]
        backend: Backend,
        /// Path to the offer JSON.
        #[arg(long)]
        offer: String,
        /// (banco) banco-proxy gateway URL.
        #[arg(long, default_value = "http://127.0.0.1:3001")]
        banco_proxy_url: String,
        /// (banco) banco-proxy wallet id to act as.
        #[arg(long, default_value = "maker")]
        wallet_id: String,
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
    },

    /// Cross-chain swap (Phase 8): draft a Liquid-Tessera ↔ Arkade-banco swap.
    ///
    /// Today this prints the envelope produced by the in-memory mock
    /// orchestrator and surfaces the primitive-gap error explaining what's
    /// blocking a real on-chain run. See docs/CROSS_CHAIN_SWAP.md.
    BridgeMake {
        /// Addressable swap id (used as the `d` tag on kind-30052).
        #[arg(long, default_value = "mosaik-swap")]
        swap_id: String,
        /// `T_short` for leg-B (unix seconds). Defaults to now + 1h.
        #[arg(long)]
        b_refund: Option<u64>,
        /// `T_long` for leg-A (unix seconds). Defaults to now + 2h.
        #[arg(long)]
        a_refund: Option<u64>,
        /// Minimum required gap a_refund - b_refund (seconds). Default 1800.
        #[arg(long, default_value_t = 1800)]
        safety_margin: u64,
        /// Write the envelope JSON here. Defaults to `<swap_id>.envelope.json`.
        #[arg(long)]
        out: Option<String>,
        /// Skip publishing the on-chain legs. Today the only behaviour: the
        /// primitive gaps are not closed yet. Kept as an explicit flag so the
        /// CLI is forward-compatible.
        #[arg(long, default_value_t = true)]
        dry_run: bool,
    },

    /// Cross-chain swap (Phase 8): fulfil one leg of a swap as the taker.
    BridgeTake {
        /// Path to the envelope JSON.
        #[arg(long)]
        envelope: String,
    },

    /// Cross-chain swap (Phase 8): once a leg is spent, claim the other leg
    /// with the revealed preimage.
    BridgeClaim {
        /// Path to the envelope JSON.
        #[arg(long)]
        envelope: String,
        /// The revealed preimage (64-char hex).
        #[arg(long)]
        preimage: String,
    },

    /// Cross-chain swap (Phase 8): print the swap envelope's current state.
    BridgeStatus {
        #[arg(long)]
        envelope: String,
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
            backend,
            amount_a,
            amount_b,
            asset_b,
            timeout,
            cancel_delay,
            partial_fill,
            banco_proxy_url,
            wallet_id,
        } => match backend {
            Backend::Liquid => make_offer(amount_a, amount_b, asset_b.as_deref(), timeout),
            Backend::Banco => banco_make_offer(
                amount_a,
                amount_b,
                asset_b.as_deref(),
                cancel_delay,
                partial_fill,
                &banco_proxy_url,
                &wallet_id,
            ),
        },
        Command::TakeOffer {
            backend,
            offer,
            fill_amount,
            banco_proxy_url,
            wallet_id,
        } => match backend {
            Backend::Liquid => take_offer(&offer),
            Backend::Banco => banco_take_offer(&offer, fill_amount, &banco_proxy_url, &wallet_id),
        },
        Command::Reclaim {
            backend,
            offer,
            banco_proxy_url,
            wallet_id,
        } => match backend {
            Backend::Liquid => reclaim(&offer),
            Backend::Banco => banco_reclaim(&offer, &banco_proxy_url, &wallet_id),
        },
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
        Command::PublishOffer {
            offer,
            relay,
            order_id,
            ttl,
        } => publish_offer(&offer, &relay, &order_id, ttl),
        Command::BrowseOffers { relay } => browse_offers(&relay),
        Command::ServeRelay { port } => block_on(mosaik_relay::run_local_relay(port)),
        Command::ServeWallet { port } => block_on(mosaik_api::run(port)),

        Command::BridgeMake {
            swap_id,
            b_refund,
            a_refund,
            safety_margin,
            out,
            dry_run,
        } => bridge_make(swap_id, b_refund, a_refund, safety_margin, out, dry_run),
        Command::BridgeTake { envelope } => bridge_take(&envelope),
        Command::BridgeClaim { envelope, preimage } => bridge_claim(&envelope, &preimage),
        Command::BridgeStatus { envelope } => bridge_status(&envelope),
    }
}

/// Maker: fund a covenant UTXO on the regtest, print the offer JSON to stdout.
fn make_offer(amount_a: u64, amount_b: u64, asset_b: Option<&str>, timeout: u32) -> Result<()> {
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
        offer.tessera()?.amount_b,
        offer.maker_address
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
            use mosaik_core::OfferKind;
            let (amount_b, asset_label) = match &o.offer.kind {
                OfferKind::Liquid(t) => (t.amount_b, hex::encode(&t.asset_b[..4])),
                OfferKind::Banco(b) => (
                    b.want_amount,
                    b.want_asset.clone().unwrap_or_else(|| "BTC".into()),
                ),
            };
            println!(
                "  [{}] {} ({}) {} of {}  →  wants {} of asset {}  ({})",
                o.order_id,
                if o.is_expired() { "EXPIRED" } else { "active" },
                o.offer.backend(),
                o.offer.amount_a,
                o.offer.asset_a,
                amount_b,
                asset_label,
                o.offer.outpoint,
            );
        }
        anyhow::Ok(())
    })
}

// ─── Arkade banco backend ────────────────────────────────────────────────────

/// Current unix time in seconds.
fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before 1970")
        .as_secs()
}

/// Read and parse an offer JSON file.
fn read_offer(path: &str) -> Result<mosaik_core::Offer> {
    let json = std::fs::read_to_string(path).map_err(|e| anyhow::anyhow!("reading {path}: {e}"))?;
    serde_json::from_str(&json).map_err(|e| anyhow::anyhow!("parsing {path}: {e}"))
}

/// Maker (banco): build, fund, and publish a banco covenant offer. Prints the
/// offer JSON to stdout (pipe to a file).
fn banco_make_offer(
    amount_a: u64,
    amount_b: u64,
    want_asset: Option<&str>,
    cancel_delay: Option<u64>,
    partial_fill: bool,
    banco_proxy_url: &str,
    wallet_id: &str,
) -> Result<()> {
    use mosaik_banco::{BancoClient, BancoMaker, OfferTerms};

    let cancel_delay = cancel_delay.unwrap_or_else(|| unix_now() + 7200);
    let terms = OfferTerms {
        sell_amount: amount_a,
        sell_asset: None, // BTC-on-Ark; future: --asset-a flag
        want_amount: amount_b,
        want_asset: want_asset.map(str::to_string),
        cancel_delay,
        partial_fill,
    };

    let client = BancoClient::new(banco_proxy_url, wallet_id);
    let offer = BancoMaker::new(client).make_offer(&terms)?;

    let banco = offer.banco()?;
    eprintln!("Published a banco covenant offer:");
    eprintln!("  funding tx    : {}", offer.outpoint);
    eprintln!("  swap pkScript : {}", banco.swap_pk_script);
    eprintln!("  locked        : {} of {}", offer.amount_a, offer.asset_a);
    eprintln!(
        "  maker wants   : {} of {}",
        banco.want_amount,
        banco.want_asset.as_deref().unwrap_or("BTC")
    );
    eprintln!("  cancel after  : unix {}", banco.cancel_delay);
    eprintln!("  partial fills : {}", banco.partial_fill);
    println!("{}", serde_json::to_string(&offer)?);
    Ok(())
}

/// Taker (banco): fulfil a banco covenant offer in a single arkd tx.
fn banco_take_offer(
    offer_path: &str,
    fill_amount: Option<u64>,
    banco_proxy_url: &str,
    wallet_id: &str,
) -> Result<()> {
    use mosaik_banco::{BancoClient, BancoTaker};

    let offer = read_offer(offer_path)?;
    let client = BancoClient::new(banco_proxy_url, wallet_id);
    let txid = BancoTaker::new(client).fulfill(&offer, fill_amount)?;
    println!("Fulfilled banco offer in a single arkd transaction.");
    println!("  settlement txid: {txid}");
    Ok(())
}

/// Maker (banco): cancel an unfilled offer after its cancel_delay.
fn banco_reclaim(offer_path: &str, banco_proxy_url: &str, wallet_id: &str) -> Result<()> {
    use mosaik_banco::{BancoClient, BancoMaker};

    let offer = read_offer(offer_path)?;
    let client = BancoClient::new(banco_proxy_url, wallet_id);
    let txid = BancoMaker::new(client).cancel(&offer)?;
    println!("Cancelled banco offer.");
    println!("  cancel txid: {txid}");
    Ok(())
}

// ─── Cross-chain bridge (Phase 8) ────────────────────────────────────────────

/// Draft (and optionally publish, when the primitive gaps close) a
/// Liquid-Tessera ↔ Arkade-banco swap. Today this only walks the mock
/// orchestrator; the publish-on-chain path returns a clean primitive-gap
/// error pointing at docs/CROSS_CHAIN_SWAP.md.
fn bridge_make(
    swap_id: String,
    b_refund: Option<u64>,
    a_refund: Option<u64>,
    safety_margin: u64,
    out: Option<String>,
    dry_run: bool,
) -> Result<()> {
    use mosaik_bridge::{
        assert_primitives_available, BridgeOrchestrator, LegBackend, MockOrchestrator, SwapTerms,
        SwapTimelocks,
    };

    let now = unix_now();
    let timelocks = SwapTimelocks {
        a_refund: a_refund.unwrap_or(now + 7200),
        b_refund: b_refund.unwrap_or(now + 3600),
        safety_margin,
    };
    timelocks
        .validate()
        .map_err(|e| anyhow::anyhow!("invalid timelocks: {e}"))?;

    // Always start by drafting through the mock so the envelope is
    // well-formed and clients have something to point at.
    let mut orch = MockOrchestrator::new();
    let (envelope, preimage) = orch
        .draft(SwapTerms {
            swap_id: swap_id.clone(),
            leg_a_backend: LegBackend::Liquid,
            leg_b_backend: LegBackend::Banco,
            timelocks,
        })
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let out_path = out.unwrap_or_else(|| format!("{swap_id}.envelope.json"));
    std::fs::write(&out_path, serde_json::to_string_pretty(&envelope)?)?;

    eprintln!("Drafted cross-chain swap envelope:");
    eprintln!("  swap_id       : {}", envelope.swap_id);
    eprintln!("  hashlock      : {}", envelope.preimage_hash.to_hex());
    eprintln!(
        "  legs          : {} (refund {}) ↔ {} (refund {})",
        envelope.leg_a.backend,
        envelope.timelocks.a_refund,
        envelope.leg_b.backend,
        envelope.timelocks.b_refund
    );
    eprintln!("  envelope      : {out_path}");
    eprintln!();
    eprintln!("  SECRET preimage (keep private, needed for bridge-claim):");
    eprintln!("  {}", preimage.to_hex());

    if !dry_run {
        // The default is dry_run=true; this branch is here so the CLI is
        // forward-compatible. The real publish path lights up when the two
        // primitive gaps close (see docs/CROSS_CHAIN_SWAP.md §"Primitive gaps").
        assert_primitives_available(LegBackend::Liquid, LegBackend::Banco)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
    } else {
        eprintln!();
        eprintln!(
            "(dry-run) Cross-chain on-chain publication is gated on the two \
             primitive extensions documented in docs/CROSS_CHAIN_SWAP.md \
             §\"Primitive gaps\". The envelope above is a valid mock draft."
        );
    }

    Ok(())
}

/// Fulfil one leg of a swap as the taker. Currently surfaces the primitive
/// gap; the envelope still loads / validates so we exercise the read path.
fn bridge_take(envelope_path: &str) -> Result<()> {
    let env: mosaik_bridge::SwapEnvelope = read_json(envelope_path)?;
    env.validate().map_err(anyhow::Error::msg)?;
    eprintln!(
        "Loaded swap envelope: {} ({})",
        env.swap_id,
        env.state.label()
    );
    let gap = mosaik_bridge::banco_hashlock_gap();
    Err(anyhow::anyhow!("{gap}"))
}

/// Claim the second leg once the preimage has been revealed on-chain.
fn bridge_claim(envelope_path: &str, preimage_hex: &str) -> Result<()> {
    use mosaik_bridge::Preimage;
    let env: mosaik_bridge::SwapEnvelope = read_json(envelope_path)?;
    env.validate().map_err(anyhow::Error::msg)?;
    let p = Preimage::from_hex(preimage_hex)?;
    if !env.preimage_hash.matches(&p) {
        anyhow::bail!("preimage does not hash to the envelope's commitment");
    }
    eprintln!("Preimage verified against envelope commitment.");
    let gap = mosaik_bridge::liquid_hashlock_gap();
    Err(anyhow::anyhow!("{gap}"))
}

/// Print the current state of a swap envelope.
fn bridge_status(envelope_path: &str) -> Result<()> {
    let env: mosaik_bridge::SwapEnvelope = read_json(envelope_path)?;
    env.validate().map_err(anyhow::Error::msg)?;
    println!("swap_id    : {}", env.swap_id);
    println!("state      : {}", env.state.label());
    println!("hashlock   : {}", env.preimage_hash.to_hex());
    println!(
        "leg-A      : backend={} outpoint={} event={}",
        env.leg_a.backend,
        env.leg_a.outpoint,
        env.leg_a.event_id.as_deref().unwrap_or("-")
    );
    println!(
        "leg-B      : backend={} outpoint={} event={}",
        env.leg_b.backend,
        env.leg_b.outpoint,
        env.leg_b.event_id.as_deref().unwrap_or("-")
    );
    println!(
        "timelocks  : a_refund={} b_refund={} safety_margin={}",
        env.timelocks.a_refund, env.timelocks.b_refund, env.timelocks.safety_margin
    );
    Ok(())
}

/// Read + parse a JSON file into a typed value.
fn read_json<T: serde::de::DeserializeOwned>(path: &str) -> Result<T> {
    let s = std::fs::read_to_string(path).map_err(|e| anyhow::anyhow!("reading {path}: {e}"))?;
    serde_json::from_str(&s).map_err(|e| anyhow::anyhow!("parsing {path}: {e}"))
}

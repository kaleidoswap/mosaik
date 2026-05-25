//! `mosaik-api` — the API daemon binary.
//!
//! A thin wrapper around [`mosaik_api::run`]. The same entry point is reachable
//! from the CLI via `mosaik serve-wallet`; this binary exists so the API ships
//! as its own image without dragging the `mosaik` CLI along.

use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(name = "mosaik-api")]
#[command(about = "Mosaik covenant-DEX HTTP API")]
struct Args {
    /// TCP port to listen on.
    #[arg(long, env = "MOSAIK_API_PORT", default_value_t = 8080)]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mosaik_api=info,tower_http=info".into()),
        )
        .init();

    let args = Args::parse();
    mosaik_api::run(args.port).await
}

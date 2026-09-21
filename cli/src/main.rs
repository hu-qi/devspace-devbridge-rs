mod api;
mod commands;
mod config;
mod output;
mod transport;

use anyhow::Result;
use clap::Parser;
use commands::Command;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(
    name = "devbridge",
    version,
    about = "DevBridge CLI — Rust implementation"
)]
struct Cli {
    #[arg(short, long, global = true)]
    verbose: bool,
    #[arg(long, global = true, env = "DEVBRIDGE_API_BASE", hide = true)]
    api_base: Option<String>,
    #[arg(long, global = true, env = "DEVBRIDGE_CLUSTER_ID", hide = true)]
    cluster_id: Option<String>,
    #[command(subcommand)]
    command: Command,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let filter = if cli.verbose { "debug" } else { "warn" };
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(filter))
        .init();
    commands::run(
        cli.command,
        cli.api_base.as_deref(),
        cli.cluster_id.as_deref(),
    )
    .await
}

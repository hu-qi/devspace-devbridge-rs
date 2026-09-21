mod api;
mod auth;
mod commands;
mod config;
mod i18n;
mod netutil;
mod output;
mod transport;
mod updater;

use anyhow::Result;
use clap::Parser;
use commands::Command;
use tracing_subscriber::EnvFilter;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Parser)]
#[command(
    name = "devbridge",
    disable_version_flag = true,
    about = "DevBridge remote tunnel port forwarding tool"
)]
struct Cli {
    #[arg(short, long, global = true, help = "Enable debug logging")]
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
        .with_target(false)
        .init();

    if !cli.command.is_version() {
        updater::check_async(VERSION);
    }

    commands::run(
        cli.command,
        cli.api_base.as_deref(),
        cli.cluster_id.as_deref(),
        VERSION,
    )
    .await
}

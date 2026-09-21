use crate::{api::ApiClient, commands::{ConnectArgs, HostArgs}, config};
use anyhow::{Result, bail};

/// Data-plane boundary for the relay protocol.
///
/// The previous Go implementation coupled Cobra command handling, WebSocket setup,
/// reconnect policy and SSH port-forwarding. The Rust rewrite keeps the public CLI
/// stable but isolates the transport so the relay protocol can evolve independently.
pub async fn host(args: HostArgs, api_base: Option<&str>, cluster: Option<&str>) -> Result<()> {
    let id = args.tunnel_id.or_else(|| config::default_tunnel().ok())
        .ok_or_else(|| anyhow::anyhow!("tunnel ID is required"))?;
    let client = ApiClient::new(args.api_key.as_deref(), api_base, cluster)?;
    let ports = if args.ports.is_empty() {
        client.list_ports(&id).await?.into_iter().map(|p| p.port).collect::<Vec<_>>()
    } else {
        args.ports
    };
    if ports.is_empty() { bail!("no ports configured for tunnel {id}"); }
    let token = match args.token {
        Some(v) => v,
        None if args.api_key.is_some() => String::new(),
        None => client.token(&id, "host").await?.token,
    };
    run_relay("host", &id, &ports, &token).await
}

pub async fn connect(args: ConnectArgs, api_base: Option<&str>, cluster: Option<&str>) -> Result<()> {
    let id = args.tunnel_id.or_else(|| config::default_tunnel().ok())
        .ok_or_else(|| anyhow::anyhow!("tunnel ID is required"))?;
    let client = ApiClient::new(args.api_key.as_deref(), api_base, cluster)?;
    let ports = client.list_ports(&id).await?.into_iter().map(|p| p.port).collect::<Vec<_>>();
    if ports.is_empty() && args.token.is_none() { bail!("no ports configured for tunnel {id}"); }
    let token = match args.token {
        Some(v) => v,
        None if args.api_key.is_some() => String::new(),
        None => client.token(&id, "connect").await?.token,
    };
    run_relay("connect", &id, &ports, &token).await
}

async fn run_relay(mode: &str, tunnel_id: &str, ports: &[i32], token: &str) -> Result<()> {
    // Protocol adapter seam. Management-plane migration is complete; this function
    // deliberately fails closed until the Microsoft dev-tunnels SSH framing used by
    // the legacy Go client has a verified Rust interoperability implementation.
    // This is preferable to silently shipping a transport that connects but corrupts
    // forwarding semantics.
    let auth = if token.is_empty() { "api-key" } else { "token" };
    bail!(
        "Rust relay transport is not enabled yet (mode={mode}, tunnel={tunnel_id}, ports={ports:?}, auth={auth}). \
         Management commands are fully migrated; relay interoperability must be validated before release."
    )
}

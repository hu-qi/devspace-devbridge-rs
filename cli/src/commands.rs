use crate::{api::{self, ApiClient}, auth, config, i18n, netutil, output, updater};
use anyhow::{Result, anyhow, bail};
use clap::{Args, Subcommand};
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

#[derive(Debug, Subcommand)]
pub enum Command {
    Auth(AuthArgs),
    #[command(subcommand)]
    Tunnel(TunnelCommand),
    List,
    Create(TunnelCreateArgs),
    Show(TunnelIdArgs),
    Update(TunnelUpdateArgs),
    Delete(TunnelIdArgs),
    DeleteAll,
    Token(TunnelTokenArgs),
    Set(TunnelSetArgs),
    Unset,
    Port(PortArgs),
    Limits,
    Echo(EchoArgs),
    Ping(PingArgs),
    Version,
    Host(HostArgs),
    Connect(ConnectArgs),
}

impl Command {
    pub fn is_version(&self) -> bool {
        matches!(self, Self::Version)
    }
}

#[derive(Debug, Args)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub cmd: AuthCommand,
}

#[derive(Debug, Subcommand)]
pub enum AuthCommand {
    Login {
        #[arg(long)]
        api_key: Option<String>,
    },
    Logout,
    Status,
}

#[derive(Debug, Subcommand)]
pub enum TunnelCommand {
    List,
    Create(TunnelCreateArgs),
    Show(TunnelIdArgs),
    Update(TunnelUpdateArgs),
    Delete(TunnelIdArgs),
    DeleteAll,
    Token(TunnelTokenArgs),
    Set(TunnelSetArgs),
    Unset,
}

#[derive(Debug, Args)]
pub struct TunnelCreateArgs {
    pub name: String,
    #[arg(short, long, default_value = "")]
    pub description: String,
    #[arg(short, long)]
    pub expiration: Option<i32>,
}

#[derive(Debug, Args)]
pub struct TunnelIdArgs {
    pub tunnel_id: Option<String>,
}

#[derive(Debug, Args)]
pub struct TunnelUpdateArgs {
    pub tunnel_id: Option<String>,
    #[arg(short = 'n', long)]
    pub name: Option<String>,
    #[arg(short, long)]
    pub description: Option<String>,
    #[arg(short, long)]
    pub expiration: Option<i32>,
}

#[derive(Debug, Args)]
pub struct TunnelTokenArgs {
    pub tunnel_id: Option<String>,
    #[arg(short = 's', long)]
    pub scope: String,
}

#[derive(Debug, Args)]
pub struct TunnelSetArgs {
    pub tunnel_id: String,
}

#[derive(Debug, Args)]
pub struct PortArgs {
    #[command(subcommand)]
    pub cmd: PortCommand,
}

#[derive(Debug, Subcommand)]
pub enum PortCommand {
    List {
        tunnel_id: Option<String>,
    },
    Create {
        tunnel_id: Option<String>,
        #[arg(short = 'p', long = "port-number", required = true)]
        port: i32,
        #[arg(long, default_value = "auto")]
        protocol: String,
        #[arg(short = 'a', long)]
        allow_anonymous: bool,
        #[arg(long)]
        deny_anonymous: bool,
    },
    Show {
        tunnel_id: Option<String>,
        #[arg(short = 'p', long = "port-number", required = true)]
        port: i32,
    },
    Update {
        tunnel_id: Option<String>,
        #[arg(short = 'p', long = "port-number", required = true)]
        port: i32,
        #[arg(short = 'a', long)]
        allow_anonymous: bool,
        #[arg(long)]
        deny_anonymous: bool,
    },
    Delete {
        tunnel_id: Option<String>,
        #[arg(short = 'p', long = "port-number", required = true)]
        port: i32,
    },
}

#[derive(Debug, Args)]
pub struct EchoArgs {
    #[arg(short, long)]
    pub port: Option<i32>,
    #[arg(short, long, default_value = "127.0.0.1")]
    pub interface: String,
}

#[derive(Debug, Args)]
pub struct PingArgs {
    pub uri: String,
    #[arg(short, long, default_value_t = 1000)]
    pub interval: u64,
}

#[derive(Debug, Args)]
pub struct HostArgs {
    pub tunnel_id: Option<String>,
    #[arg(short = 'p', long = "ports", value_delimiter = ',')]
    pub ports: Option<Vec<i32>>,
    #[arg(short = 'd', long, default_value = "")]
    pub description: String,
    #[arg(short = 'e', long)]
    pub expiration: Option<i32>,
    #[arg(short = 't', long)]
    pub token: Option<String>,
    #[arg(short = 'k', long)]
    pub api_key: Option<String>,
}

#[derive(Debug, Args)]
pub struct ConnectArgs {
    pub tunnel_id: Option<String>,
    #[arg(short = 't', long)]
    pub token: Option<String>,
    #[arg(short = 'k', long)]
    pub api_key: Option<String>,
}

fn tunnel_id(arg: Option<String>) -> Result<String> {
    arg.map(Ok).unwrap_or_else(config::default_tunnel)
}

pub async fn run(
    command: Command,
    api_base: Option<&str>,
    cluster: Option<&str>,
    version: &'static str,
) -> Result<()> {
    match command {
        Command::Auth(args) => run_auth(args, api_base).await,
        Command::Tunnel(command) => run_tunnel(command, api_base, cluster).await,
        Command::List => run_tunnel(TunnelCommand::List, api_base, cluster).await,
        Command::Create(args) => run_tunnel(TunnelCommand::Create(args), api_base, cluster).await,
        Command::Show(args) => run_tunnel(TunnelCommand::Show(args), api_base, cluster).await,
        Command::Update(args) => run_tunnel(TunnelCommand::Update(args), api_base, cluster).await,
        Command::Delete(args) => run_tunnel(TunnelCommand::Delete(args), api_base, cluster).await,
        Command::DeleteAll => run_tunnel(TunnelCommand::DeleteAll, api_base, cluster).await,
        Command::Token(args) => run_tunnel(TunnelCommand::Token(args), api_base, cluster).await,
        Command::Set(args) => run_tunnel(TunnelCommand::Set(args), api_base, cluster).await,
        Command::Unset => run_tunnel(TunnelCommand::Unset, api_base, cluster).await,
        Command::Port(args) => run_port(args, api_base, cluster).await,
        Command::Limits => run_limits(api_base, cluster).await,
        Command::Echo(args) => run_echo(args).await,
        Command::Ping(args) => run_ping(args).await,
        Command::Version => {
            println!("{version}");
            if let Some(result) = updater::check(false).await
                && updater::is_newer(version, &result.latest_version)
            {
                eprintln!(
                    "\nA new version is available: {} (current: {})\nUpdate:\n{}",
                    result.latest_version,
                    version,
                    updater::install_command()
                );
            }
            Ok(())
        }
        Command::Host(args) => crate::transport::host(args, api_base, cluster).await,
        Command::Connect(args) => crate::transport::connect(args, api_base, cluster).await,
    }
}

async fn run_auth(args: AuthArgs, api_base: Option<&str>) -> Result<()> {
    match args.cmd {
        AuthCommand::Login { api_key } => auth::login(api_key, api_base).await,
        AuthCommand::Logout => {
            if let Err(error) = auth::delete_credential() {
                tracing::warn!(%error, "failed to delete credential");
            }
            if let Err(error) = config::set_default_tunnel(None) {
                tracing::warn!(%error, "failed to delete default tunnel");
            }
            println!("{}", i18n::auth::logout_success());
            Ok(())
        }
        AuthCommand::Status => {
            let Ok(key) = auth::read_api_key(None) else {
                println!("{}", i18n::auth::not_logged_in());
                return Ok(());
            };
            if let Err(error) = auth::verify_api_key(&key, api_base).await {
                println!("{}: {error}", i18n::auth::not_logged_in());
                return Ok(());
            }
            println!("{}", i18n::auth::logged_in());
            if let Some(user) = auth::current_user_info()
                && !user.user_name.is_empty()
            {
                println!("{}:  {}", i18n::auth::user_name(), user.user_name);
            }
            Ok(())
        }
    }
}

async fn run_tunnel(
    command: TunnelCommand,
    api_base: Option<&str>,
    cluster: Option<&str>,
) -> Result<()> {
    let client = ApiClient::new(None, api_base, cluster)?;
    match command {
        TunnelCommand::List => {
            let tunnels = client.list_tunnels().await?;
            if tunnels.is_empty() {
                println!("{}", i18n::tunnel::empty());
                return Ok(());
            }
            let rows = tunnels
                .into_iter()
                .map(|t| {
                    vec![
                        t.tunnel_id,
                        t.name,
                        t.description,
                        output::tunnel_remaining(t.tunnel_expiration as i64),
                        t.port_count.to_string(),
                    ]
                })
                .collect::<Vec<_>>();
            output::table(
                &[
                    i18n::tunnel::id(),
                    i18n::tunnel::name(),
                    i18n::tunnel::description(),
                    i18n::tunnel::expiration(),
                    i18n::tunnel::port_count(),
                ],
                &rows,
            );
            Ok(())
        }
        TunnelCommand::Create(args) => {
            let tunnel = client
                .create_tunnel(&args.name, &args.description, args.expiration)
                .await
                .map_err(|e| anyhow!("Failed to create tunnel: {e}"))?;
            output::kv(&[
                (i18n::tunnel::id(), tunnel.tunnel_id),
                (i18n::tunnel::name(), tunnel.name),
                (i18n::tunnel::description(), tunnel.description),
                (
                    i18n::tunnel::expiration(),
                    output::tunnel_expiration(tunnel.expiration_hours as i64),
                ),
            ]);
            Ok(())
        }
        TunnelCommand::Show(args) => {
            let tunnel = client.show_tunnel(&tunnel_id(args.tunnel_id)?).await?;
            let status = tunnel.status.unwrap_or_default();
            output::kv(&[
                (i18n::tunnel::id(), tunnel.tunnel_id),
                (i18n::tunnel::name(), tunnel.name),
                (
                    i18n::tunnel::expiration(),
                    output::tunnel_remaining(tunnel.tunnel_expiration as i64),
                ),
                (i18n::tunnel::description(), tunnel.description),
                (
                    i18n::tunnel::client_connections(),
                    status.client_connection_count.to_string(),
                ),
                (
                    i18n::tunnel::host_connections(),
                    status.host_connection_count.to_string(),
                ),
                (i18n::tunnel::upload(), output::bytes(status.total_upload_bytes)),
                (
                    i18n::tunnel::download(),
                    output::bytes(status.total_download_bytes),
                ),
            ]);
            Ok(())
        }
        TunnelCommand::Update(args) => {
            client
                .update_tunnel(
                    &tunnel_id(args.tunnel_id)?,
                    args.name.as_deref(),
                    args.description.as_deref(),
                    args.expiration,
                )
                .await
                .map_err(|e| anyhow!("Failed to update tunnel: {e}"))?;
            println!("{}", i18n::tunnel::updated());
            Ok(())
        }
        TunnelCommand::Delete(args) => {
            let id = tunnel_id(args.tunnel_id)?;
            client.delete_tunnel(&id).await?;
            if config::load()?.default_tunnel_id.as_deref() == Some(id.as_str()) {
                config::set_default_tunnel(None)?;
                println!("{}", i18n::tunnel::default_cleared());
            }
            println!("{}", i18n::tunnel::deleted());
            Ok(())
        }
        TunnelCommand::DeleteAll => {
            client.delete_all_tunnels().await?;
            println!("{}", i18n::tunnel::deleted_all());
            Ok(())
        }
        TunnelCommand::Token(args) => {
            let token = client.token(&tunnel_id(args.tunnel_id)?, &args.scope).await?;
            output::kv(&[
                (i18n::tunnel::id(), token.tunnel_id),
                (i18n::tunnel::scope(), token.scope),
                (i18n::tunnel::token(), token.token),
            ]);
            Ok(())
        }
        TunnelCommand::Set(args) => {
            api::validate_tunnel_id(&args.tunnel_id)?;
            if let Err(error) = client.show_tunnel(&args.tunnel_id).await {
                if api::api_error_code(&error) == Some(api::TUNNEL_NOT_FOUND_CODE) {
                    bail!("Tunnel not found: {}", args.tunnel_id);
                }
                return Err(error);
            }
            config::set_default_tunnel(Some(args.tunnel_id.clone()))?;
            println!("{}: {}", i18n::tunnel::default_set(), args.tunnel_id);
            Ok(())
        }
        TunnelCommand::Unset => {
            config::set_default_tunnel(None)?;
            println!("{}", i18n::tunnel::default_unset());
            Ok(())
        }
    }
}

async fn run_port(
    args: PortArgs,
    api_base: Option<&str>,
    cluster: Option<&str>,
) -> Result<()> {
    let client = ApiClient::new(None, api_base, cluster)?;
    match args.cmd {
        PortCommand::List { tunnel_id: id } => {
            let ports = client.list_ports(&tunnel_id(id)?).await?;
            if ports.is_empty() {
                println!("{}", i18n::port::empty());
                return Ok(());
            }
            let rows = ports
                .into_iter()
                .map(|p| {
                    vec![
                        p.port.to_string(),
                        p.protocol,
                        p.allow_anonymous.to_string(),
                        p.tunnel_id,
                    ]
                })
                .collect::<Vec<_>>();
            output::table(
                &[
                    i18n::port::port(),
                    i18n::port::protocol(),
                    i18n::port::allow_anonymous(),
                    i18n::tunnel::id(),
                ],
                &rows,
            );
            Ok(())
        }
        PortCommand::Create {
            tunnel_id: id,
            port,
            protocol,
            allow_anonymous,
            deny_anonymous,
        } => {
            if !matches!(protocol.as_str(), "http" | "https" | "auto") {
                bail!("Protocol must be one of http, https, auto, got: {protocol}");
            }
            let allow = if allow_anonymous {
                true
            } else if deny_anonymous {
                false
            } else {
                false
            };
            client
                .create_port(&tunnel_id(id)?, port, &protocol, Some(allow))
                .await
                .map_err(|e| anyhow!("Failed to add port {port}: {e}"))?;
            println!("{}", i18n::port::created());
            Ok(())
        }
        PortCommand::Show {
            tunnel_id: id,
            port,
        } => {
            let p = client.show_port(&tunnel_id(id)?, port).await?;
            output::kv(&[
                (i18n::tunnel::id(), p.tunnel_id),
                (i18n::port::port(), p.port.to_string()),
                (i18n::port::protocol(), p.protocol),
                (
                    i18n::port::allow_anonymous(),
                    p.allow_anonymous.to_string(),
                ),
            ]);
            Ok(())
        }
        PortCommand::Update {
            tunnel_id: id,
            port,
            allow_anonymous,
            deny_anonymous,
        } => {
            let allow = if allow_anonymous {
                true
            } else if deny_anonymous {
                false
            } else {
                false
            };
            client
                .update_port(&tunnel_id(id)?, port, Some(allow))
                .await?;
            println!("{}", i18n::port::updated());
            Ok(())
        }
        PortCommand::Delete {
            tunnel_id: id,
            port,
        } => {
            let id = tunnel_id(id)?;
            client
                .delete_port(&id, port)
                .await
                .map_err(|e| anyhow!("Failed to delete port {port}: {e}"))?;
            println!("Port {port} removed from tunnel {id}.");
            Ok(())
        }
    }
}

async fn run_limits(api_base: Option<&str>, cluster: Option<&str>) -> Result<()> {
    let limits = ApiClient::new(None, api_base, cluster)?.limits().await?;
    let current = limits.quota_bytes - limits.remaining_bytes;
    let current = if limits.quota_bytes > 0 {
        format!(
            "{} ({:.0}%)",
            output::bytes(current),
            current as f64 * 100.0 / limits.quota_bytes as f64
        )
    } else {
        output::bytes(current)
    };
    output::kv(&[
        (i18n::limits::reset_at(), output::time(limits.reset_at)),
        (i18n::limits::quota(), output::bytes(limits.quota_bytes)),
        (i18n::limits::current(), current),
        (i18n::limits::active(), limits.active_tunnels.to_string()),
        (i18n::limits::max_tunnels(), limits.max_tunnels.to_string()),
        (i18n::limits::max_ports(), limits.max_ports_per_tunnel.to_string()),
        (i18n::limits::max_hosts(), limits.max_hosts_per_tunnel.to_string()),
        (
            i18n::limits::bandwidth(),
            format!("{}/s", output::bytes(limits.max_tunnel_bandwidth_bytes_per_second)),
        ),
        (
            i18n::limits::http_rate(),
            limits.max_http_requests_per_minute_per_port.to_string(),
        ),
        (
            i18n::limits::connections(),
            limits.max_connections_per_port.to_string(),
        ),
    ]);
    Ok(())
}

async fn run_echo(args: EchoArgs) -> Result<()> {
    let port = match args.port {
        Some(port) if !(1..=65535).contains(&port) => {
            bail!("Invalid port number {port} (Port must be between 1 and 65535)")
        }
        Some(port) => port as u16,
        None => 0,
    };
    let listener = TcpListener::bind((args.interface.as_str(), port)).await?;
    println!(
        "{} at: http://{}",
        i18n::echo::started(),
        listener.local_addr()?
    );
    loop {
        let (mut socket, peer) = listener.accept().await?;
        tokio::spawn(async move {
            let _ = handle_echo_connection(&mut socket, peer).await;
        });
    }
}

async fn handle_echo_connection(
    socket: &mut tokio::net::TcpStream,
    peer: std::net::SocketAddr,
) -> Result<()> {
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut used = 0usize;
    while used < buffer.len() {
        let n = socket.read(&mut buffer[used..]).await?;
        if n == 0 {
            break;
        }
        used += n;
        if buffer[..used].windows(4).any(|v| v == b"\r\n\r\n") {
            break;
        }
    }
    let request = String::from_utf8_lossy(&buffer[..used]);
    let mut lines = request.lines();
    let first = lines.next().unwrap_or_default();
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let url = parts.next().unwrap_or_default();
    let proto = parts.next().unwrap_or_default();
    let headers = lines
        .take_while(|line| !line.is_empty())
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim(), value.trim()))
        .collect::<Vec<_>>();
    let host = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("host"))
        .map(|(_, value)| *value)
        .unwrap_or_default();

    let mut body = String::new();
    body.push_str(&format!("{}: {method}\n", i18n::echo::method()));
    body.push_str(&format!("{}: {url}\n", i18n::echo::url()));
    body.push_str(&format!("{}: {host}\n", i18n::echo::host()));
    body.push_str(&format!("{}: {peer}\n", i18n::echo::remote_addr()));
    body.push_str(&format!("{}: {proto}\n", i18n::echo::proto()));
    body.push_str(&format!("{}:\n", i18n::echo::headers()));
    for (name, value) in headers {
        body.push_str(&format!("  {name}: [{value}]\n"));
    }

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    socket.write_all(response.as_bytes()).await?;
    Ok(())
}

async fn run_ping(args: PingArgs) -> Result<()> {
    loop {
        let result = netutil::ping_uri(&args.uri, Duration::from_secs(10)).await;
        if let Some(error) = result.error {
            println!(
                "HTTP {} -- {} ms (err: {})",
                result.status_text,
                result.latency.as_millis(),
                error
            );
            return Ok(());
        }
        println!(
            "HTTP {} -- {} ms",
            result.status_text,
            result.latency.as_millis()
        );
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(args.interval)) => {},
            _ = shutdown_signal() => return Ok(()),
        }
    }
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        if let Ok(mut term) = signal(SignalKind::terminate()) {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {},
                _ = term.recv() => {},
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

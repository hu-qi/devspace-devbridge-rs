use std::{io, net::SocketAddr, time::Duration};

use anyhow::{Context, Result, bail};
use axum::{Router, body::Bytes, extract::Request, routing::any};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{Shell, generate};
use comfy_table::{Cell, Table, presets::UTF8_FULL};
use devbridge_core::{
    Settings,
    api::{ALL_PORTS, ApiClient, PortResult},
    auth, config,
};
use devbridge_tunnel::{TunnelAuth, TunnelConfig, connect, host};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(
    name = "devbridge",
    version,
    about = "Huawei DevBridge CLI, rewritten in Rust"
)]
struct Cli {
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Auth {
        #[command(subcommand)]
        command: AuthCommand,
    },
    List,
    Create {
        name: String,
        #[arg(short, long, default_value = "")]
        description: String,
        #[arg(short, long)]
        expiration: Option<i32>,
    },
    Show {
        tunnel_id: Option<String>,
    },
    Update {
        tunnel_id: Option<String>,
        #[arg(short, long)]
        name: Option<String>,
        #[arg(short, long)]
        description: Option<String>,
        #[arg(short, long)]
        expiration: Option<i32>,
    },
    Delete {
        tunnel_id: Option<String>,
    },
    DeleteAll,
    Token {
        tunnel_id: Option<String>,
        #[arg(short, long, value_parser = ["host", "connect"])]
        scope: String,
    },
    Set {
        tunnel_id: String,
    },
    Unset,
    Port {
        #[command(subcommand)]
        command: PortCommand,
    },
    Host {
        tunnel_id: Option<String>,
        #[arg(short = 'p', long, value_delimiter = ',')]
        ports: Vec<i32>,
        #[arg(short, long, default_value = "")]
        description: String,
        #[arg(short, long)]
        expiration: Option<i32>,
        #[arg(short, long)]
        token: Option<String>,
        #[arg(short = 'k', long)]
        api_key: Option<String>,
    },
    Connect {
        tunnel_id: Option<String>,
        #[arg(short, long)]
        token: Option<String>,
        #[arg(short = 'k', long)]
        api_key: Option<String>,
        #[arg(short = 'p', long, value_delimiter = ',')]
        ports: Vec<u16>,
    },
    Limits,
    Echo {
        #[arg(default_value = "127.0.0.1:0")]
        address: SocketAddr,
    },
    Ping {
        uri: String,
        #[arg(short, long, default_value_t = 1000)]
        interval: u64,
    },
    Completions {
        shell: Shell,
    },
    Version,
}

#[derive(Subcommand, Debug)]
enum AuthCommand {
    Login {
        #[arg(long)]
        api_key: Option<String>,
    },
    Logout,
    Status,
}

#[derive(Subcommand, Debug)]
enum PortCommand {
    Create {
        tunnel_id: Option<String>,
        #[arg(short = 'p', long = "port-number")]
        port: i32,
        #[arg(long, default_value = "auto", value_parser = ["http", "https", "auto"])]
        protocol: String,
        #[arg(short = 'a', long, conflicts_with = "deny_anonymous")]
        allow_anonymous: bool,
        #[arg(long, conflicts_with = "allow_anonymous")]
        deny_anonymous: bool,
    },
    List {
        tunnel_id: Option<String>,
    },
    Show {
        tunnel_id: Option<String>,
        #[arg(short = 'p', long = "port-number")]
        port: i32,
    },
    Update {
        tunnel_id: Option<String>,
        #[arg(short = 'p', long = "port-number")]
        port: i32,
        #[arg(short = 'a', long, conflicts_with = "deny_anonymous")]
        allow_anonymous: bool,
        #[arg(long, conflicts_with = "allow_anonymous")]
        deny_anonymous: bool,
    },
    Delete {
        tunnel_id: Option<String>,
        #[arg(short = 'p', long = "port-number")]
        port: i32,
    },
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);
    let settings = Settings::default();

    match cli.command {
        Commands::Auth { command } => run_auth(&settings, command).await,
        Commands::List => list_tunnels(&settings).await,
        Commands::Create {
            name,
            description,
            expiration,
        } => {
            let result = api(&settings, None)?
                .create_tunnel(&name, &description, expiration)
                .await?;
            print_kv(&[
                ("Tunnel ID", result.tunnel_id),
                ("Name", result.name),
                ("Description", result.description),
                ("Expiration hours", result.expiration_hours.to_string()),
            ]);
            Ok(())
        }
        Commands::Show { tunnel_id } => {
            let tunnel_id = resolve_tunnel(tunnel_id)?;
            let item = api(&settings, None)?.show_tunnel(&tunnel_id).await?;
            let status = item.status.unwrap_or_default();
            print_kv(&[
                ("Tunnel ID", item.tunnel_id),
                ("Name", item.name),
                ("Description", item.description),
                ("Expires", item.tunnel_expiration.to_string()),
                ("Clients", status.client_connection_count.to_string()),
                ("Hosts", status.host_connection_count.to_string()),
                ("Upload bytes", status.total_upload_bytes.to_string()),
                ("Download bytes", status.total_download_bytes.to_string()),
            ]);
            Ok(())
        }
        Commands::Update {
            tunnel_id,
            name,
            description,
            expiration,
        } => {
            let tunnel_id = resolve_tunnel(tunnel_id)?;
            api(&settings, None)?
                .update_tunnel(
                    &tunnel_id,
                    name.as_deref(),
                    description.as_deref(),
                    expiration,
                )
                .await?;
            println!("Tunnel updated.");
            Ok(())
        }
        Commands::Delete { tunnel_id } => {
            let tunnel_id = resolve_tunnel(tunnel_id)?;
            api(&settings, None)?.delete_tunnel(&tunnel_id).await?;
            if config::default_tunnel().ok().as_deref() == Some(tunnel_id.as_str()) {
                config::set_default_tunnel(None)?;
            }
            println!("Tunnel deleted.");
            Ok(())
        }
        Commands::DeleteAll => {
            api(&settings, None)?.delete_all_tunnels().await?;
            config::set_default_tunnel(None)?;
            println!("All tunnels deleted.");
            Ok(())
        }
        Commands::Token { tunnel_id, scope } => {
            let tunnel_id = resolve_tunnel(tunnel_id)?;
            let result = api(&settings, None)?
                .tunnel_token(&tunnel_id, &scope)
                .await?;
            print_kv(&[
                ("Tunnel ID", result.tunnel_id),
                ("Scope", result.scope),
                ("Token", result.token),
            ]);
            Ok(())
        }
        Commands::Set { tunnel_id } => {
            api(&settings, None)?.show_tunnel(&tunnel_id).await?;
            config::set_default_tunnel(Some(tunnel_id.clone()))?;
            println!("Default tunnel set: {tunnel_id}");
            Ok(())
        }
        Commands::Unset => {
            config::set_default_tunnel(None)?;
            println!("Default tunnel unset.");
            Ok(())
        }
        Commands::Port { command } => run_port(&settings, command).await,
        Commands::Host {
            tunnel_id,
            ports,
            description,
            expiration,
            token,
            api_key,
        } => {
            run_host(
                &settings,
                tunnel_id,
                ports,
                description,
                expiration,
                token,
                api_key,
            )
            .await
        }
        Commands::Connect {
            tunnel_id,
            token,
            api_key,
            ports,
        } => run_connect(&settings, tunnel_id, token, api_key, ports).await,
        Commands::Limits => show_limits(&settings).await,
        Commands::Echo { address } => run_echo(address).await,
        Commands::Ping { uri, interval } => run_ping(&uri, interval).await,
        Commands::Completions { shell } => {
            let mut command = Cli::command();
            generate(shell, &mut command, "devbridge", &mut io::stdout());
            Ok(())
        }
        Commands::Version => {
            println!("devbridge {}", env!("CARGO_PKG_VERSION"));
            println!("rust {}", env!("CARGO_PKG_RUST_VERSION"));
            println!("release repository: {}", settings.release_repo);
            Ok(())
        }
    }
}

fn api(settings: &Settings, api_key: Option<&str>) -> Result<ApiClient> {
    ApiClient::from_settings(settings, api_key)
}

async fn run_auth(settings: &Settings, command: AuthCommand) -> Result<()> {
    match command {
        AuthCommand::Login { api_key } => {
            let user = match api_key {
                Some(api_key) => auth::login_with_api_key(settings, api_key).await?,
                None => auth::browser_login(settings).await?,
            };
            if user.user_name.is_empty() {
                println!("Login successful.");
            } else {
                println!("Login successful: {}", user.user_name);
            }
            Ok(())
        }
        AuthCommand::Logout => {
            auth::logout()?;
            println!("Logged out.");
            Ok(())
        }
        AuthCommand::Status => {
            let api_key = auth::resolve_api_key(None)?;
            if auth::verify_api_key(settings, &api_key).await? {
                println!("Authenticated.");
                Ok(())
            } else {
                bail!("stored API key is invalid or expired")
            }
        }
    }
}

async fn list_tunnels(settings: &Settings) -> Result<()> {
    let tunnels = api(settings, None)?.list_tunnels().await?;
    if tunnels.is_empty() {
        println!("No tunnels.");
        return Ok(());
    }
    print_table(
        &["Tunnel ID", "Name", "Description", "Expires", "Ports"],
        tunnels.into_iter().map(|item| {
            vec![
                item.tunnel_id,
                item.name,
                item.description,
                item.tunnel_expiration.to_string(),
                item.port_count.to_string(),
            ]
        }),
    );
    Ok(())
}

async fn run_port(settings: &Settings, command: PortCommand) -> Result<()> {
    let api = api(settings, None)?;
    match command {
        PortCommand::Create {
            tunnel_id,
            port,
            protocol,
            allow_anonymous,
            deny_anonymous,
        } => {
            let tunnel_id = resolve_tunnel(tunnel_id)?;
            api.create_port(
                &tunnel_id,
                port,
                &protocol,
                allow_anonymous && !deny_anonymous,
            )
            .await?;
            println!("Port created.");
        }
        PortCommand::List { tunnel_id } => {
            let tunnel_id = resolve_tunnel(tunnel_id)?;
            print_ports(api.list_ports(&tunnel_id).await?);
        }
        PortCommand::Show { tunnel_id, port } => {
            let tunnel_id = resolve_tunnel(tunnel_id)?;
            print_ports(vec![api.show_port(&tunnel_id, port).await?]);
        }
        PortCommand::Update {
            tunnel_id,
            port,
            allow_anonymous,
            deny_anonymous,
        } => {
            let tunnel_id = resolve_tunnel(tunnel_id)?;
            api.update_port(&tunnel_id, port, allow_anonymous && !deny_anonymous)
                .await?;
            println!("Port updated.");
        }
        PortCommand::Delete { tunnel_id, port } => {
            let tunnel_id = resolve_tunnel(tunnel_id)?;
            api.delete_port(&tunnel_id, port).await?;
            println!("Port deleted.");
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run_host(
    settings: &Settings,
    tunnel_id: Option<String>,
    mut ports: Vec<i32>,
    description: String,
    expiration: Option<i32>,
    token: Option<String>,
    explicit_api_key: Option<String>,
) -> Result<()> {
    let tunnel_id = if token.is_some() {
        tunnel_id.context("tunnel ID is required with --token")?
    } else if let Some(tunnel_id) = tunnel_id {
        tunnel_id
    } else if ports.is_empty() {
        config::default_tunnel()?
    } else {
        let api = api(settings, explicit_api_key.as_deref())?;
        let name = format!(
            "tunnel-{}-{}",
            ports.first().copied().unwrap_or(0),
            unix_millis() % 10_000
        );
        let created = api.create_tunnel(&name, &description, expiration).await?;
        for port in &ports {
            api.create_port(&created.tunnel_id, *port, "auto", true)
                .await?;
        }
        println!("Created tunnel: {}", created.tunnel_id);
        created.tunnel_id
    };

    if token.is_none() && ports.is_empty() {
        ports = api(settings, explicit_api_key.as_deref())?
            .list_ports(&tunnel_id)
            .await?
            .into_iter()
            .map(|port| port.port)
            .collect();
    }

    let auth = if let Some(token) = token {
        TunnelAuth::Token(token)
    } else if let Some(api_key) = explicit_api_key {
        TunnelAuth::ApiKey(api_key)
    } else {
        TunnelAuth::Token(
            api(settings, None)?
                .tunnel_token(&tunnel_id, "host")
                .await?
                .token,
        )
    };

    let tcp_ports = ports
        .into_iter()
        .filter(|port| *port != ALL_PORTS)
        .map(|port| u16::try_from(port).context("invalid TCP port"))
        .collect::<Result<Vec<_>>>()?;

    host(TunnelConfig::new(settings, tunnel_id, auth), tcp_ports).await
}

async fn run_connect(
    settings: &Settings,
    tunnel_id: Option<String>,
    token: Option<String>,
    explicit_api_key: Option<String>,
    mut ports: Vec<u16>,
) -> Result<()> {
    let tunnel_id = resolve_tunnel(tunnel_id)?;

    if token.is_none() && ports.is_empty() {
        ports = api(settings, explicit_api_key.as_deref())?
            .list_ports(&tunnel_id)
            .await?
            .into_iter()
            .filter(|item| item.port != ALL_PORTS)
            .map(|item| u16::try_from(item.port).context("invalid TCP port"))
            .collect::<Result<Vec<_>>>()?;
    }
    if token.is_some() && ports.is_empty() {
        bail!("--token mode requires --ports because REST port discovery is intentionally skipped");
    }

    let auth = if let Some(token) = token {
        TunnelAuth::Token(token)
    } else if let Some(api_key) = explicit_api_key {
        TunnelAuth::ApiKey(api_key)
    } else {
        TunnelAuth::Token(
            api(settings, None)?
                .tunnel_token(&tunnel_id, "connect")
                .await?
                .token,
        )
    };

    connect(TunnelConfig::new(settings, tunnel_id, auth), ports).await
}

async fn show_limits(settings: &Settings) -> Result<()> {
    let item = api(settings, None)?.limits().await?;
    print_kv(&[
        ("Remaining bytes", item.remaining_bytes.to_string()),
        ("Quota bytes", item.quota_bytes.to_string()),
        ("Active tunnels", item.active_tunnels.to_string()),
        ("Max tunnels", item.max_tunnels.to_string()),
        ("Max ports/tunnel", item.max_ports_per_tunnel.to_string()),
        ("Max hosts/tunnel", item.max_hosts_per_tunnel.to_string()),
        (
            "Max bandwidth B/s",
            item.max_tunnel_bandwidth_bytes_per_second.to_string(),
        ),
        (
            "Max HTTP requests/min/port",
            item.max_http_requests_per_minute_per_port.to_string(),
        ),
        (
            "Max connections/port",
            item.max_connections_per_port.to_string(),
        ),
    ]);
    Ok(())
}

fn resolve_tunnel(tunnel_id: Option<String>) -> Result<String> {
    match tunnel_id {
        Some(tunnel_id) if !tunnel_id.trim().is_empty() => Ok(tunnel_id),
        _ => config::default_tunnel(),
    }
}

fn print_ports(ports: Vec<PortResult>) {
    if ports.is_empty() {
        println!("No ports.");
        return;
    }
    print_table(
        &["Port", "Protocol", "Anonymous", "Tunnel ID"],
        ports.into_iter().map(|item| {
            vec![
                item.port.to_string(),
                item.protocol,
                item.allow_anonymous.to_string(),
                item.tunnel_id,
            ]
        }),
    );
}

fn print_table<I>(headers: &[&str], rows: I)
where
    I: IntoIterator<Item = Vec<String>>,
{
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(headers.iter().map(|value| Cell::new(*value)));
    for row in rows {
        table.add_row(row);
    }
    println!("{table}");
}

fn print_kv(rows: &[(&str, String)]) {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    for (key, value) in rows {
        table.add_row([*key, value]);
    }
    println!("{table}");
}

async fn run_echo(address: SocketAddr) -> Result<()> {
    async fn handler(request: Request) -> String {
        let method = request.method().clone();
        let uri = request.uri().clone();
        let headers = request.headers().clone();
        let body = axum::body::to_bytes(request.into_body(), 1024 * 1024)
            .await
            .unwrap_or_else(|_| Bytes::new());
        format!(
            "method: {method}\nuri: {uri}\nheaders: {headers:#?}\nbody: {}\n",
            String::from_utf8_lossy(&body)
        )
    }

    let listener = tokio::net::TcpListener::bind(address).await?;
    println!("Echo server listening on http://{}", listener.local_addr()?);
    axum::serve(listener, Router::new().fallback(any(handler))).await?;
    Ok(())
}

async fn run_ping(uri: &str, interval_ms: u64) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;
    let mut ticker = tokio::time::interval(Duration::from_millis(interval_ms.max(50)));

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => return Ok(()),
            _ = ticker.tick() => {
                let started = std::time::Instant::now();
                match client.get(uri).send().await {
                    Ok(response) => println!(
                        "{} {} ms",
                        response.status(),
                        started.elapsed().as_millis()
                    ),
                    Err(error) => println!("ERR {error}"),
                }
            }
        }
    }
}

fn init_tracing(verbose: bool) {
    let fallback = if verbose { "debug" } else { "warn" };
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(fallback)),
        )
        .with_target(false)
        .compact()
        .init();
}

fn unix_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

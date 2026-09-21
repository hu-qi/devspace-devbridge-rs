use crate::{api::ApiClient, config, output};
use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use std::time::Duration;
use tokio::{io::AsyncWriteExt, net::TcpListener};

#[derive(Debug, Subcommand)]
pub enum Command {
    Auth(AuthArgs),
    Tunnel(TunnelArgs),
    Port(PortArgs),
    Limits,
    Echo(EchoArgs),
    Ping(PingArgs),
    Version,
    Host(HostArgs),
    Connect(ConnectArgs),
}

#[derive(Debug, Args)]
pub struct AuthArgs { #[command(subcommand)] pub cmd: AuthCommand }
#[derive(Debug, Subcommand)]
pub enum AuthCommand {
    Login { #[arg(long)] api_key: String },
    Logout,
    Status,
}

#[derive(Debug, Args)]
pub struct TunnelArgs { #[command(subcommand)] pub cmd: TunnelCommand }
#[derive(Debug, Subcommand)]
pub enum TunnelCommand {
    List,
    Create { name: String, #[arg(short, long, default_value="")] description: String, #[arg(short, long)] expiration: Option<i32> },
    Show { tunnel_id: Option<String> },
    Update { tunnel_id: Option<String>, #[arg(long)] name: Option<String>, #[arg(short, long)] description: Option<String>, #[arg(short, long)] expiration: Option<i32> },
    Delete { tunnel_id: Option<String> },
    DeleteAll,
    Token { tunnel_id: Option<String>, #[arg(long, default_value="connect")] scope: String },
    Set { tunnel_id: String },
    Unset,
}

#[derive(Debug, Args)]
pub struct PortArgs { #[command(subcommand)] pub cmd: PortCommand }
#[derive(Debug, Subcommand)]
pub enum PortCommand {
    List { tunnel_id: Option<String> },
    Create { tunnel_id: Option<String>, #[arg(short='p', long="port-number")] port: i32, #[arg(long, default_value="auto")] protocol: String, #[arg(short='a', long)] allow_anonymous: bool },
    Show { tunnel_id: Option<String>, #[arg(short='p', long="port-number")] port: i32 },
    Update { tunnel_id: Option<String>, #[arg(short='p', long="port-number")] port: i32, #[arg(short='a', long)] allow_anonymous: bool, #[arg(long)] deny_anonymous: bool },
    Delete { tunnel_id: Option<String>, #[arg(short='p', long="port-number")] port: i32 },
}

#[derive(Debug, Args)]
pub struct EchoArgs {
    #[arg(short, long, default_value_t=0)] pub port: u16,
    #[arg(short, long, default_value="127.0.0.1")] pub interface: String,
}
#[derive(Debug, Args)]
pub struct PingArgs {
    pub uri: String,
    #[arg(short, long, default_value_t=1000)] pub interval: u64,
}
#[derive(Debug, Args)]
pub struct HostArgs {
    pub tunnel_id: Option<String>,
    #[arg(short='p', long="ports", value_delimiter=',')] pub ports: Vec<i32>,
    #[arg(short='t', long)] pub token: Option<String>,
    #[arg(short='k', long)] pub api_key: Option<String>,
}
#[derive(Debug, Args)]
pub struct ConnectArgs {
    pub tunnel_id: Option<String>,
    #[arg(short='t', long)] pub token: Option<String>,
    #[arg(short='k', long)] pub api_key: Option<String>,
}

fn tunnel_id(arg: Option<String>) -> Result<String> {
    arg.map(Ok).unwrap_or_else(config::default_tunnel)
}

pub async fn run(command: Command, api_base: Option<&str>, cluster: Option<&str>) -> Result<()> {
    match command {
        Command::Auth(a) => match a.cmd {
            AuthCommand::Login { api_key } => {
                let client = ApiClient::new(Some(&api_key), api_base, cluster)?;
                client.verify().await?;
                config::store_api_key(api_key)?;
                println!("Login successful");
            }
            AuthCommand::Logout => { config::clear_auth()?; println!("Logged out"); }
            AuthCommand::Status => {
                match ApiClient::new(None, api_base, cluster) {
                    Ok(client) => match client.verify().await { Ok(_) => println!("Logged in"), Err(e) => println!("Not logged in: {e}") },
                    Err(_) => println!("Not logged in"),
                }
            }
        },
        Command::Tunnel(t) => {
            let client = ApiClient::new(None, api_base, cluster)?;
            match t.cmd {
                TunnelCommand::List => {
                    for t in client.list_tunnels().await? {
                        println!("{}\t{}\t{}\t{}\t{}", t.tunnel_id, t.name, t.description, t.tunnel_expiration, t.port_count);
                    }
                }
                TunnelCommand::Create { name, description, expiration } => {
                    let t = client.create_tunnel(&name, &description, expiration).await?;
                    output::kv(&[("Tunnel ID", t.tunnel_id), ("Name", t.name), ("Description", t.description), ("Expiration hours", t.expiration_hours.to_string())]);
                }
                TunnelCommand::Show { tunnel_id: id } => {
                    let t = client.show_tunnel(&tunnel_id(id)?).await?;
                    output::kv(&[("Tunnel ID", t.tunnel_id), ("Name", t.name), ("Description", t.description), ("Expiration", t.tunnel_expiration.to_string())]);
                }
                TunnelCommand::Update { tunnel_id: id, name, description, expiration } => {
                    client.update_tunnel(&tunnel_id(id)?, name.as_deref(), description.as_deref(), expiration).await?;
                    println!("Tunnel updated");
                }
                TunnelCommand::Delete { tunnel_id: id } => {
                    let id = tunnel_id(id)?;
                    client.delete_tunnel(&id).await?;
                    if config::load()?.default_tunnel_id.as_deref() == Some(&id) { config::set_default_tunnel(None)?; }
                    println!("Tunnel deleted");
                }
                TunnelCommand::DeleteAll => { client.delete_all_tunnels().await?; config::set_default_tunnel(None)?; println!("All tunnels deleted"); }
                TunnelCommand::Token { tunnel_id: id, scope } => {
                    let t = client.token(&tunnel_id(id)?, &scope).await?;
                    output::kv(&[("Tunnel ID", t.tunnel_id), ("Scope", t.scope), ("Token", t.token)]);
                }
                TunnelCommand::Set { tunnel_id } => { client.show_tunnel(&tunnel_id).await?; config::set_default_tunnel(Some(tunnel_id.clone()))?; println!("Default tunnel set: {tunnel_id}"); }
                TunnelCommand::Unset => { config::set_default_tunnel(None)?; println!("Default tunnel unset"); }
            }
        }
        Command::Port(p) => {
            let client = ApiClient::new(None, api_base, cluster)?;
            match p.cmd {
                PortCommand::List { tunnel_id: id } => for p in client.list_ports(&tunnel_id(id)?).await? { println!("{}\t{}\t{}\t{}", p.port, p.protocol, p.allow_anonymous, p.tunnel_id); },
                PortCommand::Create { tunnel_id: id, port, protocol, allow_anonymous } => { client.create_port(&tunnel_id(id)?, port, &protocol, allow_anonymous).await?; println!("Port created"); }
                PortCommand::Show { tunnel_id: id, port } => { let p = client.show_port(&tunnel_id(id)?, port).await?; output::kv(&[("Tunnel ID", p.tunnel_id), ("Port", p.port.to_string()), ("Protocol", p.protocol), ("Allow anonymous", p.allow_anonymous.to_string())]); }
                PortCommand::Update { tunnel_id: id, port, allow_anonymous, deny_anonymous } => {
                    if allow_anonymous && deny_anonymous { bail!("--allow-anonymous and --deny-anonymous are mutually exclusive"); }
                    client.update_port(&tunnel_id(id)?, port, allow_anonymous && !deny_anonymous).await?; println!("Port updated");
                }
                PortCommand::Delete { tunnel_id: id, port } => { client.delete_port(&tunnel_id(id)?, port).await?; println!("Port deleted"); }
            }
        }
        Command::Limits => {
            let l = ApiClient::new(None, api_base, cluster)?.limits().await?;
            let used = l.quota_bytes - l.remaining_bytes;
            output::kv(&[
                ("Quota", output::bytes(l.quota_bytes)), ("Used", output::bytes(used)),
                ("Active tunnels", l.active_tunnels.to_string()), ("Max tunnels", l.max_tunnels.to_string()),
                ("Max ports/tunnel", l.max_ports_per_tunnel.to_string()), ("Max hosts/tunnel", l.max_hosts_per_tunnel.to_string())
            ]);
        }
        Command::Echo(e) => run_echo(e).await?,
        Command::Ping(p) => run_ping(p).await?,
        Command::Version => println!("{}", env!("CARGO_PKG_VERSION")),
        Command::Host(h) => {
            crate::transport::host(h, api_base, cluster).await?;
        }
        Command::Connect(c) => {
            crate::transport::connect(c, api_base, cluster).await?;
        }
    }
    Ok(())
}

async fn run_echo(args: EchoArgs) -> Result<()> {
    let listener = TcpListener::bind((args.interface.as_str(), args.port)).await?;
    println!("Echo server at: http://{}", listener.local_addr()?);
    loop {
        let (mut socket, _) = listener.accept().await?;
        tokio::spawn(async move {
            let body = b"DevBridge echo\n";
            let resp = format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
            let _ = socket.write_all(resp.as_bytes()).await;
            let _ = socket.write_all(body).await;
        });
    }
}

async fn run_ping(args: PingArgs) -> Result<()> {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(10)).build()?;
    loop {
        let start = std::time::Instant::now();
        match client.get(&args.uri).send().await {
            Ok(r) => println!("HTTP {} -- {} ms", r.status(), start.elapsed().as_millis()),
            Err(e) => { println!("HTTP ERROR -- {} ms ({e})", start.elapsed().as_millis()); return Ok(()); }
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(args.interval)) => {},
            _ = tokio::signal::ctrl_c() => return Ok(()),
        }
    }
}

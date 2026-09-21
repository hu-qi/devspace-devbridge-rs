//! DevBridge tunnel data plane.
//!
//! The gateway-facing SSH session intentionally matches the existing DevBridge
//! no-security SSH framing because TLS is the authenticated transport to the
//! gateway. Relay payloads use a separate SSH session for end-to-end channel
//! framing and TCP forwarding.

mod websocket;

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use russh::{ChannelId, CryptoVec, client, server};
use russh_keys::key;
use serde::Deserialize;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, DuplexStream},
    net::{TcpListener, TcpStream},
    sync::{RwLock, mpsc},
};
use tracing::{debug, info, warn};

use devbridge_core::Settings;

const RELAY_CHANNEL: &[u8] = b"relay";

#[derive(Debug, Clone)]
pub enum TunnelAuth {
    Token(String),
    ApiKey(String),
}

#[derive(Debug, Clone)]
pub struct TunnelConfig {
    pub tunnel_id: String,
    pub gateway_addr: String,
    pub gateway_host: String,
    pub auth: TunnelAuth,
}

impl TunnelConfig {
    pub fn new(settings: &Settings, tunnel_id: String, auth: TunnelAuth) -> Self {
        Self {
            tunnel_id,
            gateway_addr: settings.gateway_addr.clone(),
            gateway_host: settings.gateway_host.clone(),
            auth,
        }
    }
}

#[derive(Debug)]
enum OuterChannelOp {
    Open(ChannelId),
    Close(ChannelId),
    Data(ChannelId, Vec<u8>),
}

struct OuterHandler {
    sender: mpsc::UnboundedSender<OuterChannelOp>,
}

impl OuterHandler {
    fn new() -> (Self, mpsc::UnboundedReceiver<OuterChannelOp>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (Self { sender }, receiver)
    }
}

#[russh::async_trait]
impl client::Handler for OuterHandler {
    type Error = russh::Error;

    async fn check_server_key(
        self,
        _server_public_key: &key::PublicKey,
    ) -> Result<(Self, bool), Self::Error> {
        Ok((self, true))
    }

    fn server_channel_handle_unknown(&self, channel: ChannelId, channel_type: &[u8]) -> bool {
        if channel_type == RELAY_CHANNEL {
            let _ = self.sender.send(OuterChannelOp::Open(channel));
            true
        } else {
            false
        }
    }

    async fn channel_close(
        self,
        channel: ChannelId,
        session: client::Session,
    ) -> Result<(Self, client::Session), Self::Error> {
        let _ = self.sender.send(OuterChannelOp::Close(channel));
        Ok((self, session))
    }

    async fn data(
        self,
        channel: ChannelId,
        data: &[u8],
        session: client::Session,
    ) -> Result<(Self, client::Session), Self::Error> {
        let _ = self
            .sender
            .send(OuterChannelOp::Data(channel, data.to_vec()));
        Ok((self, session))
    }
}

fn gateway_ssh_config() -> Arc<client::Config> {
    Arc::new(client::Config {
        anonymous: true,
        window_size: 5 * 1024 * 1024,
        preferred: russh::Preferred {
            kex: &[russh::kex::NONE],
            key: &[russh_keys::key::NONE],
            cipher: &[russh::cipher::NONE],
            mac: russh::Preferred::DEFAULT.mac,
            compression: &["none"],
        },
        limits: russh::Limits {
            rekey_read_limit: usize::MAX,
            rekey_time_limit: Duration::MAX,
            rekey_write_limit: usize::MAX,
        },
        ..Default::default()
    })
}

pub async fn host(config: TunnelConfig, configured_ports: Vec<u16>) -> Result<()> {
    let ws = websocket::dial(&config, true).await?;
    let (handler, mut ops) = OuterHandler::new();
    let outer = client::connect_stream(gateway_ssh_config(), ws, handler)
        .await
        .context("failed to establish gateway SSH session")?;
    let outer = Arc::new(outer);

    let allowed_ports = Arc::new(RwLock::new(
        configured_ports.iter().copied().collect::<HashSet<_>>(),
    ));
    let mut channels: HashMap<ChannelId, tokio::io::WriteHalf<DuplexStream>> = HashMap::new();
    let mut first_relay = true;

    println!("Connected to tunnel: {}", config.tunnel_id);
    print_host_ports(&config, &configured_ports);

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                let _ = outer
                    .disconnect(russh::Disconnect::ByApplication, "shutdown", "en")
                    .await;
                return Ok(());
            }
            op = ops.recv() => {
                let Some(op) = op else {
                    bail!("gateway SSH session closed");
                };
                match op {
                    OuterChannelOp::Open(id) => {
                        let (application, bridge) = tokio::io::duplex(1024 * 1024);
                        let (mut outbound, inbound) = tokio::io::split(bridge);
                        channels.insert(id, inbound);

                        let session = outer.clone();
                        tokio::spawn(async move {
                            let mut buffer = vec![0u8; 64 * 1024];
                            loop {
                                match outbound.read(&mut buffer).await {
                                    Ok(0) | Err(_) => break,
                                    Ok(read) => {
                                        if session
                                            .data(id, CryptoVec::from_slice(&buffer[..read]))
                                            .await
                                            .is_err()
                                        {
                                            break;
                                        }
                                    }
                                }
                            }
                            let _ = session.eof(id).await;
                            let _ = session.close(id).await;
                        });

                        if first_relay {
                            first_relay = false;
                            let ports = allowed_ports.clone();
                            tokio::spawn(async move {
                                if let Err(error) = receive_port_notification(application, ports).await {
                                    warn!(%error, "failed to process gateway port notification");
                                }
                            });
                        } else {
                            let ports = allowed_ports.clone();
                            tokio::spawn(async move {
                                if let Err(error) = serve_relay(application, ports).await {
                                    debug!(%error, "relay session ended");
                                }
                            });
                        }
                    }
                    OuterChannelOp::Data(id, data) => {
                        if let Some(writer) = channels.get_mut(&id)
                            && writer.write_all(&data).await.is_err()
                        {
                            channels.remove(&id);
                        }
                    }
                    OuterChannelOp::Close(id) => {
                        channels.remove(&id);
                    }
                }
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct PortNotification {
    ports: Vec<u16>,
}

async fn receive_port_notification(
    mut stream: DuplexStream,
    allowed_ports: Arc<RwLock<HashSet<u16>>>,
) -> Result<()> {
    let mut data = Vec::new();
    let mut buffer = [0u8; 4096];
    let read = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buffer))
        .await
        .context("port notification timed out")??;
    data.extend_from_slice(&buffer[..read]);

    let notification: PortNotification =
        serde_json::from_slice(&data).context("invalid gateway port notification")?;
    let mut ports = allowed_ports.write().await;
    if ports.is_empty() {
        ports.extend(notification.ports);
    }
    info!(ports = ?*ports, "gateway port notification received");
    Ok(())
}

fn print_host_ports(config: &TunnelConfig, ports: &[u16]) {
    for port in ports {
        println!("Hosting port: {port}");
        println!(
            "Tunnel URL: https://{}-{}.{}",
            config.tunnel_id, port, config.gateway_host
        );
    }
    println!("Ready to accept connections");
}

struct RelayServerHandler {
    allowed_ports: Arc<RwLock<HashSet<u16>>>,
    connections: mpsc::UnboundedSender<ForwardedConnection>,
    senders: HashMap<ChannelId, mpsc::Sender<Vec<u8>>>,
}

impl RelayServerHandler {
    fn new(
        allowed_ports: Arc<RwLock<HashSet<u16>>>,
    ) -> (Self, mpsc::UnboundedReceiver<ForwardedConnection>) {
        let (connections, receiver) = mpsc::unbounded_channel();
        (
            Self {
                allowed_ports,
                connections,
                senders: HashMap::new(),
            },
            receiver,
        )
    }
}

#[russh::async_trait]
impl server::Handler for RelayServerHandler {
    type Error = russh::Error;

    async fn auth_none(self, _user: &str) -> Result<(Self, server::Auth), Self::Error> {
        Ok((self, server::Auth::Accept))
    }

    async fn channel_open_direct_tcpip(
        mut self,
        channel: russh::Channel<server::Msg>,
        _host_to_connect: &str,
        port_to_connect: u32,
        _originator_address: &str,
        _originator_port: u32,
        session: server::Session,
    ) -> Result<(Self, bool, server::Session), Self::Error> {
        let port = u16::try_from(port_to_connect).ok();
        let allowed = if let Some(port) = port {
            self.allowed_ports.read().await.contains(&port)
        } else {
            false
        };
        if !allowed {
            return Ok((self, false, session));
        }

        let (sender, receiver) = mpsc::channel(32);
        let connection = ForwardedConnection {
            port: port.expect("validated above"),
            channel: channel.id(),
            handle: session.handle(),
            receiver,
        };
        if self.connections.send(connection).is_ok() {
            self.senders.insert(channel.id(), sender);
            Ok((self, true, session))
        } else {
            Ok((self, false, session))
        }
    }

    async fn data(
        mut self,
        channel: ChannelId,
        data: &[u8],
        session: server::Session,
    ) -> Result<(Self, server::Session), Self::Error> {
        if let Some(sender) = self.senders.get(&channel)
            && sender.send(data.to_vec()).await.is_err()
        {
            self.senders.remove(&channel);
        }
        Ok((self, session))
    }

    async fn channel_close(
        mut self,
        channel: ChannelId,
        session: server::Session,
    ) -> Result<(Self, server::Session), Self::Error> {
        self.senders.remove(&channel);
        Ok((self, session))
    }
}

struct ForwardedConnection {
    port: u16,
    channel: ChannelId,
    handle: server::Handle,
    receiver: mpsc::Receiver<Vec<u8>>,
}

impl ForwardedConnection {
    async fn send(&mut self, data: &[u8]) -> Result<()> {
        self.handle
            .data(self.channel, CryptoVec::from_slice(data))
            .await
            .map_err(|_| anyhow!("SSH channel closed"))
    }

    async fn close(self) {
        let _ = self.handle.close(self.channel).await;
    }
}

async fn serve_relay(
    stream: DuplexStream,
    allowed_ports: Arc<RwLock<HashSet<u16>>>,
) -> Result<()> {
    let keypair = russh_keys::key::KeyPair::generate_ed25519()
        .context("failed to create relay host key")?;
    let config = Arc::new(server::Config {
        connection_timeout: None,
        auth_rejection_time: Duration::from_millis(10),
        keys: vec![keypair],
        ..Default::default()
    });

    let (handler, mut connections) = RelayServerHandler::new(allowed_ports);
    let session = server::run_stream(config, stream, handler)
        .await
        .context("inner SSH handshake failed")?;
    tokio::pin!(session);

    loop {
        tokio::select! {
            result = &mut session => return result.context("inner SSH session failed"),
            connection = connections.recv() => {
                let Some(connection) = connection else {
                    return Ok(());
                };
                tokio::spawn(async move {
                    if let Err(error) = forward_to_local(connection).await {
                        debug!(%error, "local forwarding ended");
                    }
                });
            }
        }
    }
}

async fn forward_to_local(mut connection: ForwardedConnection) -> Result<()> {
    let mut local = TcpStream::connect(("127.0.0.1", connection.port))
        .await
        .with_context(|| format!("nothing is listening on localhost:{}", connection.port))?;
    let mut buffer = vec![0u8; 64 * 1024];

    loop {
        tokio::select! {
            read = local.read(&mut buffer) => {
                match read? {
                    0 => break,
                    count => connection.send(&buffer[..count]).await?,
                }
            }
            data = connection.receiver.recv() => {
                match data {
                    Some(data) => local.write_all(&data).await?,
                    None => break,
                }
            }
        }
    }

    connection.close().await;
    Ok(())
}

struct RelayClientHandler;

#[russh::async_trait]
impl client::Handler for RelayClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        self,
        _server_public_key: &key::PublicKey,
    ) -> Result<(Self, bool), Self::Error> {
        Ok((self, true))
    }
}

pub async fn connect(config: TunnelConfig, ports: Vec<u16>) -> Result<()> {
    if ports.is_empty() {
        bail!("no TCP ports were supplied for local forwarding");
    }

    let ws = websocket::dial(&config, false).await?;
    let ssh_config = Arc::new(client::Config {
        keepalive_interval: None,
        ..Default::default()
    });
    let mut session = client::connect_stream(ssh_config, ws, RelayClientHandler)
        .await
        .context("failed to establish relay SSH session")?;

    let authenticated = session
        .authenticate_none("devbridge")
        .await
        .context("relay SSH authentication failed")?;
    if !authenticated {
        bail!("relay SSH authentication was rejected");
    }
    let session = Arc::new(session);

    for remote_port in ports {
        start_local_forward(session.clone(), remote_port).await?;
    }

    println!("Connected to tunnel: {}", config.tunnel_id);
    println!("Auto reconnect: enabled");

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                let _ = session
                    .disconnect(russh::Disconnect::ByApplication, "shutdown", "en")
                    .await;
                return Ok(());
            }
            _ = tokio::time::sleep(Duration::from_secs(1)) => {
                if session.is_closed() {
                    bail!("relay SSH session closed");
                }
            }
        }
    }
}

async fn start_local_forward(
    session: Arc<client::Handle<RelayClientHandler>>,
    remote_port: u16,
) -> Result<()> {
    let listener = match TcpListener::bind(("127.0.0.1", remote_port)).await {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            TcpListener::bind(("127.0.0.1", 0))
                .await
                .context("failed to bind fallback local port")?
        }
        Err(error) => return Err(error.into()),
    };
    let local_port = listener.local_addr()?.port();

    if local_port == remote_port {
        println!("Forwarding localhost:{local_port} -> tunnel port:{remote_port}");
    } else {
        println!(
            "Forwarding localhost:{local_port} -> tunnel port:{remote_port} (port {remote_port} in use)"
        );
    }

    tokio::spawn(async move {
        loop {
            let (local, peer) = match listener.accept().await {
                Ok(value) => value,
                Err(error) => {
                    warn!(%error, remote_port, "local listener stopped");
                    return;
                }
            };
            let session = session.clone();
            tokio::spawn(async move {
                if let Err(error) =
                    relay_local_connection(session, local, peer, remote_port).await
                {
                    debug!(%error, remote_port, "forwarded connection ended");
                }
            });
        }
    });
    Ok(())
}

async fn relay_local_connection(
    session: Arc<client::Handle<RelayClientHandler>>,
    mut local: TcpStream,
    peer: std::net::SocketAddr,
    remote_port: u16,
) -> Result<()> {
    let channel = session
        .channel_open_direct_tcpip(
            "127.0.0.1",
            remote_port as u32,
            &peer.ip().to_string(),
            peer.port() as u32,
        )
        .await
        .with_context(|| format!("failed to open tunnel channel for {remote_port}"))?;

    let mut remote = channel.into_stream();
    tokio::io::copy_bidirectional(&mut local, &mut remote).await?;
    Ok(())
}

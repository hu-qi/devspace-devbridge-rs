use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use anyhow::{Context as _, Result, bail};
use bytes::Bytes;
use futures_util::{Sink, Stream};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::TcpStream,
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, client_async_tls,
    tungstenite::{Message, handshake::client::Request},
};

use crate::{TunnelAuth, TunnelConfig};

pub(crate) type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub(crate) async fn dial(config: &TunnelConfig, host_mode: bool) -> Result<WsIo> {
    let sni_host = format!("{}.{}", config.tunnel_id, config.gateway_host);
    let path = if host_mode {
        format!("/{}", config.tunnel_id)
    } else {
        "/".to_string()
    };
    let uri = format!("wss://{sni_host}{path}");

    let mut request = Request::builder()
        .method("GET")
        .uri(&uri)
        .header("Host", &sni_host)
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header(
            "Sec-WebSocket-Key",
            tokio_tungstenite::tungstenite::handshake::client::generate_key(),
        );

    match &config.auth {
        TunnelAuth::Token(token) => {
            request = request.header(
                "Sec-WebSocket-Protocol",
                format!("devbridge-v1, {token}"),
            );
        }
        TunnelAuth::ApiKey(api_key) => {
            request = request
                .header("Sec-WebSocket-Protocol", "devbridge-v1")
                .header("X-API-Key", api_key);
        }
    }
    if host_mode {
        request = request.header("Cookie", "APP_COOKIE=7");
    }

    let request = request.body(())?;
    let tcp = TcpStream::connect(&config.gateway_addr)
        .await
        .with_context(|| format!("failed to connect gateway {}", config.gateway_addr))?;
    tcp.set_nodelay(true)?;

    let (socket, response) = client_async_tls(request, tcp)
        .await
        .with_context(|| format!("WebSocket handshake failed for {uri}"))?;
    if response.status().as_u16() != 101 {
        bail!("gateway rejected WebSocket upgrade: {}", response.status());
    }

    Ok(WsIo::new(socket))
}

pub(crate) struct WsIo {
    socket: Socket,
    pending: Bytes,
}

impl WsIo {
    fn new(socket: Socket) -> Self {
        Self {
            socket,
            pending: Bytes::new(),
        }
    }
}

impl AsyncRead for WsIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if !self.pending.is_empty() {
            let len = self.pending.len().min(buf.remaining());
            let chunk = self.pending.split_to(len);
            buf.put_slice(&chunk);
            return Poll::Ready(Ok(()));
        }

        loop {
            match Pin::new(&mut self.socket).poll_next(cx) {
                Poll::Ready(Some(Ok(Message::Binary(data)))) => {
                    self.pending = data;
                    let len = self.pending.len().min(buf.remaining());
                    let chunk = self.pending.split_to(len);
                    buf.put_slice(&chunk);
                    return Poll::Ready(Ok(()));
                }
                Poll::Ready(Some(Ok(Message::Text(text)))) => {
                    self.pending = Bytes::copy_from_slice(text.as_bytes());
                    let len = self.pending.len().min(buf.remaining());
                    let chunk = self.pending.split_to(len);
                    buf.put_slice(&chunk);
                    return Poll::Ready(Ok(()));
                }
                Poll::Ready(Some(Ok(Message::Close(_)))) | Poll::Ready(None) => {
                    return Poll::Ready(Ok(()));
                }
                Poll::Ready(Some(Ok(_))) => continue,
                Poll::Ready(Some(Err(error))) => {
                    return Poll::Ready(Err(io::Error::other(error)));
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

impl AsyncWrite for WsIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match Pin::new(&mut self.socket).poll_ready(cx) {
            Poll::Ready(Ok(())) => {
                Pin::new(&mut self.socket)
                    .start_send(Message::Binary(Bytes::copy_from_slice(buf)))
                    .map_err(io::Error::other)?;
                Poll::Ready(Ok(buf.len()))
            }
            Poll::Ready(Err(error)) => Poll::Ready(Err(io::Error::other(error))),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.socket)
            .poll_flush(cx)
            .map_err(io::Error::other)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.socket)
            .poll_close(cx)
            .map_err(io::Error::other)
    }
}

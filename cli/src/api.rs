use crate::config::{DEFAULT_CLUSTER_ID, DEFAULT_SERVER_DOMAIN, api_key};
use anyhow::{Context, Result, anyhow, bail};
use reqwest::{Client, Method, StatusCode};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::time::Duration;

#[derive(Clone)]
pub struct ApiClient {
    http: Client,
    base: String,
    key: String,
    cluster_id: String,
}

#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    error: Option<ApiErrorBody>,
}
#[derive(Debug, Deserialize)]
struct ApiErrorBody {
    code: String,
    message: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    pub reset_at: i64,
    pub quota_bytes: i64,
    pub remaining_bytes: i64,
    pub active_tunnels: i64,
    pub max_tunnels: i32,
    pub max_ports_per_tunnel: i32,
    pub max_hosts_per_tunnel: i32,
    pub max_tunnel_bandwidth_bytes_per_second: i64,
    pub max_http_requests_per_minute_per_port: i32,
    pub max_connections_per_port: i32,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelSummary {
    pub name: String,
    pub tunnel_id: String,
    pub tunnel_expiration: u32,
    pub description: String,
    pub port_count: i32,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelStatus {
    pub client_connection_count: i32,
    pub host_connection_count: i32,
    pub total_upload_bytes: i64,
    pub total_download_bytes: i64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tunnel {
    pub name: String,
    pub tunnel_id: String,
    pub tunnel_expiration: u32,
    pub description: String,
    pub status: Option<TunnelStatus>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedTunnel {
    pub tunnel_id: String,
    pub name: String,
    pub description: String,
    pub expiration_hours: i32,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Port {
    pub port: i32,
    pub protocol: String,
    pub allow_anonymous: bool,
    pub tunnel_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelToken {
    pub tunnel_id: String,
    pub scope: String,
    pub token: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateTunnel<'a> {
    name: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    description: &'a str,
    #[serde(rename = "ClusterId")]
    cluster_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    expiration: Option<i32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateTunnel<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expiration: Option<i32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PortRequest<'a> {
    port: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    protocol: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    allow_anonymous: Option<bool>,
}

impl ApiClient {
    pub fn new(
        key_override: Option<&str>,
        base_override: Option<&str>,
        cluster_override: Option<&str>,
    ) -> Result<Self> {
        let key = api_key(key_override)?;
        let base_domain = base_override
            .unwrap_or(DEFAULT_SERVER_DOMAIN)
            .trim_end_matches('/');
        let http = Client::builder().timeout(Duration::from_secs(30)).build()?;
        Ok(Self {
            http,
            base: format!("{base_domain}/open-api-inner/v1/relay-controller"),
            key,
            cluster_id: cluster_override.unwrap_or(DEFAULT_CLUSTER_ID).to_owned(),
        })
    }

    async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<T> {
        let url = format!("{}{}", self.base, path);
        let mut req = self
            .http
            .request(method, &url)
            .header("X-API-Key", &self.key);
        if let Some(body) = body {
            req = req.json(&body);
        }
        let resp = req.send().await.with_context(|| format!("request {url}"))?;
        let status = resp.status();
        let bytes = resp.bytes().await?;
        if !status.is_success() {
            if let Ok(env) = serde_json::from_slice::<ErrorEnvelope>(&bytes) {
                if let Some(e) = env.error {
                    bail!("API {}: {}", e.code, e.message);
                }
            }
            bail!(
                "API request failed: HTTP {}: {}",
                status,
                String::from_utf8_lossy(&bytes)
            );
        }
        if bytes.is_empty() {
            return serde_json::from_value(Value::Null).map_err(Into::into);
        }
        serde_json::from_slice(&bytes).context("decode API response")
    }

    async fn empty(&self, method: Method, path: &str, body: Option<Value>) -> Result<()> {
        let url = format!("{}{}", self.base, path);
        let mut req = self
            .http
            .request(method, &url)
            .header("X-API-Key", &self.key);
        if let Some(body) = body {
            req = req.json(&body);
        }
        let resp = req.send().await?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            bail!("API request failed: HTTP {}: {}", status, text);
        }
        Ok(())
    }

    pub async fn verify(&self) -> Result<()> {
        let url = format!("{}/auth/check", self.base);
        let resp = self
            .http
            .get(url)
            .header("X-API-Key", &self.key)
            .send()
            .await?;
        match resp.status() {
            StatusCode::NO_CONTENT => Ok(()),
            StatusCode::UNAUTHORIZED => bail!("API key is invalid or disabled"),
            status => bail!("verify API key: unexpected HTTP {status}"),
        }
    }

    pub async fn limits(&self) -> Result<Limits> {
        self.request(Method::GET, "/limits", None).await
    }
    pub async fn list_tunnels(&self) -> Result<Vec<TunnelSummary>> {
        self.request(Method::GET, "/tunnels", None).await
    }
    pub async fn show_tunnel(&self, id: &str) -> Result<Tunnel> {
        validate_tunnel_id(id)?;
        self.request(Method::GET, &format!("/tunnels/{id}"), None)
            .await
    }
    pub async fn create_tunnel(
        &self,
        name: &str,
        desc: &str,
        exp: Option<i32>,
    ) -> Result<CreatedTunnel> {
        validate_name(name)?;
        if let Some(v) = exp {
            if !(1..=720).contains(&v) {
                bail!("expiration must be 1..=720 hours");
            }
        }
        let payload = serde_json::to_value(CreateTunnel {
            name,
            description: desc,
            cluster_id: &self.cluster_id,
            expiration: exp,
        })?;
        self.request(Method::POST, "/tunnels", Some(payload)).await
    }
    pub async fn update_tunnel(
        &self,
        id: &str,
        name: Option<&str>,
        desc: Option<&str>,
        exp: Option<i32>,
    ) -> Result<()> {
        validate_tunnel_id(id)?;
        if let Some(v) = name {
            validate_name(v)?;
        }
        if let Some(v) = exp {
            if !(1..=720).contains(&v) {
                bail!("expiration must be 1..=720 hours");
            }
        }
        let payload = serde_json::to_value(UpdateTunnel {
            name,
            description: desc,
            expiration: exp,
        })?;
        self.empty(Method::PUT, &format!("/tunnels/{id}"), Some(payload))
            .await
    }
    pub async fn delete_tunnel(&self, id: &str) -> Result<()> {
        validate_tunnel_id(id)?;
        self.empty(Method::DELETE, &format!("/tunnels/{id}"), None)
            .await
    }
    pub async fn delete_all_tunnels(&self) -> Result<()> {
        self.empty(Method::DELETE, "/tunnels", None).await
    }
    pub async fn token(&self, id: &str, scope: &str) -> Result<TunnelToken> {
        validate_tunnel_id(id)?;
        if scope != "host" && scope != "connect" {
            bail!("scope must be host or connect");
        }
        self.request(
            Method::POST,
            &format!("/tunnels/{id}/token?scope={scope}"),
            Some(Value::Null),
        )
        .await
    }
    pub async fn list_ports(&self, id: &str) -> Result<Vec<Port>> {
        validate_tunnel_id(id)?;
        self.request(Method::GET, &format!("/tunnels/{id}/ports"), None)
            .await
    }
    pub async fn show_port(&self, id: &str, port: i32) -> Result<Port> {
        validate_port(port)?;
        self.request(Method::GET, &format!("/tunnels/{id}/ports/{port}"), None)
            .await
    }
    pub async fn create_port(
        &self,
        id: &str,
        port: i32,
        protocol: &str,
        allow: bool,
    ) -> Result<()> {
        validate_tunnel_id(id)?;
        validate_port(port)?;
        validate_protocol(protocol)?;
        let payload = serde_json::to_value(PortRequest {
            port,
            protocol: Some(protocol),
            allow_anonymous: Some(allow),
        })?;
        self.empty(Method::POST, &format!("/tunnels/{id}/ports"), Some(payload))
            .await
    }
    pub async fn update_port(&self, id: &str, port: i32, allow: bool) -> Result<()> {
        validate_tunnel_id(id)?;
        validate_port(port)?;
        let payload = serde_json::json!({"allowAnonymous": allow});
        self.empty(
            Method::PUT,
            &format!("/tunnels/{id}/ports/{port}"),
            Some(payload),
        )
        .await
    }
    pub async fn delete_port(&self, id: &str, port: i32) -> Result<()> {
        validate_tunnel_id(id)?;
        validate_port(port)?;
        self.empty(Method::DELETE, &format!("/tunnels/{id}/ports/{port}"), None)
            .await
    }
}

pub fn validate_tunnel_id(id: &str) -> Result<()> {
    if id.len() != 8 || !id.bytes().all(|b| matches!(b, b'a'..=b'z' | b'2'..=b'7')) {
        return Err(anyhow!(
            "invalid tunnel id {id:?}: expected 8 lowercase base32 characters"
        ));
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() || name.chars().count() > 64 {
        bail!("tunnel name must contain 1..=64 characters");
    }
    Ok(())
}

fn validate_port(port: i32) -> Result<()> {
    if port == -1 || (1..=65535).contains(&port) {
        Ok(())
    } else {
        bail!("port must be 1..=65535, or -1 for all ports")
    }
}

fn validate_protocol(protocol: &str) -> Result<()> {
    if matches!(protocol, "http" | "https" | "auto") {
        Ok(())
    } else {
        bail!("protocol must be http, https or auto")
    }
}

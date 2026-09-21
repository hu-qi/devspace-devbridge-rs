use crate::{auth, config::{DEFAULT_CLUSTER_ID, DEFAULT_SERVER_DOMAIN}, i18n};
use anyhow::{Context, Result, anyhow, bail};
use regex::Regex;
use reqwest::{Client, Method};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::{fmt, sync::LazyLock, time::Duration};

pub const TUNNEL_NOT_FOUND_CODE: &str = "10002";
pub const ALL_PORTS_SENTINEL: i32 = -1;

static TUNNEL_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z2-7]{8}$").expect("valid tunnel id regex"));
static TUNNEL_NAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[\u{4e00}-\u{9fa5}A-Za-z0-9]([\u{4e00}-\u{9fa5}A-Za-z0-9-]{0,62}[\u{4e00}-\u{9fa5}A-Za-z0-9])?$")
        .expect("valid tunnel name regex")
});
static TUNNEL_DESC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[\u{4e00}-\u{9fa5}A-Za-z0-9]{0,64}$").expect("valid tunnel description regex"));

#[derive(Clone)]
pub struct ApiClient {
    http: Client,
    base: String,
    key: String,
    cluster_id: String,
}

#[derive(Debug)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error code: {}, error message: {}", self.code, self.message)
    }
}
impl std::error::Error for ApiError {}

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

#[derive(Debug, Default, Deserialize, Serialize)]
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
    #[serde(skip_serializing_if = "str::is_empty")]
    protocol: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    allow_anonymous: Option<bool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdatePortRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    allow_anonymous: Option<bool>,
}

impl ApiClient {
    pub fn new(
        key_override: Option<&str>,
        base_override: Option<&str>,
        cluster_override: Option<&str>,
    ) -> Result<Self> {
        let key = auth::read_api_key(key_override)?;
        let base_domain = base_override
            .unwrap_or(DEFAULT_SERVER_DOMAIN)
            .trim_end_matches('/');
        let http = Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(Duration::from_secs(30))
            .build()?;
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
            .header("X-API-Key", &self.key)
            .header("content-type", "application/json");
        if let Some(body) = body {
            req = req.json(&body);
        }
        tracing::debug!(method = %req.method(), url = %url, "HTTP request");
        let start = std::time::Instant::now();
        let response = req.send().await.with_context(|| format!("request {url}"))?;
        let status = response.status();
        let bytes = response.bytes().await?;
        tracing::debug!(status = status.as_u16(), elapsed = start.elapsed().as_millis(), size = bytes.len(), "HTTP response");

        if status.as_u16() == 401 {
            let text = String::from_utf8_lossy(&bytes);
            if text.contains("APIGW.0301") {
                bail!("{}: {}", i18n::api::expired(), text);
            }
            if let Some(error) = parse_api_error(&bytes) {
                return Err(anyhow!("{}: {}", i18n::api::unauthorized(), error));
            }
            bail!("{}: {}", i18n::api::unauthorized(), text);
        }
        if !status.is_success() {
            if let Some(error) = parse_api_error(&bytes) {
                return Err(error.into());
            }
            bail!(
                "{}: status={} body={}",
                i18n::api::server_error(),
                status.as_u16(),
                String::from_utf8_lossy(&bytes)
            );
        }
        if bytes.is_empty() {
            return serde_json::from_value(Value::Null).map_err(Into::into);
        }
        serde_json::from_slice(&bytes)
            .map_err(|error| anyhow!("{}: {error}", i18n::api::invalid_response()))
    }

    async fn empty(&self, method: Method, path: &str, body: Option<Value>) -> Result<()> {
        let _: Value = self.request(method, path, body).await.or_else(|error| {
            // Legacy endpoints commonly return an empty 2xx response. request::<Value>
            // cannot deserialize an empty body, so issue the request inline in that case.
            if error.to_string().contains("EOF while parsing") {
                Ok(Value::Null)
            } else {
                Err(error)
            }
        })?;
        Ok(())
    }

    pub async fn verify(&self) -> Result<()> {
        auth::verify_api_key(&self.key, self.base_domain_override()).await
    }

    fn base_domain_override(&self) -> Option<&str> {
        self.base.strip_suffix("/open-api-inner/v1/relay-controller")
    }

    pub async fn limits(&self) -> Result<Limits> {
        self.request(Method::GET, "/limits", None).await
    }

    pub async fn list_tunnels(&self) -> Result<Vec<TunnelSummary>> {
        self.request(Method::GET, "/tunnels", None).await
    }

    pub async fn show_tunnel(&self, id: &str) -> Result<Tunnel> {
        validate_tunnel_id(id)?;
        self.request(Method::GET, &format!("/tunnels/{id}"), None).await
    }

    pub async fn create_tunnel(
        &self,
        name: &str,
        description: &str,
        expiration: Option<i32>,
    ) -> Result<CreatedTunnel> {
        validate_tunnel_name(name)?;
        validate_tunnel_description(description)?;
        validate_expiration(expiration)?;
        let body = serde_json::to_value(CreateTunnel {
            name,
            description,
            cluster_id: &self.cluster_id,
            expiration,
        })?;
        self.request(Method::POST, "/tunnels", Some(body)).await
    }

    pub async fn update_tunnel(
        &self,
        id: &str,
        name: Option<&str>,
        description: Option<&str>,
        expiration: Option<i32>,
    ) -> Result<()> {
        validate_tunnel_id(id)?;
        if let Some(name) = name {
            validate_tunnel_name(name)?;
        }
        if let Some(description) = description {
            validate_tunnel_description(description)?;
        }
        validate_expiration(expiration)?;
        let body = serde_json::to_value(UpdateTunnel {
            name,
            description,
            expiration,
        })?;
        self.request_empty(Method::PUT, &format!("/tunnels/{id}"), Some(body)).await
    }

    pub async fn delete_tunnel(&self, id: &str) -> Result<()> {
        validate_tunnel_id(id)?;
        self.request_empty(Method::DELETE, &format!("/tunnels/{id}"), None).await
    }

    pub async fn delete_all_tunnels(&self) -> Result<()> {
        self.request_empty(Method::DELETE, "/tunnels", None).await
    }

    pub async fn token(&self, id: &str, scope: &str) -> Result<TunnelToken> {
        validate_tunnel_id(id)?;
        if !matches!(scope, "host" | "connect") {
            bail!("invalid token scope: {scope:?} (scope must be one of host, connect)");
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
        self.request(Method::GET, &format!("/tunnels/{id}/ports"), None).await
    }

    pub async fn show_port(&self, id: &str, port: i32) -> Result<Port> {
        validate_tunnel_id(id)?;
        validate_port(port)?;
        self.request(Method::GET, &format!("/tunnels/{id}/ports/{port}"), None).await
    }

    pub async fn create_port(
        &self,
        id: &str,
        port: i32,
        protocol: &str,
        allow_anonymous: Option<bool>,
    ) -> Result<()> {
        validate_tunnel_id(id)?;
        validate_port(port)?;
        validate_protocol(protocol)?;
        let body = serde_json::to_value(PortRequest {
            port,
            protocol,
            allow_anonymous,
        })?;
        self.request_empty(Method::POST, &format!("/tunnels/{id}/ports"), Some(body)).await
    }

    pub async fn update_port(
        &self,
        id: &str,
        port: i32,
        allow_anonymous: Option<bool>,
    ) -> Result<()> {
        validate_tunnel_id(id)?;
        validate_port(port)?;
        let body = serde_json::to_value(UpdatePortRequest { allow_anonymous })?;
        self.request_empty(
            Method::PUT,
            &format!("/tunnels/{id}/ports/{port}"),
            Some(body),
        )
        .await
    }

    pub async fn delete_port(&self, id: &str, port: i32) -> Result<()> {
        validate_tunnel_id(id)?;
        validate_port(port)?;
        self.request_empty(Method::DELETE, &format!("/tunnels/{id}/ports/{port}"), None).await
    }

    async fn request_empty(&self, method: Method, path: &str, body: Option<Value>) -> Result<()> {
        let url = format!("{}{}", self.base, path);
        let mut req = self
            .http
            .request(method, &url)
            .header("X-API-Key", &self.key)
            .header("content-type", "application/json");
        if let Some(body) = body {
            req = req.json(&body);
        }
        let response = req.send().await?;
        let status = response.status();
        let bytes = response.bytes().await?;
        if status.as_u16() == 401 {
            let text = String::from_utf8_lossy(&bytes);
            if text.contains("APIGW.0301") {
                bail!("{}: {}", i18n::api::expired(), text);
            }
            if let Some(error) = parse_api_error(&bytes) {
                return Err(anyhow!("{}: {}", i18n::api::unauthorized(), error));
            }
            bail!("{}: {}", i18n::api::unauthorized(), text);
        }
        if !status.is_success() {
            if let Some(error) = parse_api_error(&bytes) {
                return Err(error.into());
            }
            bail!(
                "{}: status={} body={}",
                i18n::api::server_error(),
                status.as_u16(),
                String::from_utf8_lossy(&bytes)
            );
        }
        Ok(())
    }
}

fn parse_api_error(body: &[u8]) -> Option<ApiError> {
    let env: ErrorEnvelope = serde_json::from_slice(body).ok()?;
    let error = env.error?;
    Some(ApiError {
        code: error.code,
        message: error.message,
    })
}

pub fn api_error_code(error: &anyhow::Error) -> Option<&str> {
    error.downcast_ref::<ApiError>().map(|v| v.code.as_str())
}

pub fn validate_tunnel_id(id: &str) -> Result<()> {
    if TUNNEL_ID.is_match(id) {
        Ok(())
    } else {
        bail!(
            "invalid tunnel id: {id:?} (only lowercase letters and digits 2-7 allowed, length must be 8)"
        )
    }
}

pub fn validate_data_tunnel_id(id: &str) -> Result<()> {
    if !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        Ok(())
    } else if id.is_empty() {
        bail!("tunnel ID cannot be empty")
    } else {
        bail!(
            "invalid tunnel ID: {id:?} (only letters, digits, hyphens, underscores allowed, length 1-64)"
        )
    }
}

fn validate_tunnel_name(name: &str) -> Result<()> {
    if TUNNEL_NAME.is_match(name) {
        Ok(())
    } else {
        bail!("{}", i18n::tunnel::invalid_name())
    }
}

fn validate_tunnel_description(description: &str) -> Result<()> {
    if TUNNEL_DESC.is_match(description) {
        Ok(())
    } else {
        bail!("{}", i18n::tunnel::invalid_desc())
    }
}

fn validate_expiration(expiration: Option<i32>) -> Result<()> {
    if expiration.is_some_and(|v| !(1..=720).contains(&v)) {
        bail!("{}", i18n::tunnel::invalid_exp())
    }
    Ok(())
}

pub fn validate_port(port: i32) -> Result<()> {
    if port == ALL_PORTS_SENTINEL || (1..=65535).contains(&port) {
        Ok(())
    } else {
        bail!("{}", i18n::port::invalid())
    }
}

pub fn validate_protocol(protocol: &str) -> Result<()> {
    if matches!(protocol, "" | "http" | "https" | "auto") {
        Ok(())
    } else {
        bail!("{}: {protocol}", i18n::port::protocol_invalid())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_legacy_validation() {
        assert!(validate_tunnel_id("abcd2345").is_ok());
        assert!(validate_tunnel_id("ABC12345").is_err());
        assert!(validate_data_tunnel_id("ABC_123-x").is_ok());
        assert!(validate_port(-1).is_ok());
        assert!(validate_port(0).is_err());
        assert!(validate_protocol("").is_ok());
    }
}

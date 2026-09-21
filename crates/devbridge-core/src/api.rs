use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use reqwest::{Method, RequestBuilder};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use thiserror::Error;

use crate::{auth, settings::Settings};

pub const ALL_PORTS: i32 = -1;
pub const TUNNEL_NOT_FOUND_CODE: &str = "10002";

#[derive(Debug, Error)]
#[error("error code: {code}, error message: {message}")]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(Debug, Deserialize)]
struct ErrorDetail {
    code: String,
    message: String,
}

#[derive(Clone)]
pub struct ApiClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    cluster_id: String,
}

impl ApiClient {
    pub fn from_settings(settings: &Settings, explicit_api_key: Option<&str>) -> Result<Self> {
        let api_key = auth::resolve_api_key(explicit_api_key)?;
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .danger_accept_invalid_certs(settings.insecure_tls)
            .build()?;
        Ok(Self {
            http,
            base_url: settings.api_base_url(),
            api_key,
            cluster_id: settings.cluster_id.clone(),
        })
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        self.http
            .request(method, format!("{}{}", self.base_url, path))
            .header("X-API-Key", &self.api_key)
            .header("content-type", "application/json")
    }

    async fn json<T: DeserializeOwned>(&self, request: RequestBuilder) -> Result<T> {
        let response = request.send().await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            return Err(parse_api_error(status.as_u16(), &body));
        }
        serde_json::from_str(&body)
            .map_err(|error| anyhow!("invalid API response: {error}; body={body}"))
    }

    async fn empty(&self, request: RequestBuilder) -> Result<()> {
        let response = request.send().await?;
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }
        let body = response.text().await.unwrap_or_default();
        Err(parse_api_error(status.as_u16(), &body))
    }

    pub async fn list_tunnels(&self) -> Result<Vec<ListTunnelResult>> {
        self.json(self.request(Method::GET, "/tunnels")).await
    }

    pub async fn create_tunnel(
        &self,
        name: &str,
        description: &str,
        expiration: Option<i32>,
    ) -> Result<CreateTunnelResult> {
        validate_tunnel_name(name)?;
        validate_tunnel_description(description)?;
        if let Some(expiration) = expiration {
            validate_expiration(expiration)?;
        }
        let request = CreateTunnelRequest {
            name,
            description,
            cluster_id: &self.cluster_id,
            expiration,
        };
        self.json(self.request(Method::POST, "/tunnels").json(&request))
            .await
    }

    pub async fn show_tunnel(&self, tunnel_id: &str) -> Result<ShowTunnelResult> {
        validate_tunnel_id(tunnel_id)?;
        self.json(self.request(Method::GET, &format!("/tunnels/{tunnel_id}")))
            .await
    }

    pub async fn update_tunnel(
        &self,
        tunnel_id: &str,
        name: Option<&str>,
        description: Option<&str>,
        expiration: Option<i32>,
    ) -> Result<()> {
        validate_tunnel_id(tunnel_id)?;
        if let Some(name) = name {
            validate_tunnel_name(name)?;
        }
        if let Some(description) = description {
            validate_tunnel_description(description)?;
        }
        if let Some(expiration) = expiration {
            validate_expiration(expiration)?;
        }
        self.empty(
            self.request(Method::PUT, &format!("/tunnels/{tunnel_id}"))
                .json(&UpdateTunnelRequest {
                    name,
                    description,
                    expiration,
                }),
        )
        .await
    }

    pub async fn delete_tunnel(&self, tunnel_id: &str) -> Result<()> {
        validate_tunnel_id(tunnel_id)?;
        self.empty(self.request(Method::DELETE, &format!("/tunnels/{tunnel_id}")))
            .await
    }

    pub async fn delete_all_tunnels(&self) -> Result<()> {
        self.empty(self.request(Method::DELETE, "/tunnels")).await
    }

    pub async fn tunnel_token(&self, tunnel_id: &str, scope: &str) -> Result<TunnelTokenResult> {
        validate_tunnel_id(tunnel_id)?;
        if !matches!(scope, "host" | "connect") {
            bail!("scope must be one of: host, connect");
        }
        self.json(
            self.request(
                Method::POST,
                &format!("/tunnels/{tunnel_id}/token?scope={scope}"),
            )
            .json(&serde_json::Value::Null),
        )
        .await
    }

    pub async fn list_ports(&self, tunnel_id: &str) -> Result<Vec<PortResult>> {
        validate_tunnel_id(tunnel_id)?;
        self.json(self.request(Method::GET, &format!("/tunnels/{tunnel_id}/ports")))
            .await
    }

    pub async fn show_port(&self, tunnel_id: &str, port: i32) -> Result<PortResult> {
        validate_tunnel_id(tunnel_id)?;
        validate_port(port)?;
        self.json(self.request(
            Method::GET,
            &format!("/tunnels/{tunnel_id}/ports/{port}"),
        ))
        .await
    }

    pub async fn create_port(
        &self,
        tunnel_id: &str,
        port: i32,
        protocol: &str,
        allow_anonymous: bool,
    ) -> Result<()> {
        validate_tunnel_id(tunnel_id)?;
        validate_port(port)?;
        validate_protocol(protocol)?;
        self.empty(
            self.request(Method::POST, &format!("/tunnels/{tunnel_id}/ports"))
                .json(&PortRequest {
                    port,
                    protocol,
                    allow_anonymous,
                }),
        )
        .await
    }

    pub async fn update_port(
        &self,
        tunnel_id: &str,
        port: i32,
        allow_anonymous: bool,
    ) -> Result<()> {
        validate_tunnel_id(tunnel_id)?;
        validate_port(port)?;
        self.empty(
            self.request(
                Method::PUT,
                &format!("/tunnels/{tunnel_id}/ports/{port}"),
            )
            .json(&UpdatePortRequest { allow_anonymous }),
        )
        .await
    }

    pub async fn delete_port(&self, tunnel_id: &str, port: i32) -> Result<()> {
        validate_tunnel_id(tunnel_id)?;
        validate_port(port)?;
        self.empty(self.request(
            Method::DELETE,
            &format!("/tunnels/{tunnel_id}/ports/{port}"),
        ))
        .await
    }

    pub async fn limits(&self) -> Result<LimitsResult> {
        self.json(self.request(Method::GET, "/limits")).await
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateTunnelRequest<'a> {
    name: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    description: &'a str,
    #[serde(rename = "ClusterId")]
    cluster_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    expiration: Option<i32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateTunnelRequest<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expiration: Option<i32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PortRequest<'a> {
    port: i32,
    protocol: &'a str,
    allow_anonymous: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdatePortRequest {
    allow_anonymous: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTunnelResult {
    pub tunnel_id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub expiration_hours: i32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListTunnelResult {
    pub name: String,
    pub tunnel_id: String,
    pub tunnel_expiration: u32,
    #[serde(default)]
    pub description: String,
    pub port_count: i32,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelStatus {
    pub client_connection_count: i32,
    pub host_connection_count: i32,
    pub total_upload_bytes: i64,
    pub total_download_bytes: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowTunnelResult {
    pub name: String,
    pub tunnel_id: String,
    pub tunnel_expiration: u32,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub status: Option<TunnelStatus>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelTokenResult {
    pub tunnel_id: String,
    pub scope: String,
    pub token: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortResult {
    pub port: i32,
    #[serde(default)]
    pub protocol: String,
    #[serde(default)]
    pub allow_anonymous: bool,
    #[serde(default)]
    pub tunnel_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitsResult {
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

pub fn validate_tunnel_id(tunnel_id: &str) -> Result<()> {
    if tunnel_id.len() == 8
        && tunnel_id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || (b'2'..=b'7').contains(&byte))
    {
        Ok(())
    } else {
        bail!(
            "invalid tunnel id {tunnel_id:?}: expected 8 lowercase letters/base32 digits 2-7"
        )
    }
}

pub fn validate_port(port: i32) -> Result<()> {
    if port == ALL_PORTS || (1..=65535).contains(&port) {
        Ok(())
    } else {
        bail!("invalid port {port}: expected 1-65535 or -1 for all URL ports")
    }
}

fn validate_protocol(protocol: &str) -> Result<()> {
    if matches!(protocol, "http" | "https" | "auto" | "") {
        Ok(())
    } else {
        bail!("invalid protocol {protocol:?}: expected http, https or auto")
    }
}

fn validate_expiration(expiration: i32) -> Result<()> {
    if (1..=720).contains(&expiration) {
        Ok(())
    } else {
        bail!("expiration must be between 1 and 720 hours")
    }
}

fn validate_tunnel_name(name: &str) -> Result<()> {
    let chars: Vec<char> = name.chars().collect();
    if chars.is_empty() || chars.len() > 64 {
        bail!("tunnel name must contain 1-64 characters");
    }
    if !allowed_name_edge(chars[0]) || !allowed_name_edge(chars[chars.len() - 1]) {
        bail!("tunnel name must start and end with a Chinese character, letter or digit");
    }
    if !chars
        .iter()
        .all(|value| allowed_name_edge(*value) || *value == '-')
    {
        bail!("tunnel name contains unsupported characters");
    }
    Ok(())
}

fn validate_tunnel_description(description: &str) -> Result<()> {
    if description.chars().count() > 64
        || !description.chars().all(allowed_name_edge)
    {
        bail!("description supports up to 64 Chinese characters, letters or digits");
    }
    Ok(())
}

fn allowed_name_edge(value: char) -> bool {
    value.is_ascii_alphanumeric() || ('\u{4e00}'..='\u{9fa5}').contains(&value)
}

fn parse_api_error(status: u16, body: &str) -> anyhow::Error {
    if let Ok(parsed) = serde_json::from_str::<ErrorBody>(body) {
        return ApiError {
            code: parsed.error.code,
            message: parsed.error.message,
        }
        .into();
    }
    anyhow!("DevBridge API request failed: HTTP {status}: {body}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_tunnel_ids() {
        assert!(validate_tunnel_id("abcde234").is_ok());
        assert!(validate_tunnel_id("ABCde234").is_err());
        assert!(validate_tunnel_id("short").is_err());
    }

    #[test]
    fn all_ports_is_not_a_tcp_port() {
        assert!(validate_port(ALL_PORTS).is_ok());
        assert!(validate_port(0).is_err());
        assert!(validate_port(65536).is_err());
    }
}

use crate::{config, i18n};
use anyhow::{Context, Result, anyhow, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use keyring::Entry;
use serde::Deserialize;
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

const CREDENTIAL_SERVICE: &str = "HWCLOUD";
const CREDENTIAL_USER: &str = "Credentials";
const LOGIN_URL: &str = "https://devstation.ulanqab.huawei.com";
const LOGIN_SUCCESS_CODE: &str = "0000";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CallbackResponse {
    api_key: String,
    #[serde(default)]
    user_name: String,
    #[serde(default)]
    user_id: String,
}

#[derive(Debug, Deserialize)]
struct LoginEnvelope {
    error_code: String,
    #[serde(default)]
    error_msg: String,
    result: CallbackResponse,
}

pub fn read_api_key(override_key: Option<&str>) -> Result<String> {
    if let Some(key) = override_key.filter(|v| !v.is_empty()) {
        return Ok(key.to_owned());
    }
    for name in ["HW_API_KEY", "DEVBRIDGE_API_KEY"] {
        if let Ok(key) = std::env::var(name)
            && !key.is_empty()
        {
            return Ok(key);
        }
    }
    if let Ok(entry) = Entry::new(CREDENTIAL_SERVICE, CREDENTIAL_USER)
        && let Ok(blob) = entry.get_password()
        && let Ok(bytes) = STANDARD.decode(blob)
        && let Ok(key) = String::from_utf8(bytes)
        && !key.is_empty()
    {
        return Ok(key);
    }
    let (cred, _) = config::config_credential()?;
    cred.filter(|v| !v.api_key.is_empty())
        .map(|v| v.api_key)
        .ok_or_else(|| anyhow!(i18n::api::expired()))
}

pub fn current_user_info() -> Option<config::UserInfo> {
    config::user_info().ok().flatten()
}

pub fn store_credential(api_key: &str, user_info: Option<config::UserInfo>) -> Result<()> {
    let blob = STANDARD.encode(api_key.as_bytes());
    let stored_in_keyring = Entry::new(CREDENTIAL_SERVICE, CREDENTIAL_USER)
        .and_then(|entry| entry.set_password(&blob))
        .is_ok();

    let fallback = if stored_in_keyring {
        None
    } else {
        Some(config::Credential {
            api_key: api_key.to_owned(),
        })
    };
    config::store_config_credential(fallback, user_info)
}

pub fn delete_credential() -> Result<()> {
    if let Ok(entry) = Entry::new(CREDENTIAL_SERVICE, CREDENTIAL_USER) {
        let _ = entry.delete_credential();
    }
    config::clear_credentials()
}

pub async fn verify_api_key(api_key: &str, base_override: Option<&str>) -> Result<()> {
    if api_key.is_empty() {
        bail!("api key is invalid or disabled");
    }
    let base = base_override
        .unwrap_or(config::DEFAULT_SERVER_DOMAIN)
        .trim_end_matches('/');
    let url = format!("{base}/open-api-inner/v1/relay-controller/auth/check");
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(10))
        .build()?;
    let response = client
        .get(url)
        .header("X-API-Key", api_key)
        .send()
        .await
        .context("verify api key")?;
    match response.status().as_u16() {
        204 => Ok(()),
        401 => bail!("api key is invalid or disabled"),
        status => {
            let body = response.text().await.unwrap_or_default();
            bail!("verify api key: unexpected status {status}, body={body}")
        }
    }
}

pub async fn login(api_key: Option<String>, base_override: Option<&str>) -> Result<()> {
    if api_key.is_none()
        && let Ok(existing) = read_api_key(None)
        && verify_api_key(&existing, base_override).await.is_ok()
    {
        println!("{}", i18n::auth::login_success());
        return Ok(());
    }

    let (api_key, user_info) = match api_key.filter(|v| !v.is_empty()) {
        Some(key) => (key, None),
        None => browser_login().await?,
    };
    verify_api_key(&api_key, base_override)
        .await
        .map_err(|e| anyhow!("login failed: {e}"))?;
    store_credential(&api_key, user_info)?;
    println!("{}", i18n::auth::login_success());
    Ok(())
}

async fn browser_login() -> Result<(String, Option<config::UserInfo>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let language = if i18n::system_lang() == i18n::Lang::Zh {
        "zh-cn"
    } else {
        "en-us"
    };
    let mut url = url::Url::parse(&format!("{LOGIN_URL}/space/devbridge/redirect"))?;
    url.query_pairs_mut()
        .append_pair("origin", "devbridge")
        .append_pair("language", language)
        .append_pair("callback", &port.to_string());

    tracing::debug!("{}", i18n::auth::open_browser());
    if let Err(error) = open::that(url.as_str()) {
        let page = format!("{LOGIN_URL}/space/devbridge/apikey");
        bail!(
            "No browser available, please login manually. API Key management page: {page}\nFor example: ./devbridge auth login --api-key=YOUR_API_KEY ({error})"
        );
    }
    tracing::debug!("{}", i18n::auth::browser_opened());

    tokio::time::timeout(Duration::from_secs(300), async move {
        loop {
            let (mut socket, _) = listener.accept().await?;
            let mut raw = vec![0_u8; 8192];
            let mut used = 0usize;
            loop {
                if used == raw.len() {
                    bail!("bad request");
                }
                let n = socket.read(&mut raw[used..]).await?;
                if n == 0 {
                    break;
                }
                used += n;
                if raw[..used].windows(4).any(|v| v == b"\r\n\r\n") {
                    break;
                }
            }
            let header_end = raw[..used]
                .windows(4)
                .position(|v| v == b"\r\n\r\n")
                .map(|v| v + 4)
                .ok_or_else(|| anyhow!("bad request"))?;
            let headers = String::from_utf8_lossy(&raw[..header_end]);
            let request_line = headers.lines().next().unwrap_or_default();

            if request_line.starts_with("OPTIONS ") {
                write_http_response(&mut socket, 200, "OK").await?;
                continue;
            }
            if !request_line.starts_with("POST ") {
                write_http_response(&mut socket, 405, "method not allowed").await?;
                continue;
            }

            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);
            if content_length > 4096 {
                write_http_response(&mut socket, 400, "bad request").await?;
                continue;
            }

            let already = used.saturating_sub(header_end);
            while used < header_end + content_length {
                let n = socket.read(&mut raw[used..]).await?;
                if n == 0 {
                    break;
                }
                used += n;
            }
            let body_end = header_end + content_length.min(used.saturating_sub(header_end));
            let body = if already >= content_length {
                &raw[header_end..header_end + content_length]
            } else {
                &raw[header_end..body_end]
            };
            let envelope: LoginEnvelope = match serde_json::from_slice(body) {
                Ok(v) => v,
                Err(_) => {
                    write_http_response(&mut socket, 400, "bad request").await?;
                    continue;
                }
            };
            if envelope.error_code != LOGIN_SUCCESS_CODE {
                let page = format!("{LOGIN_URL}/space/devbridge/apikey");
                write_http_response(&mut socket, 400, "bad request").await?;
                bail!(
                    "Failed to login: error code: {}, error message: {}\nPlease go to the management page to delete API Keys and try again. API Key management page: {page}",
                    envelope.error_code,
                    envelope.error_msg
                );
            }
            if envelope.result.api_key.is_empty() {
                write_http_response(&mut socket, 400, "missing api key").await?;
                bail!("missing api key");
            }
            write_http_response(&mut socket, 200, "OK").await?;
            let user_info = if envelope.result.user_name.is_empty()
                && envelope.result.user_id.is_empty()
            {
                None
            } else {
                Some(config::UserInfo {
                    user_name: envelope.result.user_name,
                    user_id: envelope.result.user_id,
                })
            };
            return Ok::<_, anyhow::Error>((envelope.result.api_key, user_info));
        }
    })
    .await
    .map_err(|_| anyhow!("login timeout"))?
}

async fn write_http_response(socket: &mut tokio::net::TcpStream, status: u16, body: &str) -> Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        405 => "Method Not Allowed",
        _ => "Error",
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nAccess-Control-Allow-Origin: {LOGIN_URL}\r\nAccess-Control-Allow-Methods: POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    socket.write_all(response.as_bytes()).await?;
    Ok(())
}

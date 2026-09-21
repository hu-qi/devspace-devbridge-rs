use std::{env, sync::Arc, time::Duration};

use anyhow::{Context, Result, bail};
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};
use serde::{Deserialize, Serialize};
use tokio::{
    net::TcpListener,
    sync::{Mutex, oneshot},
};
use url::Url;

use crate::{config, settings::Settings};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Credential {
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInfo {
    #[serde(default)]
    pub user_name: String,
    #[serde(default)]
    pub user_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginResult {
    api_key: String,
    #[serde(default)]
    user_name: String,
    #[serde(default)]
    user_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct LoginEnvelope {
    error_code: String,
    #[serde(default)]
    error_msg: String,
    result: Option<LoginResult>,
}

#[derive(Clone)]
struct CallbackState {
    sender: Arc<Mutex<Option<oneshot::Sender<LoginEnvelope>>>>,
    allow_origin: HeaderValue,
}

pub fn resolve_api_key(explicit: Option<&str>) -> Result<String> {
    if let Some(value) = explicit.filter(|value| !value.trim().is_empty()) {
        return Ok(value.to_owned());
    }
    if let Ok(value) = env::var("HW_API_KEY") {
        if !value.trim().is_empty() {
            return Ok(value);
        }
    }
    config::load()?
        .credentials
        .map(|credential| credential.api_key)
        .filter(|value| !value.trim().is_empty())
        .context("missing API key; run 'devbridge auth login' or set HW_API_KEY")
}

pub async fn verify_api_key(settings: &Settings, api_key: &str) -> Result<bool> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .danger_accept_invalid_certs(settings.insecure_tls)
        .build()?;
    let response = client
        .get(format!("{}/auth/check", settings.api_base_url()))
        .header("X-API-Key", api_key)
        .send()
        .await?;
    Ok(response.status() == StatusCode::NO_CONTENT)
}

pub async fn login_with_api_key(settings: &Settings, api_key: String) -> Result<UserInfo> {
    if !verify_api_key(settings, &api_key).await? {
        bail!("API key verification failed");
    }
    let mut current = config::load()?;
    current.credentials = Some(Credential { api_key });
    let user = current.user_info.clone().unwrap_or(UserInfo {
        user_name: String::new(),
        user_id: String::new(),
    });
    config::save(&current)?;
    Ok(user)
}

pub async fn browser_login(settings: &Settings) -> Result<UserInfo> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let port = listener.local_addr()?.port();
    let (tx, rx) = oneshot::channel();
    let login_origin = Url::parse(&settings.login_url)?
        .origin()
        .ascii_serialization()
        .parse::<HeaderValue>()
        .context("invalid login origin")?;

    let state = CallbackState {
        sender: Arc::new(Mutex::new(Some(tx))),
        allow_origin: login_origin,
    };
    let app = Router::new()
        .route("/", post(callback).options(preflight))
        .with_state(state);

    let server = tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, app).await {
            tracing::debug!(%error, "login callback server stopped");
        }
    });

    let mut login_url = Url::parse(&settings.login_url)?
        .join("/space/devbridge/redirect")?;
    login_url
        .query_pairs_mut()
        .append_pair("origin", "devbridge")
        .append_pair("language", preferred_language())
        .append_pair("callback", &port.to_string());

    webbrowser::open(login_url.as_str())
        .with_context(|| format!("open this URL manually: {login_url}"))?;

    let envelope = tokio::time::timeout(Duration::from_secs(300), rx)
        .await
        .context("login timed out after 5 minutes")?
        .context("login callback closed unexpectedly")?;
    server.abort();

    if envelope.error_code != "0000" {
        bail!(
            "login failed: {} {}",
            envelope.error_code,
            envelope.error_msg
        );
    }
    let result = envelope.result.context("login response did not contain credentials")?;
    if !verify_api_key(settings, &result.api_key).await? {
        bail!("login succeeded but API key verification failed");
    }

    let user = UserInfo {
        user_name: result.user_name,
        user_id: result.user_id,
    };
    let mut current = config::load()?;
    current.credentials = Some(Credential {
        api_key: result.api_key,
    });
    current.user_info = Some(user.clone());
    config::save(&current)?;
    Ok(user)
}

pub fn logout() -> Result<()> {
    let mut current = config::load()?;
    current.credentials = None;
    current.user_info = None;
    config::save(&current)
}

async fn callback(
    State(state): State<CallbackState>,
    Json(payload): Json<LoginEnvelope>,
) -> Response {
    let mut sender = state.sender.lock().await;
    if let Some(tx) = sender.take() {
        let _ = tx.send(payload);
    }

    let mut response = (
        StatusCode::OK,
        "DevBridge login completed. You can close this page.",
    )
        .into_response();
    add_cors_headers(response.headers_mut(), &state.allow_origin);
    response
}

async fn preflight(State(state): State<CallbackState>) -> Response {
    let mut response = StatusCode::NO_CONTENT.into_response();
    add_cors_headers(response.headers_mut(), &state.allow_origin);
    response
}

fn add_cors_headers(headers: &mut axum::http::HeaderMap, origin: &HeaderValue) {
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin.clone());
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("POST, OPTIONS"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("content-type"),
    );
}

fn preferred_language() -> &'static str {
    if env::var("DEVBRIDGE_LANG")
        .map(|value| value.to_ascii_lowercase().starts_with("zh"))
        .unwrap_or(false)
    {
        "zh-CN"
    } else {
        "en-US"
    }
}

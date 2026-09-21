use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

pub const DEFAULT_SERVER_DOMAIN: &str = "https://relay-dev-local.tailb4159e.ts.net:8443";
pub const DEFAULT_CLUSTER_ID: &str = "devbridge-s2";

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct Credential {
    #[serde(default)]
    pub api_key: String,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct UserInfo {
    #[serde(default)]
    pub user_name: String,
    #[serde(default)]
    pub user_id: String,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct AppConfig {
    #[serde(default, rename = "default-tunnel-id")]
    pub default_tunnel_id: Option<String>,
    #[serde(default)]
    pub credentials: Option<Credential>,
    #[serde(default, rename = "user-info")]
    pub user_info: Option<UserInfo>,
}

pub fn config_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("cannot determine home directory"))?;
    Ok(home.join(".huawei").join("devbridge").join("config.yaml"))
}

pub fn load() -> Result<AppConfig> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let text = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    if text.trim().is_empty() {
        return Ok(AppConfig::default());
    }
    serde_yaml::from_str(&text).with_context(|| format!("parse {}", path.display()))
}

pub fn save(cfg: &AppConfig) -> Result<()> {
    let path = config_path()?;
    let dir = path.parent().ok_or_else(|| anyhow!("invalid config path"))?;
    fs::create_dir_all(dir)?;
    let data = serde_yaml::to_string(cfg)?;
    fs::write(&path, data)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

pub fn api_key(override_key: Option<&str>) -> Result<String> {
    if let Some(key) = override_key.filter(|v| !v.is_empty()) {
        return Ok(key.to_owned());
    }
    if let Ok(key) = std::env::var("DEVBRIDGE_API_KEY") {
        if !key.is_empty() {
            return Ok(key);
        }
    }
    let cfg = load()?;
    cfg.credentials
        .filter(|c| !c.api_key.is_empty())
        .map(|c| c.api_key)
        .ok_or_else(|| anyhow!("not logged in: run 'devbridge auth login --api-key <KEY>'"))
}

pub fn default_tunnel() -> Result<String> {
    load()?
        .default_tunnel_id
        .ok_or_else(|| anyhow!("tunnel ID not specified and no default tunnel set; use 'devbridge tunnel set <id>'"))
}

pub fn set_default_tunnel(id: Option<String>) -> Result<()> {
    let mut cfg = load()?;
    cfg.default_tunnel_id = id;
    save(&cfg)
}

pub fn store_api_key(key: String) -> Result<()> {
    let mut cfg = load()?;
    cfg.credentials = Some(Credential { api_key: key });
    save(&cfg)
}

pub fn clear_auth() -> Result<()> {
    let mut cfg = load()?;
    cfg.credentials = None;
    cfg.user_info = None;
    cfg.default_tunnel_id = None;
    save(&cfg)
}

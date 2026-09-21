use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::{collections::BTreeMap, fs, path::PathBuf};

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
    #[serde(default, rename = "default-tunnel-id", skip_serializing_if = "Option::is_none")]
    pub default_tunnel_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credentials: Option<Credential>,
    #[serde(default, rename = "user-info", skip_serializing_if = "Option::is_none")]
    pub user_info: Option<UserInfo>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
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

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }

    let data = if cfg.default_tunnel_id.is_none()
        && cfg.credentials.is_none()
        && cfg.user_info.is_none()
        && cfg.extra.is_empty()
    {
        String::new()
    } else {
        serde_yaml::to_string(cfg)?
    };
    fs::write(&path, data)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

pub fn default_tunnel() -> Result<String> {
    load()?.default_tunnel_id.ok_or_else(|| {
        anyhow!(
            "tunnel ID not specified and no default tunnel set, please specify via argument or use 'devbridge set' to set default"
        )
    })
}

pub fn set_default_tunnel(id: Option<String>) -> Result<()> {
    let mut cfg = load()?;
    cfg.default_tunnel_id = id;
    save(&cfg)
}

pub fn config_credential() -> Result<(Option<Credential>, Option<UserInfo>)> {
    let cfg = load()?;
    Ok((cfg.credentials, cfg.user_info))
}

pub fn store_config_credential(cred: Option<Credential>, user_info: Option<UserInfo>) -> Result<()> {
    let mut cfg = load()?;
    cfg.credentials = cred;
    if user_info.is_some() {
        cfg.user_info = user_info;
    }
    save(&cfg)
}

pub fn clear_credentials() -> Result<()> {
    let mut cfg = load()?;
    cfg.credentials = None;
    cfg.user_info = None;
    save(&cfg)
}

pub fn user_info() -> Result<Option<UserInfo>> {
    Ok(load()?.user_info)
}

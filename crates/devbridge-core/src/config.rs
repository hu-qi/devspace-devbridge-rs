use std::{fs, io::Write, path::PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::auth::{Credential, UserInfo};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct AppConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credentials: Option<Credential>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_info: Option<UserInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_tunnel_id: Option<String>,
}

pub fn config_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("cannot resolve home directory")?;
    Ok(home.join(".huawei").join("devbridge"))
}

pub fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.yaml"))
}

pub fn load() -> Result<AppConfig> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_yaml::from_str(&raw)
        .with_context(|| format!("failed to parse {}", path.display()))
}

pub fn save(config: &AppConfig) -> Result<()> {
    let dir = config_dir()?;
    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create {}", dir.display()))?;
    set_dir_permissions(&dir)?;

    let path = config_path()?;
    let raw = serde_yaml::to_string(config)?;
    let mut file = fs::File::create(&path)
        .with_context(|| format!("failed to create {}", path.display()))?;
    file.write_all(raw.as_bytes())?;
    file.sync_all()?;
    set_file_permissions(&path)?;
    Ok(())
}

pub fn set_default_tunnel(tunnel_id: Option<String>) -> Result<()> {
    let mut config = load()?;
    config.default_tunnel_id = tunnel_id;
    save(&config)
}

pub fn default_tunnel() -> Result<String> {
    load()?
        .default_tunnel_id
        .filter(|value| !value.trim().is_empty())
        .context("no default tunnel configured; run 'devbridge set <tunnel-id>' first")
}

#[cfg(unix)]
fn set_dir_permissions(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_dir_permissions(_path: &std::path::Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_file_permissions(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_file_permissions(_path: &std::path::Path) -> Result<()> {
    Ok(())
}

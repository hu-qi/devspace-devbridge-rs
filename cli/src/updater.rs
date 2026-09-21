use crate::config;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, time::{Duration, SystemTime, UNIX_EPOCH}};

const GITCODE_API: &str = "https://gitcode.com/api/v5/repos/CloudDeveloperDepartment/devbrige/releases";
const GITCODE_INSTALL_SH: &str = "https://gitcode.com/CloudDeveloperDepartment/devbrige/releases/download/latest/install.sh";
const GITCODE_INSTALL_PS: &str = "https://gitcode.com/CloudDeveloperDepartment/devbrige/releases/download/latest/install.ps1";
const CACHE_TTL: i64 = 24 * 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    pub latest_version: String,
    pub latest_tag: String,
    pub checked_at: i64,
    #[serde(default)]
    pub last_notify_at: i64,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64
}

fn cache_path() -> Option<PathBuf> {
    config::config_path().ok()?.parent().map(|p| p.join("version_cache.json"))
}

fn parse_version(v: &str) -> Option<[u64; 4]> {
    let v = v.strip_prefix('v').unwrap_or(v);
    let numeric = v.split(|c: char| !c.is_ascii_digit() && c != '.').next()?;
    let parts = numeric.split('.').collect::<Vec<_>>();
    if parts.len() < 3 || parts.len() > 4 {
        return None;
    }
    let mut out = [0_u64; 4];
    for (idx, part) in parts.iter().enumerate() {
        out[idx] = part.parse().ok()?;
    }
    Some(out)
}

pub fn is_newer(current: &str, latest: &str) -> bool {
    let Some(latest) = parse_version(latest) else { return false; };
    match parse_version(current) {
        Some(current) => latest > current,
        None => true,
    }
}

fn version_from_tag(tag: &str) -> Option<String> {
    let v = parse_version(tag)?;
    let original = tag.strip_prefix('v').unwrap_or(tag);
    let count = original.split('.').take(4).count();
    if count >= 4 && v[3] != 0 {
        Some(format!("{}.{}.{}.{}", v[0], v[1], v[2], v[3]))
    } else {
        Some(format!("{}.{}.{}", v[0], v[1], v[2]))
    }
}

fn load_cache() -> Option<CheckResult> {
    let data = fs::read(cache_path()?).ok()?;
    let result: CheckResult = serde_json::from_slice(&data).ok()?;
    (now() - result.checked_at <= CACHE_TTL).then_some(result)
}

fn save_cache(result: &CheckResult) {
    let Some(path) = cache_path() else { return; };
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(data) = serde_json::to_vec(result) {
        let _ = fs::write(path, data);
    }
}

async fn fetch_latest() -> Option<(String, String)> {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(10)).build().ok()?;
    let releases: Vec<Release> = client.get(GITCODE_API).send().await.ok()?.json().await.ok()?;
    let mut best: Option<(String, String)> = None;
    for release in releases {
        let Some(version) = version_from_tag(&release.tag_name) else { continue; };
        if best.as_ref().is_none_or(|(_, current)| is_newer(current, &version)) {
            best = Some((release.tag_name, version));
        }
    }
    best
}

pub async fn check(force: bool) -> Option<CheckResult> {
    if !force
        && let Some(cached) = load_cache()
    {
        return Some(cached);
    }
    let (tag, version) = fetch_latest().await?;
    let result = CheckResult {
        latest_version: version,
        latest_tag: tag,
        checked_at: now(),
        last_notify_at: 0,
    };
    save_cache(&result);
    Some(result)
}

pub fn check_async(current: &'static str) {
    tokio::spawn(async move {
        let Some(mut result) = check(false).await else { return; };
        if !is_newer(current, &result.latest_version) {
            return;
        }
        if result.last_notify_at > 0 && now() - result.last_notify_at < CACHE_TTL {
            return;
        }
        result.last_notify_at = now();
        save_cache(&result);
        eprintln!(
            "\nA new version is available: {} (current: {})\nUpdate:\n{}\n",
            result.latest_version,
            current,
            install_command()
        );
    });
}

pub fn install_command() -> String {
    if cfg!(windows) {
        format!(
            "  # GitCode\n  irm {GITCODE_INSTALL_PS} | iex\n\n  # GitHub\n  irm https://github.com/hu-qi/devspace-devbridge-rs/releases/latest/download/install.ps1 | iex"
        )
    } else {
        format!(
            "  # GitCode\n  curl -fsSL {GITCODE_INSTALL_SH} | bash\n\n  # GitHub\n  curl -fsSL https://github.com/hu-qi/devspace-devbridge-rs/releases/latest/download/install.sh | bash"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compares_legacy_versions() {
        assert!(is_newer("1.2.3", "1.2.4"));
        assert!(is_newer("1.2.3", "1.2.3.1"));
        assert!(!is_newer("2.0.0", "1.9.9"));
        assert!(is_newer("dev", "1.0.0"));
        assert!(!is_newer("1.0.0", "latest"));
    }
}

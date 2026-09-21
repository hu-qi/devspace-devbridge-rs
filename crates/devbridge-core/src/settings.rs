use std::env;

#[derive(Clone, Debug)]
pub struct Settings {
    pub server_domain: String,
    pub login_url: String,
    pub gateway_addr: String,
    pub gateway_host: String,
    pub cluster_id: String,
    pub release_repo: String,
    pub insecure_tls: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            server_domain: runtime_or_build(
                "DEVBRIDGE_SERVER_DOMAIN",
                option_env!("DEVBRIDGE_SERVER_DOMAIN"),
                "https://bridge.developer.myhuaweicloud.com",
            ),
            login_url: runtime_or_build(
                "DEVBRIDGE_LOGIN_URL",
                option_env!("DEVBRIDGE_LOGIN_URL"),
                "https://devstation.connect.huaweicloud.com",
            ),
            gateway_addr: runtime_or_build(
                "DEVBRIDGE_GATEWAY_ADDR",
                option_env!("DEVBRIDGE_GATEWAY_ADDR"),
                "gateway.devbridge-s2.hwtunnel.com:443",
            ),
            gateway_host: runtime_or_build(
                "DEVBRIDGE_GATEWAY_HOST",
                option_env!("DEVBRIDGE_GATEWAY_HOST"),
                "devbridge-s2.hwtunnel.com",
            ),
            cluster_id: runtime_or_build(
                "DEVBRIDGE_CLUSTER_ID",
                option_env!("DEVBRIDGE_CLUSTER_ID"),
                "devbridge-s2",
            ),
            release_repo: runtime_or_build(
                "DEVBRIDGE_RELEASE_REPO",
                option_env!("DEVBRIDGE_RELEASE_REPO"),
                "hu-qi/devspace-devbridge-rs",
            ),
            insecure_tls: env_flag("DEVBRIDGE_INSECURE_TLS"),
        }
    }
}

impl Settings {
    pub fn api_base_url(&self) -> String {
        format!(
            "{}/open-api-inner/v1/relay-controller",
            self.server_domain.trim_end_matches('/')
        )
    }
}

fn runtime_or_build(name: &str, build_value: Option<&'static str>, fallback: &str) -> String {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| build_value.map(ToOwned::to_owned))
        .unwrap_or_else(|| fallback.to_owned())
}

fn env_flag(name: &str) -> bool {
    env::var(name)
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_api_base_url_without_double_slash() {
        let mut settings = Settings::default();
        settings.server_domain = "https://example.invalid/".into();
        assert_eq!(
            settings.api_base_url(),
            "https://example.invalid/open-api-inner/v1/relay-controller"
        );
    }
}

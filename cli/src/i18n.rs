use std::env;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Lang {
    Zh,
    En,
}

pub fn current_lang() -> Lang {
    match env::var("DEVBRIDGE_LANG") {
        Ok(v) if v.to_ascii_lowercase().starts_with("zh") => Lang::Zh,
        _ => Lang::En,
    }
}

pub fn system_lang() -> Lang {
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(v) = env::var(key) {
            let lower = v.to_ascii_lowercase();
            if lower.starts_with("zh") {
                return Lang::Zh;
            }
            if lower.starts_with("en") {
                return Lang::En;
            }
        }
    }
    #[cfg(windows)]
    {
        // Keep behavior deterministic without adding a Win32-only dependency:
        // Windows users can override with LANG/LC_* or DEVBRIDGE_LANG.
    }
    Lang::En
}

pub fn t(en: &'static str, zh: &'static str) -> &'static str {
    if current_lang() == Lang::Zh { zh } else { en }
}

pub fn version_info() -> &'static str { t("DevBridge remote tunnel port forwarding tool", "DevBridge 远程隧道端口转发工具") }
pub fn days() -> &'static str { t("days", "天") }
pub fn hours() -> &'static str { t("hours", "小时") }
pub fn expired() -> &'static str { t("expired", "已过期") }

pub mod auth {
    use super::t;
    pub fn login_success() -> &'static str { t("Login successful", "登录成功") }
    pub fn logout_success() -> &'static str { t("Logout successful", "注销成功") }
    pub fn not_logged_in() -> &'static str { t("Not logged in", "未登录") }
    pub fn logged_in() -> &'static str { t("Logged in (Huawei Cloud IAM)", "已登录 (华为云 IAM)") }
    pub fn user_name() -> &'static str { t("User Name", "用户名") }
    pub fn open_browser() -> &'static str { t("Opening browser...", "正在打开浏览器...") }
    pub fn browser_opened() -> &'static str { t("Browser opened, please complete login in browser", "浏览器已打开，请在浏览器中完成登录") }
}

pub mod tunnel {
    use super::t;
    pub fn id() -> &'static str { t("Tunnel ID", "隧道ID") }
    pub fn name() -> &'static str { t("Name", "名称") }
    pub fn description() -> &'static str { t("Description", "描述") }
    pub fn expiration() -> &'static str { t("Tunnel Expiration", "隧道过期时间") }
    pub fn port_count() -> &'static str { t("Port Count", "端口数") }
    pub fn client_connections() -> &'static str { t("Client Connection Count", "Client 连接数") }
    pub fn host_connections() -> &'static str { t("Host Connection Count", "Host 连接数") }
    pub fn upload() -> &'static str { t("Total Upload Bytes", "总上传字节") }
    pub fn download() -> &'static str { t("Total Download Bytes", "总下载字节") }
    pub fn empty() -> &'static str { t("No tunnels found.", "没有隧道") }
    pub fn updated() -> &'static str { t("Tunnel updated successfully", "隧道更新成功") }
    pub fn deleted() -> &'static str { t("Tunnel deleted successfully", "隧道删除成功") }
    pub fn deleted_all() -> &'static str { t("All tunnels deleted", "所有隧道已删除") }
    pub fn default_set() -> &'static str { t("Default tunnel set to", "默认隧道已设置为") }
    pub fn default_unset() -> &'static str { t("Default tunnel unset", "默认隧道已清除") }
    pub fn default_cleared() -> &'static str { t("Default tunnel cleared as the tunnel was deleted", "默认隧道已随隧道删除而清除") }
    pub fn scope() -> &'static str { t("Scope", "范围") }
    pub fn token() -> &'static str { t("Token", "令牌") }
    pub fn invalid_name() -> &'static str { t(
        "Invalid tunnel name: only Chinese characters, digits, letters, hyphens allowed (hyphens cannot be at the beginning or end), length 1-64",
        "隧道名称格式无效: 仅中文、字母、数字、连字符(连字符不能在首尾)，长度1-64"
    ) }
    pub fn invalid_desc() -> &'static str { t(
        "Invalid tunnel description: only Chinese characters, digits, letters, length 0-64",
        "隧道描述无效: 仅中文、字母、数字，长度0-64"
    ) }
    pub fn invalid_exp() -> &'static str { t("Invalid expiration (1-720 hours)", "过期时间无效 (1-720小时)") }
}

pub mod port {
    use super::t;
    pub fn port() -> &'static str { t("Port", "端口") }
    pub fn protocol() -> &'static str { t("Protocol", "协议") }
    pub fn allow_anonymous() -> &'static str { t("Allow Anonymous", "允许匿名") }
    pub fn created() -> &'static str { t("Port created successfully", "端口创建成功") }
    pub fn updated() -> &'static str { t("Port updated successfully", "端口更新成功") }
    pub fn empty() -> &'static str { t("No ports bound to this tunnel.", "该隧道没有绑定端口") }
    pub fn invalid() -> &'static str { t("port must be between 1 and 65535", "端口必须为 1 到 65535 之间") }
    pub fn protocol_invalid() -> &'static str { t("Invalid protocol (http, https, auto)", "协议无效 (http, https, auto)") }
}

pub mod limits {
    use super::t;
    pub fn reset_at() -> &'static str { t("Quota Reset At", "配额重置时间") }
    pub fn quota() -> &'static str { t("Quota Bytes", "流量配额") }
    pub fn current() -> &'static str { t("Current", "当前用量") }
    pub fn active() -> &'static str { t("Active Tunnels", "活跃隧道数") }
    pub fn max_tunnels() -> &'static str { t("Max Tunnels", "隧道数上限") }
    pub fn max_ports() -> &'static str { t("Max Ports Per Tunnel", "单隧道端口数上限") }
    pub fn max_hosts() -> &'static str { t("Max Hosts Per Tunnel", "单隧道 Host 数上限") }
    pub fn bandwidth() -> &'static str { t("Max Tunnel Bandwidth", "隧道带宽上限") }
    pub fn http_rate() -> &'static str { t("Max HTTP Requests Per Minute Per Port", "单端口 HTTP 请求频率上限") }
    pub fn connections() -> &'static str { t("Max Connections Per Port", "单端口连接数上限") }
}

pub mod echo {
    use super::t;
    pub fn started() -> &'static str { t("Echo service started", "Echo 服务已启动") }
    pub fn method() -> &'static str { t("Method", "方法") }
    pub fn url() -> &'static str { t("URL", "URL") }
    pub fn host() -> &'static str { t("Host", "Host") }
    pub fn remote_addr() -> &'static str { t("Remote Addr", "远程地址") }
    pub fn proto() -> &'static str { t("Proto", "协议") }
    pub fn headers() -> &'static str { t("Headers", "请求头") }
}

pub mod api {
    use super::t;
    pub fn server_error() -> &'static str { t("Server error", "服务器错误") }
    pub fn unauthorized() -> &'static str { t("Unauthorized", "未授权") }
    pub fn invalid_response() -> &'static str { t("Invalid response", "无效响应") }
    pub fn expired() -> &'static str { t("API key expired, please login again", "API Key 已过期，请重新登录") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_is_default() {
        unsafe { std::env::remove_var("DEVBRIDGE_LANG") };
        assert_eq!(current_lang(), Lang::En);
    }
}

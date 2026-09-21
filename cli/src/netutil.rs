use std::time::{Duration, Instant};

pub struct PingResult {
    pub status_text: String,
    pub latency: Duration,
    pub error: Option<String>,
}

pub async fn ping_uri(uri: &str, timeout: Duration) -> PingResult {
    let start = Instant::now();
    let client = match reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(timeout)
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return PingResult {
                status_text: "Bad Gateway".into(),
                latency: start.elapsed(),
                error: Some(error.to_string()),
            };
        }
    };
    match client.get(uri).send().await {
        Ok(response) => PingResult {
            status_text: response.status().to_string(),
            latency: start.elapsed(),
            error: None,
        },
        Err(error) => PingResult {
            status_text: if error.is_timeout() { "Gateway Timeout" } else { "Bad Gateway" }.into(),
            latency: start.elapsed(),
            error: Some(error.to_string()),
        },
    }
}

pub mod anthropic;
pub mod gemini;
pub mod ollama;
pub mod openai_compat;
pub mod router;

use std::time::Duration;

pub(crate) fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(180))
        .connect_timeout(Duration::from_secs(5))
        .build()
        .expect("reqwest client")
}

pub(crate) fn probe_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .connect_timeout(Duration::from_secs(3))
        .build()
        .expect("reqwest client")
}

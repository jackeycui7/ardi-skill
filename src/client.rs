// HTTP client to the Ardi coordinator server (coord-rs).

use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::time::Duration;

pub struct ApiClient {
    base: String,
    http: Client,
}

impl ApiClient {
    pub fn new(base: impl Into<String>) -> Result<Self> {
        let http = Client::builder()
            .timeout(Duration::from_secs(15))
            .user_agent(concat!("ardi-agent/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            base: base.into().trim_end_matches('/').to_string(),
            http,
        })
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base, path);
        let resp = self
            .http
            .get(&url)
            .send()
            .with_context(|| format!("GET {url}"))?;
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        if !status.is_success() {
            return Err(anyhow!("HTTP {} from {}: {}", status.as_u16(), url, text));
        }
        serde_json::from_str(&text)
            .with_context(|| format!("parse JSON from {url}: {text}"))
    }

    pub fn try_get_json<T: DeserializeOwned>(&self, path: &str) -> Result<Option<T>> {
        let url = format!("{}{}", self.base, path);
        let resp = self
            .http
            .get(&url)
            .send()
            .with_context(|| format!("GET {url}"))?;
        if resp.status().as_u16() == 404 {
            return Ok(None);
        }
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        if !status.is_success() {
            return Err(anyhow!("HTTP {} from {}: {}", status.as_u16(), url, text));
        }
        let v = serde_json::from_str(&text)
            .with_context(|| format!("parse JSON from {url}: {text}"))?;
        Ok(Some(v))
    }

    pub fn ping(&self) -> Result<Value> {
        self.get_json("/v1/health")
    }
}

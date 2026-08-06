//! HTTP client wrapper for the CLI.
//!
//! Talks to a local ron server (`$RON_URL`, default `http://127.0.0.1:7780`).
//! The bearer token is read from `~/.config/ron/cli-token.json` so that
//! `ron token grant` saves it once and every later command reuses it.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, Result};
use reqwest::blocking::{Client, RequestBuilder, Response};
use serde::{Deserialize, Serialize};

use crate::models::{Metric, Note, Pulse};

const DEFAULT_URL: &str = "http://127.0.0.1:7780";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredToken {
    pub id: String,
    pub label: String,
    pub secret: String,
}

/// Where the CLI persists the bearer token after `ron token grant`.
pub fn token_file() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "ron")
        .ok_or_else(|| anyhow!("could not determine config dir"))?;
    let dir = dirs.config_dir().to_path_buf();
    std::fs::create_dir_all(&dir).ok();
    Ok(dir.join("cli-token.json"))
}

pub fn load_token() -> Result<Option<StoredToken>> {
    let path = token_file()?;
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&text).ok())
}

pub fn save_token(token: &StoredToken) -> Result<()> {
    let path = token_file()?;
    std::fs::write(&path, serde_json::to_string_pretty(token)?)?;
    Ok(())
}

pub fn clear_token() -> Result<()> {
    let path = token_file()?;
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

pub fn base_url() -> String {
    std::env::var("RON_URL").unwrap_or_else(|_| DEFAULT_URL.to_string())
}

pub fn http_client() -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| anyhow!(e).context("building HTTP client"))
}

fn require_token() -> Result<String> {
    let stored = load_token()?.ok_or_else(|| {
        anyhow!("no CLI token found; run `ron token grant` first")
    })?;
    Ok(stored.secret)
}

/// Authorize a RequestBuilder with the stored bearer token.
fn auth(rb: RequestBuilder) -> Result<RequestBuilder> {
    Ok(rb.bearer_auth(require_token()?))
}

/// Convert a `Response` into JSON, mapping 4xx/5xx to errors with the body.
fn json_or_err<T: for<'de> Deserialize<'de>>(resp: Response) -> Result<T> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp.json()?);
    }
    let body = resp.text().unwrap_or_default();
    let detail = match serde_json::from_str::<serde_json::Value>(&body) {
        Ok(v) => v["error"].as_str().unwrap_or(&body).to_string(),
        Err(_) => body,
    };
    Err(anyhow!("HTTP {status}: {detail}"))
}

/// A tiny wrapper for issuing GET/POST/PUT/DELETE with auth + error handling.
pub struct Api;

impl Api {
    /// Issue a request with no bearer header. Use only for `/api/tokens`
    /// endpoints (which the server exempts from auth) and during bootstrap.
    pub fn post_json_no_auth(path: &str, body: &serde_json::Value) -> Result<Response> {
        let rb = http_client()?.post(format!("{}{path}", base_url())).json(body);
        rb.send().map_err(Into::into)
    }
    pub fn delete_no_auth(path: &str) -> Result<Response> {
        let rb = http_client()?.delete(format!("{}{path}", base_url()));
        rb.send().map_err(Into::into)
    }
    pub fn get_no_auth(path: &str) -> Result<Response> {
        http_client()?.get(format!("{}{path}", base_url())).send().map_err(Into::into)
    }

    pub fn get(path: &str) -> Result<Response> {
        let rb = http_client()?.get(format!("{}{path}", base_url()));
        auth(rb)?.send().map_err(Into::into)
    }
    pub fn post_json(path: &str, body: &serde_json::Value) -> Result<Response> {
        let rb = http_client()?.post(format!("{}{path}", base_url())).json(body);
        auth(rb)?.send().map_err(Into::into)
    }
    pub fn put_json(path: &str, body: &serde_json::Value) -> Result<Response> {
        let rb = http_client()?.put(format!("{}{path}", base_url())).json(body);
        auth(rb)?.send().map_err(Into::into)
    }
    pub fn delete(path: &str) -> Result<Response> {
        let rb = http_client()?.delete(format!("{}{path}", base_url()));
        auth(rb)?.send().map_err(Into::into)
    }
    pub fn get_json<T: for<'de> Deserialize<'de>>(path: &str) -> Result<T> {
        json_or_err(Self::get(path)?)
    }
    pub fn post_json_reply<T: for<'de> Deserialize<'de>>(
        path: &str,
        body: &serde_json::Value,
    ) -> Result<T> {
        json_or_err(Self::post_json(path, body)?)
    }
    pub fn put_json_reply<T: for<'de> Deserialize<'de>>(
        path: &str,
        body: &serde_json::Value,
    ) -> Result<T> {
        json_or_err(Self::put_json(path, body)?)
    }
}

// ----- High-level helpers ---------------------------------------------------

pub fn list_notes(limit: Option<u32>) -> Result<Vec<Note>> {
    let path = match limit {
        Some(n) => format!("/api/notes?limit={n}"),
        None => "/api/notes".to_string(),
    };
    Api::get_json(&path)
}

pub fn get_note(id: &str) -> Result<Note> {
    Api::get_json(&format!("/api/notes/{id}"))
}

pub fn create_note(title: &str, tags: Vec<String>, notebook: &str, body: &str) -> Result<Note> {
    Api::post_json_reply(
        "/api/notes",
        &serde_json::json!({
            "title": title,
            "tags": tags,
            "notebook": notebook,
            "body": body,
        }),
    )
}

pub fn update_note(
    id: &str,
    title: Option<String>,
    tags: Option<Vec<String>>,
    notebook: Option<String>,
    body: Option<String>,
    related: Option<Vec<String>>,
) -> Result<Note> {
    let mut payload = serde_json::json!({});
    if let Some(v) = title {
        payload["title"] = serde_json::Value::String(v);
    }
    if let Some(v) = tags {
        payload["tags"] = serde_json::Value::Array(v.into_iter().map(serde_json::Value::String).collect());
    }
    if let Some(v) = notebook {
        payload["notebook"] = serde_json::Value::String(v);
    }
    if let Some(v) = body {
        payload["body"] = serde_json::Value::String(v);
    }
    if let Some(v) = related {
        payload["related"] = serde_json::Value::Array(v.into_iter().map(serde_json::Value::String).collect());
    }
    Api::put_json_reply(&format!("/api/notes/{id}"), &payload)
}

pub fn delete_note(id: &str) -> Result<()> {
    let resp = Api::delete(&format!("/api/notes/{id}"))?;
    if !resp.status().is_success() {
        return Err(anyhow!("delete failed: HTTP {}", resp.status()));
    }
    Ok(())
}

pub fn search_notes(
    q: &str,
    field: &str,
    ignore_case: bool,
    whole_word: bool,
) -> Result<Vec<Note>> {
    let path = format!(
        "/api/notes/search?q={}&field={}&ignore_case={}&whole_word={}",
        urlencoding::encode_or_self(q),
        field,
        ignore_case,
        whole_word
    );
    Api::get_json(&path)
}

// tiny URL-encoder shim to avoid pulling in a crate for one call.
mod urlencoding {
    pub fn encode_or_self(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for c in s.chars() {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
                out.push(c);
            } else {
                let mut buf = [0u8; 4];
                for b in c.encode_utf8(&mut buf).as_bytes() {
                    out.push_str(&format!("%{:02X}", b));
                }
            }
        }
        out
    }
}

pub fn list_notebooks() -> Result<Vec<String>> {
    let notes: Vec<Note> = Api::get_json("/api/notes")?;
    let mut nbs: Vec<String> = notes.into_iter().map(|n| n.notebook).collect();
    nbs.sort();
    nbs.dedup();
    Ok(nbs)
}

pub fn list_pulses(active_only: bool) -> Result<Vec<Pulse>> {
    let path = if active_only {
        "/api/pulses?active_only=true"
    } else {
        "/api/pulses"
    };
    Api::get_json(path)
}

pub fn create_pulse(topic: &str, interval: &str) -> Result<Pulse> {
    Api::post_json_reply(
        "/api/pulses",
        &serde_json::json!({ "topic": topic, "interval": interval }),
    )
}

pub fn set_pulse_slot(id: &str, on: Option<&str>, checked: bool) -> Result<Pulse> {
    let path = match on {
        Some(slot) => format!("/api/pulses/{id}/check?on={}", urlencoding::encode_or_self(slot)),
        None => format!("/api/pulses/{id}/check"),
    };
    if checked {
        Api::post_json_reply(&path, &serde_json::json!({}))
    } else {
        // DELETE with optional query
        let path = match on {
            Some(slot) => format!("/api/pulses/{id}/check?on={}", urlencoding::encode_or_self(slot)),
            None => format!("/api/pulses/{id}/check"),
        };
        let resp = Api::delete(&path)?;
        json_or_err(resp)
    }
}

pub fn delete_pulse(id: &str) -> Result<()> {
    let resp = Api::delete(&format!("/api/pulses/{id}"))?;
    if !resp.status().is_success() {
        return Err(anyhow!("delete failed: HTTP {}", resp.status()));
    }
    Ok(())
}

pub fn list_metrics() -> Result<Vec<Metric>> {
    Api::get_json("/api/metrics")
}

pub fn create_metric(topic: &str) -> Result<Metric> {
    Api::post_json_reply("/api/metrics", &serde_json::json!({ "topic": topic }))
}

pub fn append_metric_point(id: &str, value: f64, ts: Option<&str>) -> Result<Metric> {
    let mut body = serde_json::json!({ "value": value });
    if let Some(ts) = ts {
        body["ts"] = serde_json::Value::String(ts.to_string());
    }
    Api::post_json_reply(&format!("/api/metrics/{id}/points"), &body)
}

#[derive(Debug, Deserialize)]
pub struct StatsResponse {
    pub topic: String,
    pub count: usize,
    pub mean: f64,
    pub median: f64,
    pub min: f64,
    pub max: f64,
    pub points: Vec<crate::models::MetricPoint>,
}

pub fn metric_stats(id: &str, from: Option<&str>, to: Option<&str>) -> Result<StatsResponse> {
    let mut path = format!("/api/metrics/{id}/stats");
    let mut sep = '?';
    if let Some(f) = from {
        path.push_str(&format!("{sep}from={}", urlencoding::encode_or_self(f)));
        sep = '&';
    }
    if let Some(t) = to {
        path.push_str(&format!("{sep}to={}", urlencoding::encode_or_self(t)));
    }
    Api::get_json(&path)
}

pub fn delete_metric(id: &str) -> Result<()> {
    let resp = Api::delete(&format!("/api/metrics/{id}"))?;
    if !resp.status().is_success() {
        return Err(anyhow!("delete failed: HTTP {}", resp.status()));
    }
    Ok(())
}

// ----- admin -----

#[derive(Debug, Deserialize)]
pub struct ExportReport {
    pub notes: usize,
    pub pulses: usize,
    pub metrics: usize,
    pub committed: bool,
}

#[derive(Debug, Deserialize)]
pub struct ImportReport {
    pub items: usize,
}

#[derive(Debug, Deserialize)]
pub struct SyncReport {
    pub changed_files: Vec<String>,
    pub items_loaded: usize,
}

pub fn export() -> Result<ExportReport> {
    Api::post_json_reply("/api/export", &serde_json::json!({}))
}

pub fn import() -> Result<ImportReport> {
    Api::post_json_reply("/api/import", &serde_json::json!({}))
}

pub fn backup() -> Result<()> {
    let resp = Api::post_json("/api/backup", &serde_json::json!({}))?;
    if !resp.status().is_success() {
        return Err(anyhow!("backup failed: HTTP {}", resp.status()));
    }
    Ok(())
}

pub fn sync() -> Result<SyncReport> {
    Api::post_json_reply("/api/sync", &serde_json::json!({}))
}

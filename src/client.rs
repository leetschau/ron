//! HTTP client wrapper for the CLI.
//!
//! Talks to a local ron server. Base URL resolution (`base_url`):
//! `$RON_URL` env var, else the `url` key in `~/.config/ron/server.json`,
//! else `http://127.0.0.1:7780`.
//! The bearer token is read from `~/.config/ron/cli-token.json` so that
//! `ron token grant` saves it once and every later command reuses it.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, Result};
use reqwest::blocking::{Client, RequestBuilder, Response};
use serde::{Deserialize, Serialize};

use crate::models::{Draft, DraftContent, Metric, Note, Pulse};

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
    if let Some(u) = std::env::var("RON_URL")
        .ok()
        .filter(|u| !u.is_empty())
    {
        return u;
    }
    crate::paths::read_configured_url().unwrap_or_else(|| DEFAULT_URL.to_string())
}

pub fn http_client() -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .no_proxy()
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

// ----- drafts ----------------------------------------------------------------

/// Reply of `GET /api/drafts/:key`.
#[derive(Debug, Deserialize)]
pub struct DraftInfo {
    pub draft: Option<Draft>,
    /// `updated` of the most recently consumed draft for this key; local
    /// copies with `saved_at <= consumed_updated` are stale and droppable.
    #[serde(default)]
    pub consumed_updated: Option<chrono::NaiveDateTime>,
}

/// Fetch the live server draft (if any) and the consume watermark.
pub fn get_draft(key: &str) -> Result<DraftInfo> {
    Api::get_json(&format!("/api/drafts/{}", urlencoding::encode_or_self(key)))
}

pub fn list_drafts() -> Result<Vec<Draft>> {
    Api::get_json("/api/drafts")
}

/// Push a draft to the server. The server stamps `updated`; the returned
/// draft carries that authoritative timestamp.
pub fn save_draft(key: &str, content: &DraftContent) -> Result<Draft> {
    Api::put_json_reply(&format!("/api/drafts/{}", urlencoding::encode_or_self(key)), &serde_json::to_value(content)?)
}

/// Hard-delete a draft on the server (watermark included).
pub fn delete_draft(key: &str) -> Result<()> {
    let resp = Api::delete(&format!("/api/drafts/{}", urlencoding::encode_or_self(key)))?;
    if !resp.status().is_success() {
        return Err(anyhow!("draft delete failed: HTTP {}", resp.status()));
    }
    Ok(())
}

// ----- local draft store -------------------------------------------------------

/// One cached draft in `~/.local/share/ron/drafts.json`: the raw editor
/// buffer exactly as it was, plus when it was saved.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocalDraft {
    pub content: String,
    pub saved_at: chrono::NaiveDateTime,
}

/// Where the CLI caches drafts locally (offline fallback). Lives next to
/// the DB, outside the git repo.
pub fn drafts_file() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "ron")
        .ok_or_else(|| anyhow!("could not determine data dir"))?;
    Ok(dirs.data_local_dir().join("drafts.json"))
}

fn read_local_store(path: &std::path::Path) -> std::collections::BTreeMap<String, LocalDraft> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn write_local_store(
    path: &std::path::Path,
    store: &std::collections::BTreeMap<String, LocalDraft>,
) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(store)?)?;
    Ok(())
}

/// Read the local draft for `key` (raw editor buffer). `None` when absent.
pub fn load_local_draft(path: &std::path::Path, key: &str) -> Option<LocalDraft> {
    read_local_store(path).get(key).cloned()
}

/// Cache `buffer` as the local draft for `key`, stamping `saved_at` now.
/// Returns the stamp.
pub fn store_local_draft(path: &std::path::Path, key: &str, buffer: &str) -> Result<chrono::NaiveDateTime> {
    let mut store = read_local_store(path);
    let saved_at = chrono::Local::now().naive_local();
    store.insert(
        key.to_string(),
        LocalDraft {
            content: buffer.to_string(),
            saved_at,
        },
    );
    write_local_store(path, &store)?;
    Ok(saved_at)
}

/// Remove the local draft for `key`; returns whether anything was removed.
pub fn drop_local_draft(path: &std::path::Path, key: &str) -> Result<bool> {
    let mut store = read_local_store(path);
    let removed = store.remove(key).is_some();
    if removed {
        write_local_store(path, &store)?;
    }
    Ok(removed)
}

/// All local drafts (key -> entry), for `ron draft list` / `clear`.
pub fn load_all_local_drafts(
    path: &std::path::Path,
) -> std::collections::BTreeMap<String, LocalDraft> {
    read_local_store(path)
}

pub fn clear_local_drafts(path: &std::path::Path) -> Result<()> {
    write_local_store(path, &std::collections::BTreeMap::new())
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

pub fn update_pulse(
    id: &str,
    topic: Option<String>,
    interval: Option<String>,
) -> Result<Pulse> {
    let mut payload = serde_json::json!({});
    if let Some(v) = topic {
        payload["topic"] = serde_json::Value::String(v);
    }
    if let Some(v) = interval {
        payload["interval"] = serde_json::Value::String(v);
    }
    Api::put_json_reply(&format!("/api/pulses/{id}"), &payload)
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

pub fn update_metric(id: &str, topic: Option<String>) -> Result<Metric> {
    let mut payload = serde_json::json!({});
    if let Some(v) = topic {
        payload["topic"] = serde_json::Value::String(v);
    }
    Api::put_json_reply(&format!("/api/metrics/{id}"), &payload)
}

// ----- admin -----

/// Client-relevant server configuration (`GET /api/config`). The server is
/// the authority for `default_notebook`; the local `server.json` value is
/// only an offline fallback.
#[derive(Debug, Deserialize)]
pub struct ServerInfo {
    pub default_notebook: String,
}

/// Fetch the server's `default_notebook`.
pub fn server_default_notebook() -> Result<String> {
    Ok(Api::get_json::<ServerInfo>("/api/config")?.default_notebook)
}

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

#[derive(Debug, Deserialize)]
pub struct CommitLine {
    pub hash: String,
    pub subject: String,
}

#[derive(Debug, Deserialize)]
pub struct BackupStatus {
    pub remote_url: Option<String>,
    pub fetched: bool,
    pub ahead: usize,
    pub behind: usize,
    pub dirty: bool,
    pub to_push: Vec<CommitLine>,
    pub to_pull: Vec<CommitLine>,
}

#[derive(Debug, Deserialize)]
pub struct BackupReport {
    pub dry_run: bool,
    #[serde(default)]
    pub pushed: bool,
    #[serde(default)]
    pub status: Option<BackupStatus>,
}

pub fn export() -> Result<ExportReport> {
    Api::post_json_reply("/api/export", &serde_json::json!({}))
}

pub fn import() -> Result<ImportReport> {
    Api::post_json_reply("/api/import", &serde_json::json!({}))
}

pub fn backup(dry_run: bool) -> Result<BackupReport> {
    Api::post_json_reply("/api/backup", &serde_json::json!({ "dry_run": dry_run }))
}

pub fn sync() -> Result<SyncReport> {
    Api::post_json_reply("/api/sync", &serde_json::json!({}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_draft_store_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("drafts.json");
        assert!(load_local_draft(&path, "new").is_none());

        let ts = store_local_draft(&path, "new", "Title: hi\n\n------\n\nbody").unwrap();
        let l = load_local_draft(&path, "new").unwrap();
        assert_eq!(l.content, "Title: hi\n\n------\n\nbody");
        assert_eq!(l.saved_at, ts);

        // A second key coexists; dropping one leaves the other.
        store_local_draft(&path, "note:note-1", "other").unwrap();
        assert!(drop_local_draft(&path, "new").unwrap());
        assert!(load_local_draft(&path, "new").is_none());
        assert!(load_local_draft(&path, "note:note-1").is_some());
        assert!(!drop_local_draft(&path, "new").unwrap());
    }

    #[test]
    fn local_draft_store_survives_corrupt_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("drafts.json");
        std::fs::write(&path, "not json{").unwrap();
        assert!(load_local_draft(&path, "new").is_none());
        store_local_draft(&path, "new", "ok").unwrap();
        assert_eq!(load_local_draft(&path, "new").unwrap().content, "ok");
    }

    #[test]
    fn clear_local_drafts_empties_store() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("drafts.json");
        store_local_draft(&path, "new", "a").unwrap();
        store_local_draft(&path, "note:note-1", "b").unwrap();
        clear_local_drafts(&path).unwrap();
        assert!(load_all_local_drafts(&path).is_empty());
    }
}

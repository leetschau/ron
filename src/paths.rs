//! Locate on-disk paths used by the ron server.
//!
//! Layout under the user's home:
//! ```text
//! ~/.local/share/ron/        <- app_home
//!   db.sqlite3               <- SQLite working store
//!   repo/                    <- YAML files (git-tracked by the server)
//!     notes/note-*.yaml
//!     pulses/pulse-*.yaml
//!     metrics/metric-*.yaml
//!     resources/             <- note attachments (`resources/<name>` refs)
//! ~/.config/ron/
//!   server.json              <- listen address, optional viewer gate, the
//!                              `url` CLI clients dial as a fallback
//!                              when $RON_URL is unset, plus optional
//!                              `default_notebook` / `editor` / `viewer`
//!   tokens.json              <- bearer-token store (NOT committed to git)
//! ```

use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct Paths {
    pub app_home: PathBuf,
    pub db_path: PathBuf,
    pub repo_dir: PathBuf,
    pub config_dir: PathBuf,
    pub server_config: PathBuf,
    pub tokens_file: PathBuf,
}

impl Paths {
    pub fn detect() -> Result<Self> {
        let dirs = ProjectDirs::from("", "", "ron")
            .context("could not determine user directories")?;
        let app_home = dirs.data_local_dir().to_path_buf();
        let config_dir = dirs.config_dir().to_path_buf();
        let db_path = app_home.join("db.sqlite3");
        let repo_dir = app_home.join("repo");
        let server_config = config_dir.join("server.json");
        let tokens_file = config_dir.join("tokens.json");
        std::fs::create_dir_all(&app_home).with_context(|| format!("mkdir {}", app_home.display()))?;
        std::fs::create_dir_all(&config_dir)
            .with_context(|| format!("mkdir {}", config_dir.display()))?;
        std::fs::create_dir_all(&repo_dir)
            .with_context(|| format!("mkdir {}", repo_dir.display()))?;
        Ok(Self {
            app_home,
            db_path,
            repo_dir,
            config_dir,
            server_config,
            tokens_file,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_listen")]
    pub listen: String,
    /// URL CLI clients dial when `$RON_URL` is unset (e.g.
    /// `http://192.168.1.5:7780` on a second machine). Not a bind spec —
    /// that's `listen`; see `client::base_url` for the precedence order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Optional shared passphrase gating the browser viewer. When `None`,
    /// viewer routes stay open (the historical localhost-bind behaviour).
    /// When `Some`, viewer routes require a `ron_viewer` cookie obtained via
    /// `/?key=<secret>` or the `/login` form. See docs/phone-access.md.
    #[serde(default)]
    pub viewer_secret: Option<String>,
    /// Notebook used when a note is created without one (empty notebook
    /// field). The **server is the authority**: `create_note_inner` applies
    /// it server-side, the viewer form prefills from it, and the CLI fetches
    /// it via `GET /api/config` for the `ron add` prefill — the local value
    /// is only an offline fallback (see `client::server_default_notebook`).
    #[serde(default = "default_notebook")]
    pub default_notebook: String,
    /// Editor command (may include args, e.g. `code -w`) used by `ron add` /
    /// `ron edit`. Takes precedence over `$EDITOR`; fallback chain is
    /// this key → `$EDITOR` → `nvim`. CLI-side only — ignored by the server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor: Option<String>,
    /// Command (args allowed) `ron view` pipes the note through instead of
    /// plain cat-style stdout. Set to `""` to get the raw print back.
    /// CLI-side only — ignored by the server.
    #[serde(default = "default_cli_viewer")]
    pub cli_viewer: String,
    /// Serve the browser viewer (HTML routes + `/resources/*`)? Set `false`
    /// for an API-only server. `true` by default.
    #[serde(default = "default_viewer")]
    pub viewer: bool,
}

fn default_listen() -> String {
    // Bind on all interfaces so a phone (or a remote CLI host) on the LAN can
    // reach the server. The viewer gate (`viewer_secret`) and loopback-only
    // `/api/tokens` keep the surfaces protected; see docs/phone-access.md.
    "0.0.0.0:7780".to_string()
}

fn default_notebook() -> String {
    "default".to_string()
}

fn default_cli_viewer() -> String {
    "mdless".to_string()
}

fn default_viewer() -> bool {
    true
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen: default_listen(),
            url: None,
            viewer_secret: None,
            default_notebook: default_notebook(),
            editor: None,
            cli_viewer: default_cli_viewer(),
            viewer: default_viewer(),
        }
    }
}

/// Read the config file if it exists, without creating anything (unlike
/// `ServerConfig::load`). Returns `None` when the file is absent or
/// unparseable — a remote CLI host may have no config and should stay that
/// way.
fn read_existing_config() -> Option<ServerConfig> {
    let dirs = ProjectDirs::from("", "", "ron")?;
    let path = dirs.config_dir().join("server.json");
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Read the `url` key from `~/.config/ron/server.json`, if the file exists
/// and sets one. Unlike `Paths::detect` / `ServerConfig::load` this never
/// creates files or directories — a remote CLI host may have nothing but
/// `cli-token.json` and should stay that way.
pub fn read_configured_url() -> Option<String> {
    read_existing_config().and_then(|c| c.url)
}

/// Read `default_notebook` without creating anything; `"default"` when the
/// file or key is absent.
pub fn read_default_notebook() -> String {
    read_existing_config()
        .map(|c| c.default_notebook)
        .unwrap_or_else(default_notebook)
}

/// Read the configured `editor` command (args included) without creating
/// anything; `None` when unset.
pub fn read_editor() -> Option<String> {
    read_existing_config().and_then(|c| c.editor)
}

/// Read the `cli_viewer` command `ron view` pipes notes through, without
/// creating anything; `"mdless"` when the file or key is absent. An empty
/// string means "print raw, no viewer".
pub fn read_cli_viewer() -> String {
    read_existing_config()
        .map(|c| c.cli_viewer)
        .unwrap_or_else(default_cli_viewer)
}

impl ServerConfig {
    pub fn load(paths: &Paths) -> Result<Self> {
        if paths.server_config.exists() {
            let text = std::fs::read_to_string(&paths.server_config)
                .with_context(|| format!("reading {}", paths.server_config.display()))?;
            Ok(serde_json::from_str(&text).unwrap_or_default())
        } else {
            let cfg = Self::default();
            cfg.save(paths)?;
            Ok(cfg)
        }
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(&paths.server_config, text)
            .with_context(|| format!("writing {}", paths.server_config.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn server_config_round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("server.json");
        let cfg = ServerConfig {
            listen: "127.0.0.1:9000".into(),
            url: Some("http://192.168.1.5:9000".into()),
            viewer_secret: Some("hush".into()),
            default_notebook: "journal".into(),
            editor: Some("code -w".into()),
            cli_viewer: "bat -l md".into(),
            viewer: false,
        };
        std::fs::write(&path, serde_json::to_string(&cfg).unwrap()).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let back: ServerConfig = serde_json::from_str(&text).unwrap();
        assert_eq!(back.listen, "127.0.0.1:9000");
        assert_eq!(back.url.as_deref(), Some("http://192.168.1.5:9000"));
        assert_eq!(back.viewer_secret.as_deref(), Some("hush"));
        assert_eq!(back.default_notebook, "journal");
        assert_eq!(back.editor.as_deref(), Some("code -w"));
        assert_eq!(back.cli_viewer, "bat -l md");
        assert!(!back.viewer);
    }

    #[test]
    fn server_config_uses_defaults_when_partial() {
        let text = "{}";
        let cfg: ServerConfig = serde_json::from_str(text).unwrap();
        assert_eq!(cfg.listen, "0.0.0.0:7780");
        assert!(cfg.url.is_none());
        assert!(cfg.viewer_secret.is_none());
        assert_eq!(cfg.default_notebook, "default");
        assert!(cfg.editor.is_none());
        assert_eq!(cfg.cli_viewer, "mdless");
        assert!(cfg.viewer);
    }

    #[test]
    fn server_config_parses_new_keys() {
        let cfg: ServerConfig = serde_json::from_str(
            r#"{"default_notebook": "work", "editor": "code -w", "viewer": false}"#,
        )
        .unwrap();
        assert_eq!(cfg.default_notebook, "work");
        assert_eq!(cfg.editor.as_deref(), Some("code -w"));
        assert!(!cfg.viewer);
        assert_eq!(cfg.listen, "0.0.0.0:7780");
    }

    #[test]
    fn cli_viewer_can_be_emptied() {
        let cfg: ServerConfig = serde_json::from_str(r#"{"cli_viewer": ""}"#).unwrap();
        assert_eq!(cfg.cli_viewer, "");
    }

    #[test]
    fn server_config_omits_url_when_unset() {
        let text = serde_json::to_string(&ServerConfig::default()).unwrap();
        assert!(!text.contains("url"));
        assert!(!text.contains("editor"));
        assert!(text.contains("\"viewer\":true"));
        assert!(text.contains("\"default_notebook\":\"default\""));
        assert!(text.contains("\"cli_viewer\":\"mdless\""));
    }

    #[test]
    fn server_config_parses_url_alone() {
        let cfg: ServerConfig =
            serde_json::from_str(r#"{"url": "http://192.168.1.5:7780"}"#).unwrap();
        assert_eq!(cfg.url.as_deref(), Some("http://192.168.1.5:7780"));
        assert_eq!(cfg.listen, "0.0.0.0:7780");
    }
}

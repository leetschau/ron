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
//!   server.json              <- listen address, optional viewer gate, and
//!                              the `url` CLI clients dial as a fallback
//!                              when $RON_URL is unset
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
}

fn default_listen() -> String {
    // Bind on all interfaces so a phone (or a remote CLI host) on the LAN can
    // reach the server. The viewer gate (`viewer_secret`) and loopback-only
    // `/api/tokens` keep the surfaces protected; see docs/phone-access.md.
    "0.0.0.0:7780".to_string()
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen: default_listen(),
            url: None,
            viewer_secret: None,
        }
    }
}

/// Read the `url` key from `~/.config/ron/server.json`, if the file exists
/// and sets one. Unlike `Paths::detect` / `ServerConfig::load` this never
/// creates files or directories — a remote CLI host may have nothing but
/// `cli-token.json` and should stay that way.
pub fn read_configured_url() -> Option<String> {
    let dirs = ProjectDirs::from("", "", "ron")?;
    let path = dirs.config_dir().join("server.json");
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<ServerConfig>(&text).ok()?.url
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
        };
        std::fs::write(&path, serde_json::to_string(&cfg).unwrap()).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let back: ServerConfig = serde_json::from_str(&text).unwrap();
        assert_eq!(back.listen, "127.0.0.1:9000");
        assert_eq!(back.url.as_deref(), Some("http://192.168.1.5:9000"));
        assert_eq!(back.viewer_secret.as_deref(), Some("hush"));
    }

    #[test]
    fn server_config_uses_defaults_when_partial() {
        let text = "{}";
        let cfg: ServerConfig = serde_json::from_str(text).unwrap();
        assert_eq!(cfg.listen, "0.0.0.0:7780");
        assert!(cfg.url.is_none());
        assert!(cfg.viewer_secret.is_none());
    }

    #[test]
    fn server_config_parses_url_alone() {
        let cfg: ServerConfig =
            serde_json::from_str(r#"{"url": "http://192.168.1.5:7780"}"#).unwrap();
        assert_eq!(cfg.url.as_deref(), Some("http://192.168.1.5:7780"));
        assert_eq!(cfg.listen, "0.0.0.0:7780");
    }

    #[test]
    fn server_config_omits_url_when_unset() {
        let text = serde_json::to_string(&ServerConfig::default()).unwrap();
        assert!(!text.contains("url"));
    }
}

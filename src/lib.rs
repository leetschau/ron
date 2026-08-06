//! ron: a note-taking / habit-tracker / metric-tracker app.
//!
//! v2.x architecture: a server owns a SQLite store and a git repo of YAML
//! files. Browser and CLI clients talk to it over a localhost REST API.
//!
//! This crate exposes the data layer (models, DB, YAML, migration) so it can
//! be exercised directly from tests or a CLI without going through HTTP.

pub mod db;
pub mod id;
pub mod migrate;
pub mod models;
pub mod paths;
pub mod server;
pub mod token;
pub mod viewer;
pub mod yaml;

pub use models::{Metric, Note, Pulse};

/// Format version of the YAML on-disk files.
pub const FORMAT_VERSION: u32 = yaml::FORMAT_VERSION;

pub use paths::{Paths, ServerConfig};

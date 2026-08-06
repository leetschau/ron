//! YAML serialization for the on-disk format.
//!
//! Each item (Note, Pulse, Metric) is stored in its own file under the repo
//! directory. The file name is `<id>.yaml` (NOT `.md`, since the body lives
//! inside YAML as a string). All files carry a `version` and a `type` field
//! for forward-compatible migrations.
//!
//! Example (note):
//! ```yaml
//! version: 2
//! type: note
//! id: note-20260806-1432-a8f
//! title: Hello
//! tags: [greeting]
//! notebook: default
//! created: 2026-08-06T14:32:00
//! updated: 2026-08-06T14:32:00
//! related: []
//! body: |
//!   # Hi
//!
//!   world
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::models::{Metric, Note, Pulse};

/// On-disk format version. Bumped on every breaking change to the YAML shape.
pub const FORMAT_VERSION: u32 = 2;

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Item {
    Note(Note),
    Pulse(Pulse),
    Metric(Metric),
}

/// Wrapper that injects/validates the `version` field on write/read.
#[derive(Serialize, Deserialize, Debug)]
struct Versioned {
    version: u32,
    #[serde(flatten)]
    item: Item,
}

pub fn serialize(note: &Note) -> Result<String> {
    let v = Versioned {
        version: FORMAT_VERSION,
        item: Item::Note(note.clone()),
    };
    Ok(serde_yaml::to_string(&v)?)
}

pub fn serialize_pulse(pulse: &Pulse) -> Result<String> {
    let v = Versioned {
        version: FORMAT_VERSION,
        item: Item::Pulse(pulse.clone()),
    };
    Ok(serde_yaml::to_string(&v)?)
}

pub fn serialize_metric(metric: &Metric) -> Result<String> {
    let v = Versioned {
        version: FORMAT_VERSION,
        item: Item::Metric(metric.clone()),
    };
    Ok(serde_yaml::to_string(&v)?)
}

/// Parse any versioned item from YAML text.
pub fn parse(text: &str) -> Result<Item> {
    let v: Versioned = serde_yaml::from_str(text).context("parsing YAML item")?;
    if v.version != FORMAT_VERSION {
        anyhow::bail!(
            "unsupported on-disk version {}: this build handles {}",
            v.version,
            FORMAT_VERSION
        );
    }
    Ok(v.item)
}

/// Write an item to `<dir>/<id>.yaml`.
pub fn write_item(dir: &Path, item: &Item) -> Result<PathBuf> {
    let (id, text) = match item {
        Item::Note(n) => (n.id.clone(), serialize(n)?),
        Item::Pulse(p) => (p.id.clone(), serialize_pulse(p)?),
        Item::Metric(m) => (m.id.clone(), serialize_metric(m)?),
    };
    let path = dir.join(format!("{id}.yaml"));
    fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

/// Read a single item from a YAML file.
pub fn read_item(path: &Path) -> Result<Item> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse(&text)
}

/// Read all items in a directory. Non-`.yaml` files and parse errors are
/// skipped with a warning printed to stderr.
pub fn read_all(dir: &Path) -> Result<Vec<Item>> {
    let mut items = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("yaml") {
            continue;
        }
        match read_item(&path) {
            Ok(item) => items.push(item),
            Err(e) => eprintln!("skip {}: {e:#}", path.display()),
        }
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Interval, MetricPoint};

    fn note() -> Note {
        Note::new(
            "note-20260806-1432-a8f".into(),
            "Hello".into(),
            vec!["greeting".into(), "misc".into()],
            "default".into(),
            "2026-08-06T14:32:00".parse().unwrap(),
            "2026-08-06T14:32:00".parse().unwrap(),
            "# Hi\n\nworld\n".into(),
        )
    }

    #[test]
    fn note_yaml_round_trip() {
        let original = note();
        let text = serialize(&original).unwrap();
        assert!(text.contains("version: 2"));
        assert!(text.contains("type: note"));
        let parsed = parse(&text).unwrap();
        match parsed {
            Item::Note(n) => {
                assert_eq!(n.id, original.id);
                assert_eq!(n.title, original.title);
                assert_eq!(n.tags, original.tags);
                assert_eq!(n.notebook, original.notebook);
                assert_eq!(n.created, original.created);
                assert_eq!(n.body, original.body);
            }
            _ => panic!("expected Note"),
        }
    }

    #[test]
    fn body_preserves_internal_newlines() {
        let mut n = note();
        n.body = "line 1\n\nline 2\n```rs\nfn main() {}\n```\n".into();
        let text = serialize(&n).unwrap();
        let back = parse(&text).unwrap();
        match back {
            Item::Note(parsed) => assert_eq!(parsed.body, n.body),
            _ => panic!("expected Note"),
        }
    }

    #[test]
    fn pulse_yaml_round_trip() {
        let mut p = Pulse::new(
            "pulse-20260806-1432-000".into(),
            "jog".into(),
            Interval::Daily,
            "2026-08-06T08:00:00".parse().unwrap(),
        );
        p.set_slot("2026-08-06", true);
        p.set_slot("2026-08-05", false);
        let text = serialize_pulse(&p).unwrap();
        assert!(text.contains("type: pulse"));
        let parsed = parse(&text).unwrap();
        match parsed {
            Item::Pulse(back) => {
                assert_eq!(back.id, p.id);
                assert_eq!(back.interval, Interval::Daily);
                assert_eq!(back.get_slot("2026-08-06"), Some(true));
                assert_eq!(back.get_slot("2026-08-05"), Some(false));
            }
            _ => panic!("expected Pulse"),
        }
    }

    #[test]
    fn metric_yaml_round_trip() {
        let mut m = Metric::new(
            "metric-20260806-1432-000".into(),
            "weight".into(),
            "2026-08-06T08:00:00".parse().unwrap(),
        );
        m.append("2026-08-06T08:00:00".parse().unwrap(), 72.5);
        let text = serialize_metric(&m).unwrap();
        assert!(text.contains("type: metric"));
        let parsed = parse(&text).unwrap();
        match parsed {
            Item::Metric(back) => {
                assert_eq!(back.id, m.id);
                assert_eq!(back.points.len(), 1);
                assert!(matches!(
                    back.points[0],
                    MetricPoint { value, .. } if (value - 72.5).abs() < 1e-9
                ));
            }
            _ => panic!("expected Metric"),
        }
    }

    #[test]
    fn version_mismatch_is_rejected() {
        let text = serialize(&note()).unwrap().replace("version: 2", "version: 3");
        assert!(parse(&text).is_err());
    }
}

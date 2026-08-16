//! Draft model: an in-flight note edit cached for crash/network recovery.
//!
//! Drafts are transient working copies, never exported to YAML/git. One
//! live draft per key: `new` (an unpublished note) or `note:<id>` (pending
//! edits to an existing note). When the note is saved, the draft is
//! "consumed" — its `updated` timestamp is kept as a watermark so clients
//! holding stale local copies can drop them.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

/// Structured content of a draft. Mirrors the fields of the note form /
/// editor buffer so the viewer and the CLI can round-trip it losslessly.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DraftContent {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub notebook: String,
    #[serde(default)]
    pub related: Vec<String>,
    #[serde(default)]
    pub body: String,
}

/// A live draft: its key, content, and when it was last saved.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Draft {
    pub key: String,
    pub content: DraftContent,
    pub updated: NaiveDateTime,
}

/// A valid draft key: `new`, or `note:<note-id>`.
pub fn valid_draft_key(key: &str) -> bool {
    if key == "new" {
        return true;
    }
    match key.strip_prefix("note:") {
        Some(id) => !id.is_empty() && !id.contains('/') && id.len() < 100,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_content_round_trips() {
        let c = DraftContent {
            title: "t".into(),
            tags: vec!["a".into(), "b".into()],
            notebook: "nb".into(),
            related: vec!["note-1".into()],
            body: "hello".into(),
        };
        let json = serde_json::to_string(&c).unwrap();
        let back: DraftContent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn draft_content_tolerates_missing_fields() {
        let c: DraftContent = serde_json::from_str(r#"{"title":"x"}"#).unwrap();
        assert_eq!(c.title, "x");
        assert!(c.tags.is_empty());
        assert!(c.body.is_empty());
    }

    #[test]
    fn draft_key_validation() {
        assert!(valid_draft_key("new"));
        assert!(valid_draft_key("note:note-20260816-1432-a8f"));
        assert!(!valid_draft_key(""));
        assert!(!valid_draft_key("note:"));
        assert!(!valid_draft_key("note:a/b"));
        assert!(!valid_draft_key("pulse:p1"));
        assert!(!valid_draft_key("note/../etc"));
        let long = format!("note:{}", "x".repeat(200));
        assert!(!valid_draft_key(&long));
    }
}

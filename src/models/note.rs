//! Note model.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

/// Reference to a related note, by ID.
pub type RelatedRef = String;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
    pub notebook: String,
    pub created: NaiveDateTime,
    pub updated: NaiveDateTime,
    /// IDs of notes related to this one. Stored verbatim; validity is checked
    /// by the DB/API layer when needed.
    pub related: Vec<RelatedRef>,
    /// Markdown body. Rendered to HTML in browser view mode.
    pub body: String,
}

impl Note {
    pub fn new(
        id: String,
        title: String,
        tags: Vec<String>,
        notebook: String,
        created: NaiveDateTime,
        updated: NaiveDateTime,
        body: String,
    ) -> Self {
        Self {
            id,
            title,
            tags,
            notebook,
            created,
            updated,
            related: Vec::new(),
            body,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Note {
        Note::new(
            "note-20260806-1432-a8f".into(),
            "Hello".into(),
            vec!["greeting".into()],
            "default".into(),
            "2026-08-06T14:32:00".parse().unwrap(),
            "2026-08-06T14:32:00".parse().unwrap(),
            "# Hi\n\nworld".into(),
        )
    }

    #[test]
    fn note_serializes_with_serde() {
        let n = sample();
        let json = serde_json::to_string(&n).unwrap();
        let back: Note = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, n.id);
        assert_eq!(back.title, n.title);
        assert_eq!(back.tags, n.tags);
        assert_eq!(back.body, n.body);
    }
}

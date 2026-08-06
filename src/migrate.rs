//! Migrate notes from the 1.x markdown format to the 2.x YAML format.
//!
//! 1.x file shape (see `src/lib.rs:236-248` of the original codebase):
//! ```text
//! Title: <title>
//! Tags: <tag1; tag2>
//! Notebook: <notebook>
//! Created: YYYY-MM-DD HH:MM:SS
//! Updated: YYYY-MM-DD HH:MM:SS
//!
//! ------
//!
//! <markdown body>
//! ```
//!
//! 2.x output: a Note serialized via [`crate::yaml`] with a fresh ID.

use std::path::Path;

use anyhow::{anyhow, Context, Result};
use chrono::{NaiveDateTime, TimeZone};

use crate::id::{new_id_at, Kind};
use crate::models::Note;

/// Parse a single 1.x markdown note. Returns `Ok(None)` if the file is empty
/// or doesn't look like a 1.x note (no `Title:` line).
pub fn parse_v1(text: &str) -> Result<Option<ParsedV1>> {
    let mut lines = text.lines();
    let title_line = match lines.next() {
        Some(l) => l,
        None => return Ok(None),
    };
    let title = title_line
        .strip_prefix("Title: ")
        .ok_or_else(|| anyhow!("missing `Title: ` prefix: {title_line:?}"))?
        .to_string();

    let tag_line = lines.next().ok_or_else(|| anyhow!("truncated note (tags)"))?;
    let tags_str = tag_line
        .strip_prefix("Tags: ")
        .ok_or_else(|| anyhow!("missing `Tags: ` prefix: {tag_line:?}"))?;
    let tags: Vec<String> = if tags_str.trim().is_empty() {
        Vec::new()
    } else {
        tags_str.split("; ").map(str::to_string).collect()
    };

    let nb_line = lines.next().ok_or_else(|| anyhow!("truncated note (notebook)"))?;
    let notebook = nb_line
        .strip_prefix("Notebook: ")
        .ok_or_else(|| anyhow!("missing `Notebook: ` prefix: {nb_line:?}"))?
        .to_string();

    let created_line = lines.next().ok_or_else(|| anyhow!("truncated note (created)"))?;
    let created_str = created_line
        .strip_prefix("Created: ")
        .ok_or_else(|| anyhow!("missing `Created: ` prefix: {created_line:?}"))?;
    let created = NaiveDateTime::parse_from_str(created_str, "%Y-%m-%d %H:%M:%S")
        .with_context(|| format!("bad Created timestamp: {created_str}"))?;

    let updated_line = lines.next().ok_or_else(|| anyhow!("truncated note (updated)"))?;
    let updated_str = updated_line
        .strip_prefix("Updated: ")
        .ok_or_else(|| anyhow!("missing `Updated: ` prefix: {updated_line:?}"))?;
    let updated = NaiveDateTime::parse_from_str(updated_str, "%Y-%m-%d %H:%M:%S")
        .with_context(|| format!("bad Updated timestamp: {updated_str}"))?;

    // Skip the blank line, the `------` separator, and the blank line after it.
    // Be tolerant: skip until we've passed the first `------` fence, then
    // take everything that remains as the body.
    let mut saw_fence = false;
    let mut body_lines: Vec<&str> = Vec::new();
    for line in lines {
        if !saw_fence {
            if line.trim_start().starts_with("------") {
                saw_fence = true;
            }
            continue;
        }
        body_lines.push(line);
    }
    // Trim a single leading blank line if present.
    if body_lines.first().map_or(false, |l| l.is_empty()) {
        body_lines.remove(0);
    }
    let body = body_lines.join("\n");

    Ok(Some(ParsedV1 {
        title,
        tags,
        notebook,
        created,
        updated,
        body,
    }))
}

pub struct ParsedV1 {
    pub title: String,
    pub tags: Vec<String>,
    pub notebook: String,
    pub created: NaiveDateTime,
    pub updated: NaiveDateTime,
    pub body: String,
}

impl ParsedV1 {
    /// Convert into a Note. `id_source` decides whether the ID is regenerated
    /// (using `created` for the date/time prefix) or copied from a provided
    /// string (useful when the original 1.x filename should be preserved).
    pub fn into_note(self, id: Option<String>) -> Note {
        let note_id = id.unwrap_or_else(|| {
            // Use the local time at `created` for the ID timestamp portion.
            let local = chrono::Local.from_local_datetime(&self.created).unwrap();
            new_id_at(Kind::Note, &local)
        });
        Note {
            id: note_id,
            title: self.title,
            tags: self.tags,
            notebook: self.notebook,
            created: self.created,
            updated: self.updated,
            related: Vec::new(),
            body: self.body,
        }
    }
}

/// Migrate every `.md` file under `src_dir` into 2.x YAML files under
/// `dst_dir`. Returns the count of successfully migrated notes.
/// Files that fail to parse are reported but don't abort the batch.
pub fn migrate_dir(src_dir: &Path, dst_dir: &Path) -> MigrateReport {
    let mut report = MigrateReport::default();
    if let Err(e) = std::fs::create_dir_all(dst_dir) {
        report.fatal = Some(format!("could not create {}: {e}", dst_dir.display()));
        return report;
    }
    let entries = match std::fs::read_dir(src_dir) {
        Ok(e) => e,
        Err(e) => {
            report.fatal = Some(format!("could not read {}: {e}", src_dir.display()));
            return report;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                report.failed.push((path, format!("read: {e}")));
                continue;
            }
        };
        match parse_v1(&text) {
            Ok(Some(parsed)) => {
                // Generate a fresh v2 ID using the note's `created` time.
                // V1 had no cross-references, so the old filename carries no
                // information worth preserving.
                let note = parsed.into_note(None);
                match crate::yaml::write_item(dst_dir, &crate::yaml::Item::Note(note.clone())) {
                    Ok(_) => report.succeeded.push((path, note.id)),
                    Err(e) => report.failed.push((path, format!("write: {e:#}"))),
                }
            }
            Ok(None) => report.skipped.push(path),
            Err(e) => report.failed.push((path, format!("parse: {e:#}"))),
        }
    }
    report
}

#[derive(Default, Debug)]
pub struct MigrateReport {
    pub succeeded: Vec<(std::path::PathBuf, String)>,
    pub failed: Vec<(std::path::PathBuf, String)>,
    pub skipped: Vec<std::path::PathBuf>,
    pub fatal: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    fn sample_v1() -> &'static str {
        "Title: Hello world\n\
         Tags: greeting; misc\n\
         Notebook: default\n\
         Created: 2026-08-06 14:32:00\n\
         Updated: 2026-08-06 14:32:00\n\
         \n\
         ------\n\
         \n\
         # Hi\n\
         \n\
         This is the body.\n"
    }

    #[test]
    fn parses_v1_note() {
        let parsed = parse_v1(sample_v1()).unwrap().unwrap();
        assert_eq!(parsed.title, "Hello world");
        assert_eq!(parsed.tags, vec!["greeting".to_string(), "misc".to_string()]);
        assert_eq!(parsed.notebook, "default");
        assert_eq!(parsed.created.format("%Y-%m-%d").to_string(), "2026-08-06");
        assert!(parsed.body.contains("# Hi"));
        assert!(parsed.body.contains("This is the body."));
    }

    #[test]
    fn empty_file_returns_none() {
        assert!(parse_v1("").unwrap().is_none());
    }

    #[test]
    fn missing_title_prefix_errors() {
        assert!(parse_v1("Hello\n").is_err());
    }

    #[test]
    fn into_note_keeps_provided_id() {
        let parsed = parse_v1(sample_v1()).unwrap().unwrap();
        let note = parsed.into_note(Some("custom-id".into()));
        assert_eq!(note.id, "custom-id");
    }

    #[test]
    fn migrate_dir_round_trip() {
        let src = tempdir().unwrap();
        let dst = tempdir().unwrap();
        let mut f = std::fs::File::create(src.path().join("note210806143200.md")).unwrap();
        f.write_all(sample_v1().as_bytes()).unwrap();
        drop(f);

        let report = migrate_dir(src.path(), dst.path());
        assert!(report.fatal.is_none(), "{:?}", report.fatal);
        assert_eq!(report.succeeded.len(), 1);
        assert_eq!(report.failed.len(), 0);
        let items = crate::yaml::read_all(dst.path()).unwrap();
        assert_eq!(items.len(), 1);
        match items.into_iter().next().unwrap() {
            crate::yaml::Item::Note(n) => {
                // Fresh v2 ID shape: note-YYYYMMDD-HHMM-xxx
                assert!(n.id.starts_with("note-20260806-1432-"));
                assert_eq!(n.title, "Hello world");
            }
            _ => panic!("expected Note"),
        }
    }
}

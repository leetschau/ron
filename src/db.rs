//! SQLite storage layer.
//!
//! SQLite is the working store; YAML files on disk are the source of truth on
//! cold start / sync. The DB is rebuilt from YAML by the server's `import`
//! command. Schema version lives in the `meta` table.

use std::path::Path;

use anyhow::{Context, Result};
use chrono::NaiveDateTime;
use rusqlite::{params, Connection, OptionalExtension};

use crate::models::{Interval, Metric, MetricPoint, Note, Pulse, PulseSlot};

pub const SCHEMA_VERSION: u32 = 1;

const SCHEMA: &str = r#"
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS notes (
    id       TEXT PRIMARY KEY,
    title    TEXT NOT NULL,
    tags     TEXT NOT NULL,           -- JSON array
    notebook TEXT NOT NULL,
    created  TEXT NOT NULL,           -- RFC3339-ish "YYYY-MM-DDTHH:MM:SS"
    updated  TEXT NOT NULL,
    related  TEXT NOT NULL DEFAULT '[]',  -- JSON array of note IDs
    body     TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_notes_updated ON notes(updated);
CREATE INDEX IF NOT EXISTS idx_notes_created ON notes(created);
CREATE INDEX IF NOT EXISTS idx_notes_notebook ON notes(notebook);

CREATE TABLE IF NOT EXISTS pulses (
    id       TEXT PRIMARY KEY,
    topic    TEXT NOT NULL,
    interval TEXT NOT NULL,           -- "daily" | "weekly" | "monthly" | "yearly"
    created  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS pulse_slots (
    pulse_id TEXT NOT NULL,
    slot     TEXT NOT NULL,
    checked  INTEGER NOT NULL,        -- 0 / 1
    PRIMARY KEY (pulse_id, slot),
    FOREIGN KEY (pulse_id) REFERENCES pulses(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS metrics (
    id      TEXT PRIMARY KEY,
    topic   TEXT NOT NULL,
    created TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS metric_points (
    metric_id TEXT NOT NULL,
    ts        TEXT NOT NULL,
    value     REAL NOT NULL,
    PRIMARY KEY (metric_id, ts),
    FOREIGN KEY (metric_id) REFERENCES metrics(id) ON DELETE CASCADE
);
"#;

pub fn open(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
    conn.execute_batch(SCHEMA)?;
    let current: Option<u32> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |r| r.get::<_, String>(0),
        )
        .optional()?
        .and_then(|s| s.parse().ok());
    match current {
        Some(v) if v == SCHEMA_VERSION => {}
        None => {
            conn.execute(
                "INSERT INTO meta (key, value) VALUES ('schema_version', ?1)",
                params![SCHEMA_VERSION.to_string()],
            )?;
        }
        Some(v) => {
            anyhow::bail!(
                "DB schema version {} is newer/different than this build supports ({})",
                v,
                SCHEMA_VERSION
            );
        }
    }
    Ok(conn)
}

fn ts_to_str(t: NaiveDateTime) -> String {
    t.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn ts_from_str(s: &str) -> Result<NaiveDateTime> {
    Ok(s.parse()?)
}

// ----- Notes ----------------------------------------------------------------

/// Serialize an Interval as its lowercase keyword.
fn interval_to_str(i: Interval) -> &'static str {
    match i {
        Interval::Daily => "daily",
        Interval::Weekly => "weekly",
        Interval::Monthly => "monthly",
        Interval::Yearly => "yearly",
    }
}

fn interval_from_str(s: &str) -> Result<Interval> {
    Ok(match s {
        "daily" => Interval::Daily,
        "weekly" => Interval::Weekly,
        "monthly" => Interval::Monthly,
        "yearly" => Interval::Yearly,
        other => anyhow::bail!("unknown interval {other}"),
    })
}

pub fn upsert_note(conn: &Connection, note: &Note) -> Result<()> {
    let tags = serde_json::to_string(&note.tags)?;
    let related = serde_json::to_string(&note.related)?;
    conn.execute(
        "INSERT INTO notes (id, title, tags, notebook, created, updated, related, body)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(id) DO UPDATE SET
            title=excluded.title, tags=excluded.tags, notebook=excluded.notebook,
            created=excluded.created, updated=excluded.updated,
            related=excluded.related, body=excluded.body",
        params![
            note.id,
            note.title,
            tags,
            note.notebook,
            ts_to_str(note.created),
            ts_to_str(note.updated),
            related,
            note.body,
        ],
    )?;
    Ok(())
}

pub fn get_note(conn: &Connection, id: &str) -> Result<Option<Note>> {
    let row = conn
        .query_row("SELECT * FROM notes WHERE id = ?1", params![id], note_from_row)
        .optional()?;
    Ok(row)
}

fn note_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Note> {
    let tags_json: String = row.get("tags")?;
    let related_json: String = row.get("related")?;
    let created: String = row.get("created")?;
    let updated: String = row.get("updated")?;
    Ok(Note {
        id: row.get("id")?,
        title: row.get("title")?,
        tags: serde_json::from_str(&tags_json).unwrap_or_default(),
        notebook: row.get("notebook")?,
        created: ts_from_str(&created).unwrap_or_else(|_| chrono::Local::now().naive_local()),
        updated: ts_from_str(&updated).unwrap_or_else(|_| chrono::Local::now().naive_local()),
        related: serde_json::from_str(&related_json).unwrap_or_default(),
        body: row.get("body")?,
    })
}

pub fn delete_note(conn: &Connection, id: &str) -> Result<bool> {
    let n = conn.execute("DELETE FROM notes WHERE id = ?1", params![id])?;
    Ok(n > 0)
}

pub fn list_notes(conn: &Connection, limit: Option<u32>) -> Result<Vec<Note>> {
    let sql = match limit {
        Some(n) => format!("SELECT * FROM notes ORDER BY updated DESC LIMIT {n}"),
        None => "SELECT * FROM notes ORDER BY updated DESC".to_string(),
    };
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], note_from_row)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// Text matching options for note search.
#[derive(Clone, Copy, Debug)]
pub struct NoteMatch {
    pub ignore_case: bool,
    pub whole_word: bool,
}

impl Default for NoteMatch {
    fn default() -> Self {
        Self {
            ignore_case: true,
            whole_word: false,
        }
    }
}

/// Search field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteField {
    Title,
    Tags,
    Notebook,
    Content, // title + tags + notebook + body
}

/// In-memory search since SQLite FTS would be its own migration. For a
/// single-user dataset this is plenty.
pub fn search_notes(
    conn: &Connection,
    field: NoteField,
    pattern: &str,
    opts: NoteMatch,
) -> Result<Vec<Note>> {
    let all = list_notes(conn, None)?;
    let norm = |s: String| -> String {
        if opts.ignore_case {
            s.to_lowercase()
        } else {
            s
        }
    };
    let ptn = norm(pattern.to_string());
    let matches = |target: &str| -> bool {
        let t = norm(target.to_string());
        if opts.whole_word {
            t.split_whitespace().any(|w| w == ptn)
        } else {
            t.contains(&ptn)
        }
    };
    let mut out = Vec::new();
    for n in all {
        let hit = match field {
            NoteField::Title => matches(&n.title),
            NoteField::Tags => matches(&n.tags.join("; ")),
            NoteField::Notebook => matches(&n.notebook),
            NoteField::Content => {
                let combined = format!("{}\n{}\n{}\n{}", n.title, n.tags.join("; "), n.notebook, n.body);
                matches(&combined)
            }
        };
        if hit {
            out.push(n);
        }
    }
    Ok(out)
}

// ----- Pulses ----------------------------------------------------------------

pub fn upsert_pulse(conn: &Connection, pulse: &Pulse) -> Result<()> {
    conn.execute(
        "INSERT INTO pulses (id, topic, interval, created)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(id) DO UPDATE SET
            topic=excluded.topic, interval=excluded.interval, created=excluded.created",
        params![pulse.id, pulse.topic, interval_to_str(pulse.interval), ts_to_str(pulse.created)],
    )?;
    conn.execute(
        "DELETE FROM pulse_slots WHERE pulse_id = ?1",
        params![pulse.id],
    )?;
    for s in &pulse.slots {
        conn.execute(
            "INSERT INTO pulse_slots (pulse_id, slot, checked) VALUES (?1, ?2, ?3)
             ON CONFLICT(pulse_id, slot) DO UPDATE SET checked=excluded.checked",
            params![pulse.id, s.slot, if s.checked { 1 } else { 0 }],
        )?;
    }
    Ok(())
}

pub fn get_pulse(conn: &Connection, id: &str) -> Result<Option<Pulse>> {
    let pulse = conn
        .query_row("SELECT * FROM pulses WHERE id = ?1", params![id], |row| {
            let interval_str: String = row.get("interval")?;
            let interval = match interval_from_str(&interval_str) {
                Ok(i) => i,
                Err(e) => {
                    return Err(rusqlite::Error::FromSqlConversionFailure(
                        1,
                        rusqlite::types::Type::Text,
                        e.to_string().into(),
                    ))
                }
            };
            let created_str: String = row.get("created")?;
            Ok(Pulse {
                id: row.get("id")?,
                topic: row.get("topic")?,
                interval,
                created: ts_from_str(&created_str).unwrap_or_else(|_| chrono::Local::now().naive_local()),
                slots: Vec::new(),
            })
        })
        .optional()?;
    let Some(mut pulse) = pulse else { return Ok(None) };
    let mut stmt = conn.prepare("SELECT slot, checked FROM pulse_slots WHERE pulse_id = ?1")?;
    let rows = stmt.query_map(params![id], |row| {
        Ok(PulseSlot {
            slot: row.get(0)?,
            checked: row.get::<_, i64>(1)? != 0,
        })
    })?;
    for r in rows {
        pulse.slots.push(r?);
    }
    Ok(Some(pulse))
}

pub fn list_pulses(conn: &Connection) -> Result<Vec<Pulse>> {
    let mut stmt = conn.prepare("SELECT id FROM pulses ORDER BY created")?;
    let ids: Vec<String> = stmt.query_map([], |r| r.get::<_, String>(0))?.filter_map(Result::ok).collect();
    let mut out = Vec::new();
    for id in ids {
        if let Some(p) = get_pulse(conn, &id)? {
            out.push(p);
        }
    }
    Ok(out)
}

pub fn delete_pulse(conn: &Connection, id: &str) -> Result<bool> {
    let n = conn.execute("DELETE FROM pulses WHERE id = ?1", params![id])?;
    Ok(n > 0)
}

// ----- Metrics ---------------------------------------------------------------

pub fn upsert_metric(conn: &Connection, metric: &Metric) -> Result<()> {
    conn.execute(
        "INSERT INTO metrics (id, topic, created)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(id) DO UPDATE SET topic=excluded.topic, created=excluded.created",
        params![metric.id, metric.topic, ts_to_str(metric.created)],
    )?;
    conn.execute(
        "DELETE FROM metric_points WHERE metric_id = ?1",
        params![metric.id],
    )?;
    for p in &metric.points {
        conn.execute(
            "INSERT INTO metric_points (metric_id, ts, value) VALUES (?1, ?2, ?3)
             ON CONFLICT(metric_id, ts) DO UPDATE SET value=excluded.value",
            params![metric.id, ts_to_str(p.ts), p.value],
        )?;
    }
    Ok(())
}

pub fn get_metric(conn: &Connection, id: &str) -> Result<Option<Metric>> {
    let metric = conn
        .query_row("SELECT * FROM metrics WHERE id = ?1", params![id], |row| {
            let created_str: String = row.get("created")?;
            Ok(Metric {
                id: row.get("id")?,
                topic: row.get("topic")?,
                created: ts_from_str(&created_str).unwrap_or_else(|_| chrono::Local::now().naive_local()),
                points: Vec::new(),
            })
        })
        .optional()?;
    let Some(mut metric) = metric else { return Ok(None) };
    let mut stmt = conn.prepare("SELECT ts, value FROM metric_points WHERE metric_id = ?1 ORDER BY ts")?;
    let rows = stmt.query_map(params![id], |row| {
        let ts_str: String = row.get(0)?;
        Ok(MetricPoint {
            ts: ts_from_str(&ts_str).unwrap_or_else(|_| chrono::Local::now().naive_local()),
            value: row.get(1)?,
        })
    })?;
    for r in rows {
        metric.points.push(r?);
    }
    Ok(Some(metric))
}

pub fn list_metrics(conn: &Connection) -> Result<Vec<Metric>> {
    let mut stmt = conn.prepare("SELECT id FROM metrics ORDER BY created")?;
    let ids: Vec<String> = stmt.query_map([], |r| r.get::<_, String>(0))?.filter_map(Result::ok).collect();
    let mut out = Vec::new();
    for id in ids {
        if let Some(m) = get_metric(conn, &id)? {
            out.push(m);
        }
    }
    Ok(out)
}

pub fn delete_metric(conn: &Connection, id: &str) -> Result<bool> {
    let n = conn.execute("DELETE FROM metrics WHERE id = ?1", params![id])?;
    Ok(n > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Interval, Pulse, PulseSlot};
    use tempfile::NamedTempFile;

    fn conn() -> Connection {
        let tmp = NamedTempFile::new().unwrap().into_temp_path().keep().unwrap();
        open(&tmp).unwrap()
    }

    fn now() -> NaiveDateTime {
        NaiveDateTime::parse_from_str("2026-08-06T14:32:00", "%Y-%m-%dT%H:%M:%S").unwrap()
    }

    #[test]
    fn note_crud() {
        let conn = conn();
        let note = Note::new(
            "note-x".into(),
            "Hello world".into(),
            vec!["g".into()],
            "default".into(),
            now(),
            now(),
            "body".into(),
        );
        upsert_note(&conn, &note).unwrap();
        let fetched = get_note(&conn, "note-x").unwrap().unwrap();
        assert_eq!(fetched.title, "Hello world");
        assert_eq!(fetched.tags, vec!["g".to_string()]);
        let listed = list_notes(&conn, None).unwrap();
        assert_eq!(listed.len(), 1);
        assert!(delete_note(&conn, "note-x").unwrap());
        assert!(get_note(&conn, "note-x").unwrap().is_none());
    }

    #[test]
    fn search_notes_by_title_case_insensitive() {
        let conn = conn();
        let n1 = Note::new(
            "n1".into(),
            "PowerShell profile".into(),
            vec![],
            "nb".into(),
            now(),
            now(),
            "".into(),
        );
        let n2 = Note::new(
            "n2".into(),
            "Random".into(),
            vec![],
            "nb".into(),
            now(),
            now(),
            "".into(),
        );
        upsert_note(&conn, &n1).unwrap();
        upsert_note(&conn, &n2).unwrap();
        let hit = search_notes(&conn, NoteField::Title, "powershell", NoteMatch::default()).unwrap();
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].id, "n1");
    }

    #[test]
    fn pulse_crud_with_slots() {
        let conn = conn();
        let mut p = Pulse::new("p1".into(), "jog".into(), Interval::Daily, now());
        p.set_slot("2026-08-06", true);
        p.set_slot("2026-08-05", false);
        upsert_pulse(&conn, &p).unwrap();
        let back = get_pulse(&conn, "p1").unwrap().unwrap();
        assert_eq!(back.interval, Interval::Daily);
        assert_eq!(back.get_slot("2026-08-06"), Some(true));
        assert_eq!(back.get_slot("2026-08-05"), Some(false));
        assert!(delete_pulse(&conn, "p1").unwrap());
        assert!(get_pulse(&conn, "p1").unwrap().is_none());
    }

    #[test]
    fn metric_crud_with_points() {
        let conn = conn();
        let mut m = Metric::new("m1".into(), "weight".into(), now());
        m.append(now(), 72.5);
        m.append(now() + chrono::Duration::days(1), 73.0);
        upsert_metric(&conn, &m).unwrap();
        let back = get_metric(&conn, "m1").unwrap().unwrap();
        assert_eq!(back.points.len(), 2);
        let stats = back.stats(None, None).unwrap();
        assert!((stats.mean - 72.75).abs() < 1e-9);
        assert!(delete_metric(&conn, "m1").unwrap());
    }

    #[test]
    fn schema_version_is_stored() {
        let conn = conn();
        let v: String = conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(v, SCHEMA_VERSION.to_string());
    }

    // suppress unused
    #[test]
    fn _slot_marker() {
        let _: PulseSlot;
    }
}

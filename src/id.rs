//! ID generation for notes, pulses, and metrics.
//!
//! Format: `<kind>-<YYYYMMDD>-<HHMM>-<3 hex chars>`, e.g. `note-20260806-1432-a8f`.
//! Files on disk are named `<id>.md`.

use chrono::Local;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Note,
    Pulse,
    Metric,
}

impl Kind {
    pub fn prefix(self) -> &'static str {
        match self {
            Kind::Note => "note",
            Kind::Pulse => "pulse",
            Kind::Metric => "metric",
        }
    }
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.prefix())
    }
}

/// Generate a new ID with the given kind, timestamped now.
pub fn new_id(kind: Kind) -> String {
    new_id_at(kind, &Local::now())
}

/// Generate a new ID with the given kind at a specific time. Mostly for tests.
pub fn new_id_at(kind: Kind, now: &chrono::DateTime<chrono::Local>) -> String {
    let date = now.format("%Y%m%d");
    let time = now.format("%H%M");
    // Take 12 bits from a UUID and render as 3 lowercase hex chars.
    let uuid = Uuid::new_v4();
    let bytes = uuid.as_bytes();
    let rand3 = (u16::from_be_bytes([bytes[0], bytes[1]]) & 0x0FFF) as u32;
    format!("{}-{}-{}-{:03x}", kind.prefix(), date, time, rand3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_has_expected_shape() {
        let now = Local::now();
        let id = new_id_at(Kind::Note, &now);
        // note-YYYYMMDD-HHMM-xxx (3 lowercase hex)
        let rest = id.strip_prefix("note-").unwrap();
        let parts: Vec<&str> = rest.split('-').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].len(), 8, "date component: {}", parts[0]);
        assert_eq!(parts[1].len(), 4, "time component: {}", parts[1]);
        assert_eq!(parts[2].len(), 3, "rand component: {}", parts[2]);
        assert_eq!(parts[2].to_lowercase(), parts[2]);
    }

    #[test]
    fn kinds_get_distinct_prefixes() {
        assert_eq!(Kind::Note.prefix(), "note");
        assert_eq!(Kind::Pulse.prefix(), "pulse");
        assert_eq!(Kind::Metric.prefix(), "metric");
    }
}

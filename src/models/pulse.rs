//! Pulse model: a recurring boolean tracker (`Timeseries<bool>`).

use chrono::{Datelike, NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};

/// Cadence at which a pulse ticks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Interval {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

impl std::fmt::Display for Interval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Interval::Daily => "daily",
            Interval::Weekly => "weekly",
            Interval::Monthly => "monthly",
            Interval::Yearly => "yearly",
        })
    }
}

impl Interval {
    /// Slot key for a given date, e.g. `"2026-08-06"` for daily.
    /// Weekly keys use ISO week: `"2026-W32"`.
    /// Monthly: `"2026-08"`. Yearly: `"2026"`.
    pub fn slot_key(self, date: NaiveDate) -> String {
        match self {
            Interval::Daily => date.format("%Y-%m-%d").to_string(),
            Interval::Weekly => {
                let iso = date.iso_week();
                format!("{:04}-W{:02}", iso.year(), iso.week())
            }
            Interval::Monthly => date.format("%Y-%m").to_string(),
            Interval::Yearly => date.format("%Y").to_string(),
        }
    }

    /// The slot key that is currently active at the given local date-time.
    pub fn current_slot(self, now: NaiveDateTime) -> String {
        self.slot_key(now.date())
    }
}

/// A single boolean sample in a pulse's history.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PulseSlot {
    /// Slot key, e.g. `"2026-08-06"` for a daily pulse.
    pub slot: String,
    pub checked: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pulse {
    pub id: String,
    pub topic: String,
    pub interval: Interval,
    pub created: NaiveDateTime,
    /// All recorded slots, in no particular order. Absent slot means
    /// "unrecorded", which is semantically false but distinguishable in the
    /// UI from explicitly false.
    pub slots: Vec<PulseSlot>,
}

impl Pulse {
    pub fn new(id: String, topic: String, interval: Interval, created: NaiveDateTime) -> Self {
        Self {
            id,
            topic,
            interval,
            created,
            slots: Vec::new(),
        }
    }

    pub fn set_slot(&mut self, slot: impl Into<String>, checked: bool) {
        let slot = slot.into();
        if let Some(s) = self.slots.iter_mut().find(|s| s.slot == slot) {
            s.checked = checked;
        } else {
            self.slots.push(PulseSlot { slot, checked });
        }
    }

    pub fn get_slot(&self, slot: &str) -> Option<bool> {
        self.slots.iter().find(|s| s.slot == slot).map(|s| s.checked)
    }

    /// Whether the pulse still needs action this interval at the given time.
    /// A pulse is "active" (i.e. needs to be shown) when its current slot has
    /// not been explicitly checked off yet, or has been recorded as unchecked.
    /// Active here means "the current slot is unset or false".
    pub fn is_active_at(&self, now: NaiveDateTime) -> bool {
        let key = self.interval.current_slot(now);
        !matches!(self.get_slot(&key), Some(true))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(s: &str) -> NaiveDateTime {
        s.parse().unwrap()
    }

    #[test]
    fn slot_keys_match_interval() {
        let date = NaiveDate::from_ymd_opt(2026, 8, 6).unwrap();
        assert_eq!(Interval::Daily.slot_key(date), "2026-08-06");
        assert_eq!(Interval::Monthly.slot_key(date), "2026-08");
        assert_eq!(Interval::Yearly.slot_key(date), "2026");
        // 2026-08-06 is Thursday of ISO week 32.
        assert_eq!(Interval::Weekly.slot_key(date), "2026-W32");
    }

    #[test]
    fn set_slot_round_trips() {
        let mut p = Pulse::new(
            "pulse-20260806-1432-a8f".into(),
            "jog 15 min".into(),
            Interval::Daily,
            dt("2026-08-06T14:32:00"),
        );
        p.set_slot("2026-08-06", true);
        assert_eq!(p.get_slot("2026-08-06"), Some(true));
        p.set_slot("2026-08-06", false);
        assert_eq!(p.get_slot("2026-08-06"), Some(false));
        assert_eq!(p.get_slot("2026-08-05"), None);
    }

    #[test]
    fn is_active_when_current_slot_unchecked() {
        let mut p = Pulse::new(
            "pulse-20260806-1432-a8f".into(),
            "jog".into(),
            Interval::Daily,
            dt("2026-08-06T08:00:00"),
        );
        // No slot recorded yet -> active.
        assert!(p.is_active_at(dt("2026-08-06T09:00:00")));
        // Checked for today -> inactive.
        p.set_slot("2026-08-06", true);
        assert!(!p.is_active_at(dt("2026-08-06T23:59:00")));
        // Tomorrow it becomes active again (new slot).
        assert!(p.is_active_at(dt("2026-08-07T00:01:00")));
    }
}

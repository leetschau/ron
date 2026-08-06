//! Metric model: free-form time series of numeric samples (`Timeseries<f64>`).

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MetricPoint {
    pub ts: NaiveDateTime,
    pub value: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Metric {
    pub id: String,
    pub topic: String,
    pub created: NaiveDateTime,
    /// Points in no particular order; the DB layer sorts on read.
    pub points: Vec<MetricPoint>,
}

impl Metric {
    pub fn new(id: String, topic: String, created: NaiveDateTime) -> Self {
        Self {
            id,
            topic,
            created,
            points: Vec::new(),
        }
    }

    pub fn append(&mut self, ts: NaiveDateTime, value: f64) {
        // Replace any existing point at the same timestamp.
        if let Some(p) = self.points.iter_mut().find(|p| p.ts == ts) {
            p.value = value;
        } else {
            self.points.push(MetricPoint { ts, value });
        }
    }

    /// Points sorted ascending by timestamp.
    pub fn sorted_points(&self) -> Vec<&MetricPoint> {
        let mut v: Vec<&MetricPoint> = self.points.iter().collect();
        v.sort_by_key(|p| p.ts);
        v
    }

    /// Basic summary statistics over points in the given time range
    /// (inclusive). Empty input yields None.
    pub fn stats(&self, from: Option<NaiveDateTime>, to: Option<NaiveDateTime>) -> Option<Stats> {
        let values: Vec<f64> = self
            .sorted_points()
            .into_iter()
            .filter(|p| from.map_or(true, |f| p.ts >= f))
            .filter(|p| to.map_or(true, |t| p.ts <= t))
            .map(|p| p.value)
            .collect();
        Stats::from_values(values)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stats {
    pub count: usize,
    pub mean: f64,
    pub median: f64,
    pub min: f64,
    pub max: f64,
}

impl Stats {
    pub fn from_values(mut values: Vec<f64>) -> Option<Self> {
        if values.is_empty() {
            return None;
        }
        let count = values.len();
        let sum: f64 = values.iter().sum();
        let mean = sum / count as f64;
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median = if count % 2 == 1 {
            values[count / 2]
        } else {
            (values[count / 2 - 1] + values[count / 2]) / 2.0
        };
        Some(Stats {
            count,
            mean,
            median,
            min: values[0],
            max: values[count - 1],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(s: &str) -> NaiveDateTime {
        s.parse().unwrap()
    }

    #[test]
    fn append_replaces_same_ts() {
        let mut m = Metric::new("metric-1".into(), "weight".into(), dt("2026-08-06T08:00:00"));
        m.append(dt("2026-08-06T08:00:00"), 72.5);
        m.append(dt("2026-08-06T08:00:00"), 72.8);
        assert_eq!(m.points.len(), 1);
        assert!((m.points[0].value - 72.8).abs() < 1e-9);
    }

    #[test]
    fn stats_are_correct() {
        let mut m = Metric::new("metric-1".into(), "weight".into(), dt("2026-08-01T08:00:00"));
        m.append(dt("2026-08-02T08:00:00"), 70.0);
        m.append(dt("2026-08-03T08:00:00"), 80.0);
        m.append(dt("2026-08-04T08:00:00"), 75.0);
        m.append(dt("2026-08-05T08:00:00"), 76.0);
        let s = m.stats(None, None).unwrap();
        assert_eq!(s.count, 4);
        assert!((s.mean - 75.25).abs() < 1e-9);
        assert!((s.median - 75.5).abs() < 1e-9);
        assert!((s.min - 70.0).abs() < 1e-9);
        assert!((s.max - 80.0).abs() < 1e-9);
    }

    #[test]
    fn stats_empty_is_none() {
        let m = Metric::new("metric-1".into(), "x".into(), dt("2026-08-01T08:00:00"));
        assert!(m.stats(None, None).is_none());
    }

    #[test]
    fn stats_respects_range() {
        let mut m = Metric::new("metric-1".into(), "x".into(), dt("2026-08-01T08:00:00"));
        m.append(dt("2026-08-02T08:00:00"), 70.0);
        m.append(dt("2026-08-03T08:00:00"), 80.0);
        m.append(dt("2026-08-04T08:00:00"), 90.0);
        let s = m.stats(Some(dt("2026-08-03T00:00:00")), None).unwrap();
        assert_eq!(s.count, 2);
        assert!((s.mean - 85.0).abs() < 1e-9);
    }
}

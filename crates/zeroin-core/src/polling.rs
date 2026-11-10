//! Mouse polling-rate estimation from raw-input timestamps.

use serde::{Deserialize, Serialize};

/// Standard USB polling rates.
pub const STANDARD_RATES: [u32; 7] = [125, 250, 500, 1000, 2000, 4000, 8000];

/// Result of analysing a burst of motion reports.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PollingStats {
    /// Measured reports per second while moving.
    pub rate_hz: f64,
    /// Closest standard rate, if within 30 %.
    pub nominal_hz: Option<u32>,
    /// Share of intervals within ±50 % of the mean interval (0–1).
    pub stability: f64,
    /// Number of intervals analysed.
    pub samples: usize,
}

impl PollingStats {
    /// A rate is considered stable when at least 80 % of intervals are close to the mean.
    pub fn is_stable(&self) -> bool {
        self.stability >= 0.8
    }
}

/// Minimum intervals needed for a reading.
pub const MIN_SAMPLES: usize = 200;

/// Analyse timestamps (µs, ascending) of consecutive motion reports from one device.
/// Gaps over 20 ms are treated as pauses and ignored.
pub fn analyze(timestamps_us: &[u64]) -> Option<PollingStats> {
    let intervals: Vec<f64> = timestamps_us.windows(2).map(|w| w[1].saturating_sub(w[0]) as f64).filter(|&d| d < 20_000.0).collect();
    if intervals.len() < MIN_SAMPLES {
        return None;
    }
    let mean = intervals.iter().sum::<f64>() / intervals.len() as f64;
    if mean <= 0.0 {
        return None;
    }
    let rate = 1e6 / mean;
    let close = intervals.iter().filter(|&&d| (d - mean).abs() <= 0.5 * mean).count();
    let nominal = STANDARD_RATES
        .iter()
        .copied()
        .min_by(|a, b| ((*a as f64 - rate).abs()).partial_cmp(&(*b as f64 - rate).abs()).unwrap())
        .filter(|n| (*n as f64 - rate).abs() / (*n as f64) <= 0.3);
    Some(PollingStats { rate_hz: rate, nominal_hz: nominal, stability: close as f64 / intervals.len() as f64, samples: intervals.len() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_1000hz() {
        let ts: Vec<u64> = (0..1000).map(|i| i * 1000 + (i % 3) * 50).collect();
        let s = analyze(&ts).unwrap();
        assert_eq!(s.nominal_hz, Some(1000));
        assert!(s.is_stable());
    }

    #[test]
    fn detects_8000hz_and_ignores_pauses() {
        let mut ts: Vec<u64> = (0..800).map(|i| i * 125).collect();
        let last = *ts.last().unwrap();
        ts.extend((0..800).map(|i| last + 500_000 + i * 125));
        let s = analyze(&ts).unwrap();
        assert_eq!(s.nominal_hz, Some(8000));
    }

    #[test]
    fn unstable_is_flagged_and_short_is_none() {
        let ts: Vec<u64> = (0..400u64)
            .scan(0u64, |t, i| {
                *t += if i % 2 == 0 { 200 } else { 1800 };
                Some(*t)
            })
            .collect();
        assert!(!analyze(&ts).unwrap().is_stable());
        assert!(analyze(&[0, 1000, 2000]).is_none());
    }
}

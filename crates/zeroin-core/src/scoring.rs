//! Turn drill measurements into a comparable score per candidate.
//!
//! The score is a weighted blend of four normalised components:
//!
//! | component | weight | measure |
//! |---|---|---|
//! | accuracy | 0.35 | hit rate over flick + micro trials |
//! | speed    | 0.25 | Fitts' law throughput (bits/s) of hits |
//! | control  | 0.20 | low overshoot relative to distance, few corrections (hits only) |
//! | tracking | 0.20 | time on target and mean error |
//!
//! `bias` is the mean signed first-stop error relative to distance: positive
//! means the player tends to overshoot (sensitivity likely too high), negative
//! means they stop short (likely too low). The search uses it as a tie-breaker.

use crate::drills::{BlockResult, TrialResult};
use serde::{Deserialize, Serialize};

/// Component weights.
pub const WEIGHTS: [f64; 4] = [0.35, 0.25, 0.20, 0.20];

/// Score for one candidate (components in 0–1, total in 0–100).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Score {
    /// Weighted total, 0–100.
    pub total: f64,
    /// Hit rate.
    pub accuracy: f64,
    /// Normalised throughput.
    pub speed: f64,
    /// Overshoot / correction control.
    pub control: f64,
    /// Tracking quality.
    pub tracking: f64,
    /// Signed overshoot tendency in `[-1, 1]`.
    pub bias: f64,
    /// Median time to target over hits, ms.
    pub median_ttt_ms: f64,
}

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        0.5 * (v[n / 2 - 1] + v[n / 2])
    }
}

/// Fitts' law index of difficulty (Shannon form), bits.
pub fn index_of_difficulty(distance_deg: f64, radius_deg: f64) -> f64 {
    (distance_deg / (2.0 * radius_deg).max(1e-6) + 1.0).log2()
}

/// Throughput reference: 8 bits/s maps to a speed score of 1.
pub const THROUGHPUT_REF: f64 = 8.0;

/// Score one block.
pub fn score_block(r: &BlockResult) -> Score {
    let trials: Vec<&TrialResult> = r.flicks.iter().chain(r.micros.iter()).collect();
    let n = trials.len().max(1) as f64;
    let accuracy = trials.iter().filter(|t| t.hit).count() as f64 / n;

    let tps: Vec<f64> = trials
        .iter()
        .filter(|t| t.hit && t.click_ms > 1.0)
        .map(|t| index_of_difficulty(t.distance_deg, t.radius_deg) / (t.click_ms / 1000.0))
        .collect();
    let speed = (median(tps) / THROUGHPUT_REF).clamp(0.0, 1.0);

    // Control is judged on hits only, so idle or random input cannot score well here.
    let hits: Vec<&&TrialResult> = trials.iter().filter(|t| t.hit).collect();
    let control = if hits.is_empty() {
        0.0
    } else {
        let h = hits.len() as f64;
        let mean_os = hits.iter().map(|t| t.overshoot_deg / t.distance_deg.max(1e-6)).sum::<f64>() / h;
        let mean_corr = hits.iter().map(|t| t.corrections as f64).sum::<f64>() / h;
        (1.0 - 2.0 * mean_os - 0.15 * mean_corr).clamp(0.0, 1.0)
    };

    let tr = &r.tracking;
    let tracking = if tr.radius_deg > 0.0 {
        (0.7 * tr.on_target + 0.3 * (1.0 - tr.mean_error_deg / (4.0 * tr.radius_deg)).clamp(0.0, 1.0)).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let bias = if trials.is_empty() {
        0.0
    } else {
        (trials.iter().map(|t| t.first_stop_error_deg / t.distance_deg.max(1e-6)).sum::<f64>() / n).clamp(-1.0, 1.0)
    };

    let total = 100.0 * (WEIGHTS[0] * accuracy + WEIGHTS[1] * speed + WEIGHTS[2] * control + WEIGHTS[3] * tracking);
    Score {
        total,
        accuracy,
        speed,
        control,
        tracking,
        bias,
        median_ttt_ms: median(trials.iter().filter_map(|t| t.time_to_target_ms).collect()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drills::TrackingResult;

    fn trial(hit: bool, click_ms: f64, overshoot: f64, stop: f64, corr: u32) -> TrialResult {
        TrialResult {
            distance_deg: 30.0,
            radius_deg: 1.6,
            hit,
            time_to_target_ms: Some(click_ms * 0.8),
            click_ms,
            overshoot_deg: overshoot,
            first_stop_error_deg: stop,
            corrections: corr,
        }
    }

    #[test]
    fn better_performance_scores_higher() {
        let good = BlockResult {
            flicks: vec![trial(true, 400.0, 0.2, 0.1, 0); 8],
            micros: vec![],
            tracking: TrackingResult { on_target: 0.8, mean_error_deg: 1.0, radius_deg: 2.0 },
        };
        let bad = BlockResult {
            flicks: vec![trial(false, 900.0, 6.0, 5.0, 3); 8],
            micros: vec![],
            tracking: TrackingResult { on_target: 0.2, mean_error_deg: 6.0, radius_deg: 2.0 },
        };
        let (g, b) = (score_block(&good), score_block(&bad));
        assert!(g.total > b.total + 30.0, "{g:?} {b:?}");
        assert!(b.bias > 0.1);
        assert!((0.0..=100.0).contains(&g.total));
    }

    #[test]
    fn empty_block_is_zero_not_nan() {
        let s = score_block(&BlockResult::default());
        assert!(s.total.is_finite());
        assert_eq!(s.accuracy, 0.0);
    }

    #[test]
    fn fitts_id() {
        assert!((index_of_difficulty(3.0, 0.5) - 2.0).abs() < 1e-12);
    }
}

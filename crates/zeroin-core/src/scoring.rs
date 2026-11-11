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
//! Since 0.2 the search objective is [`Score::v2`] (spec 10 §5): a Fitts
//! effective-throughput pointing term and a tracking term measured over raw
//! input time. `total` and its components are still computed for display and
//! for comparing with 0.1 history.
//!
//! `bias` is the mean signed first-stop error relative to distance. People
//! stop short of targets by about 11 % on average whatever their sensitivity,
//! so [`Score::bias_corrected`] subtracts that baseline before any advice is
//! given. Neither value moves the search.

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
    /// Block score v2 (0–100): `100 · (0.6 · pointing + 0.4 · tracking_v2)`. The search objective.
    #[serde(default)]
    pub v2: f64,
    /// Pointing term of v2 (0–1).
    #[serde(default)]
    pub pointing: f64,
    /// Effective throughput in bits/s.
    #[serde(default)]
    pub throughput: f64,
    /// Tracking term of v2 (0–1), from time on target and RMS error.
    #[serde(default)]
    pub tracking_v2: f64,
    /// `bias` minus the population baseline of −0.11 (0 = typical).
    #[serde(default)]
    pub bias_corrected: f64,
}

/// Typical primary-submovement endpoint error relative to distance: people
/// undershoot by about 11 % regardless of sensitivity.
pub const UNDERSHOOT_BASELINE: f64 = -0.11;
/// Fixed movement time charged for a miss, in seconds.
pub const MISS_TIME_S: f64 = 1.5;

/// Effective throughput (bits/s) and the pointing term of score v2.
pub fn pointing_v2(trials: &[&TrialResult]) -> (f64, f64) {
    if trials.is_empty() {
        return (0.0, 0.0);
    }
    let n = trials.len() as f64;
    let rel: Vec<f64> = trials.iter().map(|t| t.first_stop_error_deg / t.distance_deg.max(1e-6)).collect();
    let mean = rel.iter().sum::<f64>() / n;
    let sd = if trials.len() > 1 { (rel.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt() } else { 0.0 };
    let tpe = trials
        .iter()
        .map(|t| {
            let a = t.distance_deg.max(1e-6);
            let we = (4.133 * sd * a).max(0.5 * 2.0 * t.radius_deg).max(1e-6);
            let ide = (a / we + 1.0).log2();
            let mte = if t.hit { (t.click_ms / 1000.0).max(0.05) } else { MISS_TIME_S };
            ide / mte
        })
        .sum::<f64>()
        / n;
    let hit_rate = trials.iter().filter(|t| t.hit).count() as f64 / n;
    (tpe, (tpe / THROUGHPUT_REF).min(1.0) * (0.5 + 0.5 * hit_rate))
}

/// Tracking term of score v2.
pub fn tracking_v2(tr: &crate::drills::TrackingResult) -> f64 {
    if tr.radius_deg <= 0.0 {
        return 0.0;
    }
    // History from 0.1 has no RMS; fall back to the mean error.
    let rms = if tr.rms_error_deg > 0.0 { tr.rms_error_deg } else { tr.mean_error_deg };
    (0.7 * tr.on_target + 0.3 * (1.0 - rms / (4.0 * tr.radius_deg)).clamp(0.0, 1.0)).clamp(0.0, 1.0)
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
    let (throughput, pointing) = pointing_v2(&trials);
    let t2 = tracking_v2(tr);
    Score {
        v2: 100.0 * (0.6 * pointing + 0.4 * t2),
        pointing,
        throughput,
        tracking_v2: t2,
        bias_corrected: if trials.is_empty() { 0.0 } else { bias - UNDERSHOOT_BASELINE },
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
            anticipated: false,
            timed_out: false,
        }
    }

    #[test]
    fn better_performance_scores_higher() {
        let good = BlockResult {
            flicks: vec![trial(true, 400.0, 0.2, 0.1, 0); 8],
            micros: vec![],
            tracking: TrackingResult { on_target: 0.8, mean_error_deg: 1.0, radius_deg: 2.0, rms_error_deg: 1.2 },
            input_gaps: 0,
        };
        let bad = BlockResult {
            flicks: vec![trial(false, 900.0, 6.0, 5.0, 3); 8],
            micros: vec![],
            tracking: TrackingResult { on_target: 0.2, mean_error_deg: 6.0, radius_deg: 2.0, rms_error_deg: 7.0 },
            input_gaps: 0,
        };
        let (g, b) = (score_block(&good), score_block(&bad));
        assert!(g.total > b.total + 30.0, "{g:?} {b:?}");
        assert!(b.bias > 0.1);
        assert!((0.0..=100.0).contains(&g.total));
        assert!(g.v2 > b.v2 + 30.0, "{g:?} {b:?}");
        assert!((0.0..=100.0).contains(&g.v2));
    }

    #[test]
    fn undershoot_baseline_is_removed() {
        let r = BlockResult { flicks: vec![trial(true, 400.0, 0.0, -3.3, 0); 6], ..Default::default() };
        let s = score_block(&r);
        assert!((s.bias + 0.11).abs() < 1e-9);
        assert!(s.bias_corrected.abs() < 1e-9);
    }

    #[test]
    fn v2_rewards_speed_at_equal_accuracy() {
        let fast = BlockResult { flicks: vec![trial(true, 350.0, 0.0, -1.0, 0); 6], ..Default::default() };
        let slow = BlockResult { flicks: vec![trial(true, 700.0, 0.0, -1.0, 0); 6], ..Default::default() };
        assert!(score_block(&fast).v2 > score_block(&slow).v2);
        // A miss costs a fixed 1.5 s.
        let miss = BlockResult { flicks: vec![trial(false, 300.0, 0.0, -1.0, 0); 6], ..Default::default() };
        assert!(score_block(&miss).v2 < score_block(&slow).v2);
    }

    #[test]
    fn empty_block_is_zero_not_nan() {
        let s = score_block(&BlockResult::default());
        assert!(s.total.is_finite() && s.v2.is_finite());
        assert_eq!(s.accuracy, 0.0);
    }

    #[test]
    fn fitts_id() {
        assert!((index_of_difficulty(3.0, 0.5) - 2.0).abs() < 1e-12);
    }
}

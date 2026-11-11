//! The 0.1 iterative-narrowing sensitivity search.
//!
//! Since 0.2 the app uses [`crate::bayes`]. This module is kept because its
//! [`Observation`] type is part of the history format and so 0.1 results can
//! still be read and compared.
//!
//! The search works on a multiplier `m` of the player's current sensitivity,
//! in log space (so 0.8× and 1.25× are equally far from 1×). Each round tests
//! three candidates: `low`, `mid` and `high`. After scoring:
//!
//! 1. The best-scoring candidate becomes the new centre.
//! 2. The centre is nudged against the player's overshoot bias (overshooting
//!    suggests a lower sensitivity, stopping short a higher one).
//! 3. The span halves when the best was the middle candidate, and shrinks
//!    only to 80 % when it was an edge, so the search can walk outward.
//!
//! After the last round a weighted quadratic is fitted to the most recent
//! observations; its vertex is used when the fit is concave and the vertex
//! lies inside the tested range, otherwise the best observation wins.
//!
//! This algorithm is the source of truth. The optional local model may only
//! propose a replacement for the middle candidate via [`Search::propose_mid`],
//! which clamps it to the inner half of the current range.

use crate::scoring::Score;
use serde::{Deserialize, Serialize};

/// Tunables.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SearchConfig {
    /// Number of rounds (3 candidates each).
    pub rounds: u32,
    /// Initial half-span in log space (`ln(1.6)` ≈ ±60 %).
    pub initial_half_span: f64,
    /// Smallest multiplier ever tested.
    pub min_mult: f64,
    /// Largest multiplier ever tested.
    pub max_mult: f64,
    /// Strength of the overshoot-bias nudge, as a fraction of the half-span.
    pub bias_gain: f64,
}

impl Default for SearchConfig {
    fn default() -> Self {
        SearchConfig { rounds: 4, initial_half_span: 1.6f64.ln(), min_mult: 0.35, max_mult: 2.8, bias_gain: 0.3 }
    }
}

/// One scored candidate.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    /// Round index (0-based).
    pub round: u32,
    /// Multiplier of the starting sensitivity.
    pub mult: f64,
    /// Its score.
    pub score: Score,
}

/// How the final recommendation was derived.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecommendationMethod {
    /// Vertex of a concave quadratic fit.
    QuadraticFit,
    /// Best single observation.
    BestObserved,
    /// The 0.2 grid-posterior search ([`crate::bayes`]).
    Bayes,
}

/// Final result of the search.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Recommendation {
    /// Multiplier of the starting sensitivity.
    pub mult: f64,
    /// Confidence in `[0, 1]`.
    pub confidence: f64,
    /// Derivation.
    pub method: RecommendationMethod,
}

/// Search state machine.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Search {
    cfg: SearchConfig,
    center: f64,
    half: f64,
    round: u32,
    observations: Vec<Observation>,
    advised_mid: Option<f64>,
}

impl Search {
    /// Start a search centred on the current sensitivity (multiplier 1).
    pub fn new(cfg: SearchConfig) -> Self {
        Search { cfg, center: 0.0, half: cfg.initial_half_span, round: 0, observations: Vec::new(), advised_mid: None }
    }

    /// Configuration.
    pub fn config(&self) -> &SearchConfig {
        &self.cfg
    }

    /// Current round (0-based).
    pub fn round(&self) -> u32 {
        self.round
    }

    /// All observations so far.
    pub fn observations(&self) -> &[Observation] {
        &self.observations
    }

    /// True when every round has been recorded.
    pub fn is_done(&self) -> bool {
        self.round >= self.cfg.rounds
    }

    fn clamp_mult(&self, m: f64) -> f64 {
        m.clamp(self.cfg.min_mult, self.cfg.max_mult)
    }

    /// Lowest and highest multiplier of the current round.
    pub fn bounds(&self) -> (f64, f64) {
        (self.clamp_mult((self.center - self.half).exp()), self.clamp_mult((self.center + self.half).exp()))
    }

    /// The range a model proposal is clamped to: the inner half of the round.
    pub fn advice_bounds(&self) -> (f64, f64) {
        (self.clamp_mult((self.center - self.half / 2.0).exp()), self.clamp_mult((self.center + self.half / 2.0).exp()))
    }

    /// Candidates for this round, ordered low, mid, high.
    pub fn candidates(&self) -> [f64; 3] {
        let (lo, hi) = self.bounds();
        let mid = self.advised_mid.unwrap_or_else(|| self.clamp_mult(self.center.exp()));
        [lo, mid, hi]
    }

    /// Offer a replacement for the middle candidate (from the local model).
    /// Non-finite input is ignored; anything else is clamped. Returns the value used.
    pub fn propose_mid(&mut self, mult: f64) -> Option<f64> {
        if !mult.is_finite() || mult <= 0.0 || self.is_done() {
            return None;
        }
        let (lo, hi) = self.advice_bounds();
        let m = mult.clamp(lo, hi);
        self.advised_mid = Some(m);
        Some(m)
    }

    /// Record scores for the three candidates returned by [`Self::candidates`].
    pub fn record_round(&mut self, scores: [Score; 3]) {
        if self.is_done() {
            return;
        }
        let cands = self.candidates();
        for (m, s) in cands.iter().zip(scores.iter()) {
            self.observations.push(Observation { round: self.round, mult: *m, score: *s });
        }
        // Best by total; ties go to the candidate nearest the centre (index 1).
        let mut best = 1usize;
        for i in [0usize, 2] {
            if scores[i].total > scores[best].total + 1e-9 {
                best = i;
            }
        }
        let bias = scores[best].bias.clamp(-1.0, 1.0);
        let nudge = -self.cfg.bias_gain * self.half * bias;
        let new_center = cands[best].ln() + nudge;
        let lo = self.cfg.min_mult.ln();
        let hi = self.cfg.max_mult.ln();
        self.center = new_center.clamp(lo, hi);
        self.half *= if best == 1 { 0.5 } else { 0.8 };
        self.advised_mid = None;
        self.round += 1;
    }

    /// Final recommendation (available once at least one round is recorded).
    pub fn recommendation(&self) -> Option<Recommendation> {
        let best =
            self.observations.iter().max_by(|a, b| a.score.total.partial_cmp(&b.score.total).unwrap_or(std::cmp::Ordering::Equal))?;
        let last = self.observations.iter().map(|o| o.round).max().unwrap_or(0);
        let recent: Vec<&Observation> = self.observations.iter().filter(|o| o.round + 1 >= last).collect();

        let confidence = {
            let others: Vec<f64> = recent.iter().filter(|o| o.mult != best.mult).map(|o| o.score.total).collect();
            let gap = if others.is_empty() { 0.0 } else { best.score.total - others.iter().sum::<f64>() / others.len() as f64 };
            let progress = (self.round as f64 / self.cfg.rounds.max(1) as f64).min(1.0);
            ((gap / 12.0).clamp(0.0, 1.0) * 0.6 + 0.4 * progress).clamp(0.0, 1.0)
        };

        if let Some(v) = quadratic_vertex(&recent) {
            let xs = recent.iter().map(|o| o.mult.ln());
            let (mn, mx) = xs.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), x| (a.min(x), b.max(x)));
            if v >= mn && v <= mx {
                return Some(Recommendation { mult: self.clamp_mult(v.exp()), confidence, method: RecommendationMethod::QuadraticFit });
            }
        }
        Some(Recommendation { mult: best.mult, confidence, method: RecommendationMethod::BestObserved })
    }
}

/// Least-squares fit `score = a x² + b x + c` over `x = ln(mult)`. Returns the
/// vertex when the parabola is concave and there are at least 3 distinct x.
fn quadratic_vertex(obs: &[&Observation]) -> Option<f64> {
    let mut xs: Vec<f64> = obs.iter().map(|o| o.mult.ln()).collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    if xs.len() < 3 {
        return None;
    }
    // Normal equations for [x², x, 1].
    let mut m = [[0.0f64; 4]; 3];
    for o in obs {
        let x = o.mult.ln();
        let y = o.score.total;
        let row = [x * x, x, 1.0];
        for i in 0..3 {
            for j in 0..3 {
                m[i][j] += row[i] * row[j];
            }
            m[i][3] += row[i] * y;
        }
    }
    // Gaussian elimination with partial pivoting.
    for col in 0..3 {
        let piv = (col..3).max_by(|&a, &b| m[a][col].abs().partial_cmp(&m[b][col].abs()).unwrap())?;
        if m[piv][col].abs() < 1e-12 {
            return None;
        }
        m.swap(col, piv);
        for r in 0..3 {
            if r != col {
                let f = m[r][col] / m[col][col];
                let pivot_row = m[col];
                for (dst, src) in m[r].iter_mut().zip(pivot_row.iter()).skip(col) {
                    *dst -= f * src;
                }
            }
        }
    }
    let a = m[0][3] / m[0][0];
    let b = m[1][3] / m[1][1];
    if !(a.is_finite() && b.is_finite()) || a >= -1e-9 {
        return None;
    }
    Some(-b / (2.0 * a))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic(best: f64, m: f64) -> Score {
        let d = (m / best).ln();
        let total = (85.0 - 140.0 * d * d).clamp(0.0, 100.0);
        // Overshoot when too fast, undershoot when too slow.
        let bias = (d * 0.8).clamp(-1.0, 1.0);
        Score { total, bias, ..Default::default() }
    }

    fn run(best: f64) -> f64 {
        let mut s = Search::new(SearchConfig::default());
        while !s.is_done() {
            let c = s.candidates();
            s.record_round([synthetic(best, c[0]), synthetic(best, c[1]), synthetic(best, c[2])]);
        }
        s.recommendation().unwrap().mult
    }

    #[test]
    fn converges_near_optimum() {
        for best in [0.55, 0.8, 1.0, 1.3, 1.9] {
            let got = run(best);
            assert!((got / best - 1.0).abs() < 0.08, "best {best}, got {got}");
        }
    }

    #[test]
    fn deterministic() {
        assert_eq!(run(1.23), run(1.23));
    }

    #[test]
    fn advice_is_clamped_and_validated() {
        let mut s = Search::new(SearchConfig::default());
        let (lo, hi) = s.advice_bounds();
        assert_eq!(s.propose_mid(100.0), Some(hi));
        assert_eq!(s.propose_mid(0.0001), Some(lo));
        assert_eq!(s.propose_mid(f64::NAN), None);
        assert_eq!(s.propose_mid(-1.0), None);
        let c = s.candidates();
        assert!(c[0] < c[1] && c[1] <= c[2]);
    }

    #[test]
    fn stays_inside_global_limits() {
        let cfg = SearchConfig::default();
        let mut s = Search::new(cfg);
        while !s.is_done() {
            let c = s.candidates();
            for m in c {
                assert!(m >= cfg.min_mult - 1e-12 && m <= cfg.max_mult + 1e-12);
            }
            // Always prefer the highest candidate.
            s.record_round([
                Score { total: 10.0, ..Default::default() },
                Score { total: 20.0, ..Default::default() },
                Score { total: 30.0, ..Default::default() },
            ]);
        }
        assert!(s.recommendation().unwrap().mult <= cfg.max_mult);
    }
}

//! The 0.2 sensitivity search: three bracket blocks, then adaptive blocks
//! chosen from a grid posterior, and a keep-unless-it-clearly-helps decision.
//!
//! Each block measures score v2 at one sensitivity. Positions are `x`, the
//! natural-log change in degrees per count from the player's current
//! setting (x = 0 is the current sens, ln 1.6 is 60 % faster).
//!
//! Model (validated in simulation, research notes 09 and 10):
//!
//! ```text
//! v2_block = h − k·(x − θ)² + β·i + noise(σ = 3.5)
//! ```
//!
//! θ is the player's best setting, k the curvature (points per log²), β a
//! learning / fatigue drift per block and h the height, which is profiled out
//! by centring residuals. The posterior over (θ, k, β) is computed on a grid.
//! The prior on θ is N(0, 0.6²) around the current sens times the per-game
//! population prior from [`crate::priors`].
//!
//! Decision: the expected gain of moving is E[k]·E[θ]². If it is below the
//! mode's threshold (Conservative 2, Balanced 1, Explorer 0.5 points) the
//! advice is to keep the current sens. Otherwise the recommendation is
//! exp(E[θ]) × current, limited to ×0.5–×2 per session.
//!
//! The result is shown as a range of about-equally-good settings (predicted
//! loss under 2 points), not as a confidence percentage: the interval
//! coverage was not calibrated in simulation, so no probability is claimed.

use crate::scoring::Score;
use crate::search::Observation;
use serde::{Deserialize, Serialize};

/// Lowest multiplier the search will test.
pub const MIN_MULT: f64 = 0.35;
/// Highest multiplier the search will test.
pub const MAX_MULT: f64 = 2.8;
/// Largest change recommended in one session (and its inverse).
pub const MAX_SESSION_CHANGE: f64 = 2.0;
/// Fewer valid blocks than this gives no result.
pub const MIN_VALID_BLOCKS: usize = 6;
/// Width of the "about equally good" band, in score points.
pub const PLATEAU_PTS: f64 = 2.0;

const N_THETA: usize = 141;
const KS: [f64; 7] = [1.0, 2.5, 5.0, 10.0, 20.0, 40.0, 80.0];
const BS: [f64; 5] = [-1.5, -0.75, 0.0, 0.75, 1.5];

/// How readily the search recommends a change.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Change only for a predicted gain of 2+ points.
    Conservative,
    /// Change for a predicted gain of 1+ point (default).
    #[default]
    Balanced,
    /// Change for a predicted gain of 0.5+ point.
    Explorer,
}

impl Mode {
    /// Minimum predicted gain, in points, for recommending a change.
    pub fn threshold(self) -> f64 {
        match self {
            Mode::Conservative => 2.0,
            Mode::Balanced => 1.0,
            Mode::Explorer => 0.5,
        }
    }
    /// Display label.
    pub fn label(self) -> &'static str {
        match self {
            Mode::Conservative => "Conservative",
            Mode::Balanced => "Balanced",
            Mode::Explorer => "Explorer",
        }
    }
}

/// Search settings. Serialised with a resumable session.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BayesConfig {
    /// Planned valid blocks (6 quick, 9 standard).
    pub blocks: u32,
    /// Decision mode.
    pub mode: Mode,
    /// Population prior centre on θ (see [`crate::priors::GamePrior::theta_centre`]).
    pub pop_centre: f64,
    /// Population prior sd (log units).
    pub pop_sd: f64,
    /// Prior sd around the current sens.
    pub current_sd: f64,
    /// Bracket step (log units); the first three blocks are 0, +step, −step.
    pub bracket: f64,
    /// Largest step between consecutive adaptive blocks (log units).
    pub cap: f64,
    /// Assumed block-score noise, points.
    pub sigma: f64,
}

impl BayesConfig {
    /// Recommended settings for `blocks` blocks with a population prior.
    pub fn new(blocks: u32, mode: Mode, pop_centre: f64, pop_sd: f64) -> Self {
        BayesConfig {
            blocks: blocks.clamp(MIN_VALID_BLOCKS as u32, 12),
            mode,
            pop_centre: if pop_centre.is_finite() { pop_centre } else { 0.0 },
            pop_sd: if pop_sd.is_finite() && pop_sd > 0.05 { pop_sd } else { 0.43 },
            current_sd: 0.6,
            bracket: 1.6f64.ln(),
            cap: 1.25f64.ln(),
            sigma: 3.5,
        }
    }
}

/// Posterior summary.
#[derive(Clone, Debug, PartialEq)]
pub struct Posterior {
    /// Posterior mean of θ.
    pub mean: f64,
    /// Posterior sd of θ.
    pub sd: f64,
    /// Posterior mean of k.
    pub k_mean: f64,
    /// Minimiser of the expected loss, E[kθ]/E[k].
    pub loss_centre: f64,
}

/// The outcome of a finished search.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    /// True when the advice is to keep the current sens.
    pub keep: bool,
    /// Recommended multiplier of degrees per count (1.0 when keeping).
    pub mult: f64,
    /// Predicted gain of moving to the posterior mean, in score points.
    pub gain_pts: f64,
    /// Range of about-equally-good multipliers (low, high).
    pub band: (f64, f64),
    /// Valid blocks used.
    pub blocks: u32,
}

/// The search state: configuration plus valid observations.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BayesSearch {
    cfg: BayesConfig,
    obs: Vec<Observation>,
}

fn theta_grid(i: usize) -> f64 {
    let (lo, hi) = (MIN_MULT.ln(), MAX_MULT.ln());
    lo + (hi - lo) * i as f64 / (N_THETA - 1) as f64
}

impl BayesSearch {
    /// New search.
    pub fn new(cfg: BayesConfig) -> Self {
        BayesSearch { cfg, obs: Vec::new() }
    }

    /// Configuration.
    pub fn config(&self) -> &BayesConfig {
        &self.cfg
    }

    /// Valid observations so far (`mult` = multiplier tested, `round` = block index).
    pub fn observations(&self) -> &[Observation] {
        &self.obs
    }

    /// Valid blocks recorded.
    pub fn blocks_done(&self) -> usize {
        self.obs.len()
    }

    /// True once the planned number of blocks is recorded.
    pub fn is_done(&self) -> bool {
        self.obs.len() >= self.cfg.blocks as usize
    }

    /// Log-multiplier for the next block, or `None` when done.
    pub fn next_x(&self) -> Option<f64> {
        let i = self.obs.len();
        if self.is_done() {
            return None;
        }
        let (lo, hi) = (MIN_MULT.ln(), MAX_MULT.ln());
        let x = match i {
            0 => 0.0,
            1 => self.cfg.bracket,
            2 => -self.cfg.bracket,
            _ => {
                let p = self.posterior();
                let side = if i % 2 == 1 { 1.0 } else { -1.0 };
                let x = p.mean + (0.8 * p.sd + 0.08) * side;
                let prev = self.obs[i - 1].mult.ln();
                x.clamp(prev - self.cfg.cap, prev + self.cfg.cap)
            }
        };
        Some(x.clamp(lo, hi))
    }

    /// Multiplier for the next block.
    pub fn next_mult(&self) -> Option<f64> {
        self.next_x().map(f64::exp)
    }

    /// Record a valid block measured at multiplier `mult` (after rounding to
    /// what the game can represent).
    pub fn record(&mut self, mult: f64, score: Score) {
        if !(mult.is_finite() && mult > 0.0 && score.v2.is_finite()) || self.is_done() {
            return;
        }
        self.obs.push(Observation { round: self.obs.len() as u32, mult, score });
    }

    /// Unnormalised log posterior on the (θ, β, k) grid.
    fn log_post(&self) -> Vec<f64> {
        let xs: Vec<f64> = self.obs.iter().map(|o| o.mult.ln()).collect();
        let ys: Vec<f64> = self.obs.iter().map(|o| o.score.v2).collect();
        let n = xs.len();
        let s2 = self.cfg.sigma * self.cfg.sigma;
        let mut out = Vec::with_capacity(N_THETA * BS.len() * KS.len());
        let mut resid = vec![0.0; n];
        for ti in 0..N_THETA {
            let th = theta_grid(ti);
            let prior = -0.5 * (th / self.cfg.current_sd).powi(2) - 0.5 * ((th - self.cfg.pop_centre) / self.cfg.pop_sd).powi(2);
            for b in BS {
                for k in KS {
                    let mut mean = 0.0;
                    for j in 0..n {
                        let pred = -k * (xs[j] - th).powi(2) + b * j as f64;
                        resid[j] = ys[j] - pred;
                        mean += resid[j];
                    }
                    let mut ll = 0.0;
                    if n > 0 {
                        mean /= n as f64;
                        for r in &resid {
                            ll += (r - mean).powi(2);
                        }
                    }
                    out.push(prior - 0.5 * ll / s2);
                }
            }
        }
        out
    }

    /// Posterior summary (the prior alone when nothing is recorded).
    pub fn posterior(&self) -> Posterior {
        let lp = self.log_post();
        let max = lp.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let (mut z, mut m1, mut m2, mut ek, mut ekt) = (0.0, 0.0, 0.0, 0.0, 0.0);
        let per_theta = BS.len() * KS.len();
        for (idx, l) in lp.iter().enumerate() {
            let w = (l - max).exp();
            let th = theta_grid(idx / per_theta);
            let k = KS[idx % KS.len()];
            z += w;
            m1 += w * th;
            m2 += w * th * th;
            ek += w * k;
            ekt += w * k * th;
        }
        let mean = m1 / z;
        let sd = (m2 / z - mean * mean).max(0.0).sqrt();
        let k_mean = ek / z;
        Posterior { mean, sd, k_mean, loss_centre: (ekt / z) / k_mean }
    }

    /// The decision, once at least [`MIN_VALID_BLOCKS`] valid blocks exist.
    pub fn decision(&self) -> Option<Decision> {
        if self.obs.len() < MIN_VALID_BLOCKS {
            return None;
        }
        let p = self.posterior();
        let gain = p.k_mean * p.mean * p.mean;
        let keep = gain < self.cfg.mode.threshold();
        let lim = MAX_SESSION_CHANGE.ln();
        let x = if keep { 0.0 } else { p.mean.clamp(-lim, lim) };
        // Expected loss is E[k]·(x − x*)² + const, so the 2-point band is
        // x* ± sqrt(2 / E[k]). Keep it inside the tested range.
        let half = (PLATEAU_PTS / p.k_mean).sqrt();
        let tested_lo = self.obs.iter().map(|o| o.mult.ln()).fold(f64::INFINITY, f64::min);
        let tested_hi = self.obs.iter().map(|o| o.mult.ln()).fold(f64::NEG_INFINITY, f64::max);
        let mut lo = (p.loss_centre - half).max(tested_lo).max(-lim);
        let mut hi = (p.loss_centre + half).min(tested_hi).min(lim);
        lo = lo.min(x);
        hi = hi.max(x);
        Some(Decision { keep, mult: x.exp(), gain_pts: gain, band: (lo.exp(), hi.exp()), blocks: self.obs.len() as u32 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Rng;

    fn score(v2: f64) -> Score {
        Score { v2, ..Default::default() }
    }

    fn gauss(r: &mut Rng) -> f64 {
        let (u1, u2) = (r.next_f64().max(1e-12), r.next_f64());
        (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }

    fn run(theta: f64, k: f64, noise: f64, seed: u64, mode: Mode) -> (BayesSearch, Decision) {
        let mut s = BayesSearch::new(BayesConfig::new(9, mode, 0.0, 0.4));
        let mut r = Rng::new(seed);
        while let Some(x) = s.next_x() {
            let y = 70.0 - k * (x - theta).powi(2) + noise * gauss(&mut r);
            s.record(x.exp(), score(y));
        }
        let d = s.decision().unwrap();
        (s, d)
    }

    #[test]
    fn bracket_then_adaptive_and_capped() {
        let (s, _) = run(0.3, 10.0, 0.0, 1, Mode::Balanced);
        let xs: Vec<f64> = s.observations().iter().map(|o| o.mult.ln()).collect();
        assert_eq!(xs.len(), 9);
        assert!((xs[0]).abs() < 1e-12 && (xs[1] - 1.6f64.ln()).abs() < 1e-12 && (xs[2] + 1.6f64.ln()).abs() < 1e-12);
        for w in xs[2..].windows(2).skip(1) {
            assert!((w[1] - w[0]).abs() <= 1.25f64.ln() + 1e-9, "{xs:?}");
        }
    }

    #[test]
    fn finds_a_clear_optimum() {
        let (_, d) = run(0.35, 15.0, 0.5, 2, Mode::Balanced);
        assert!(!d.keep);
        assert!((d.mult.ln() - 0.35).abs() < 0.12, "{d:?}");
        assert!(d.band.0 <= d.mult && d.mult <= d.band.1);
    }

    #[test]
    fn keeps_current_when_flat() {
        let (_, d) = run(0.05, 3.0, 2.0, 3, Mode::Balanced);
        assert!(d.keep, "{d:?}");
        assert_eq!(d.mult, 1.0);
        assert!(d.band.0 <= 1.0 && 1.0 <= d.band.1);
    }

    #[test]
    fn modes_order_thresholds() {
        assert!(Mode::Conservative.threshold() > Mode::Balanced.threshold());
        assert!(Mode::Balanced.threshold() > Mode::Explorer.threshold());
    }

    #[test]
    fn session_change_is_capped() {
        let (_, d) = run(1.2, 20.0, 0.3, 4, Mode::Explorer);
        assert!(d.mult <= MAX_SESSION_CHANGE + 1e-9, "{d:?}");
    }

    #[test]
    fn needs_six_blocks_and_ignores_garbage() {
        let mut s = BayesSearch::new(BayesConfig::new(6, Mode::Balanced, 0.0, 0.4));
        for _ in 0..5 {
            let m = s.next_mult().unwrap();
            s.record(m, score(60.0));
        }
        assert!(s.decision().is_none());
        s.record(f64::NAN, score(60.0));
        s.record(1.0, score(f64::INFINITY));
        assert_eq!(s.blocks_done(), 5);
        let m = s.next_mult().unwrap();
        s.record(m, score(60.0));
        assert!(s.is_done() && s.next_x().is_none() && s.decision().is_some());
    }

    #[test]
    fn deterministic_and_round_trips() {
        let (a, da) = run(-0.2, 8.0, 2.0, 9, Mode::Balanced);
        let (b, db) = run(-0.2, 8.0, 2.0, 9, Mode::Balanced);
        assert_eq!(da, db);
        let json = serde_json::to_string(&a).unwrap();
        let back: BayesSearch = serde_json::from_str(&json).unwrap();
        assert_eq!(back.blocks_done(), b.blocks_done());
        // JSON may round the last bit of a float; the decision must not move.
        let dr = back.decision().unwrap();
        assert_eq!(dr.keep, da.keep);
        assert!((dr.mult - da.mult).abs() < 1e-9 && (dr.gain_pts - da.gain_pts).abs() < 1e-6);
    }

    #[test]
    fn population_prior_pulls_the_mean() {
        let slow = BayesSearch::new(BayesConfig::new(9, Mode::Balanced, -0.5, 0.3)).posterior();
        let fast = BayesSearch::new(BayesConfig::new(9, Mode::Balanced, 0.5, 0.3)).posterior();
        assert!(slow.mean < -0.1 && fast.mean > 0.1);
    }

    /// Same numbers as the research simulator (stress.py) for a fixed input.
    #[test]
    fn matches_the_research_simulator() {
        let mut s = BayesSearch::new(BayesConfig::new(9, Mode::Balanced, 0.1, 0.3));
        let xs = [0.0, 1.6f64.ln(), -(1.6f64.ln()), 0.2, -0.1, 0.3];
        let ys = [60.0, 58.0, 55.0, 61.0, 59.0, 60.5];
        for (x, y) in xs.iter().zip(ys) {
            s.record(x.exp(), score(y));
        }
        let p = s.posterior();
        assert!((p.mean - 0.129_390_934).abs() < 1e-6, "{p:?}");
        assert!((p.sd - 0.217_886_163).abs() < 1e-6, "{p:?}");
        assert!((p.k_mean - 8.054_049_264).abs() < 1e-5, "{p:?}");
    }
}

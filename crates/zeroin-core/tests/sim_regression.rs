//! Simulation regression check for the 0.2 search (test plan 11 §2).
//!
//! A block-level port of the research simulator: 30 synthetic players (true
//! score-v2 curves from `data/sim_curves.csv`, 10 per genre preset), each run
//! from 20 starting sensitivities, with block noise, early-block learning,
//! slow fatigue and a re-adaptation cost after each sensitivity switch.
//!
//! Metrics (as in the research notes):
//! * churn: players already inside their 2-point plateau who are told to
//!   change by more than 15 %;
//! * fix: players outside the plateau whose recommendation lands inside it;
//! * regret: points lost at the recommended sens versus the true peak.
//!
//! The gate fails the build if churn at offset sd 0.4 exceeds 20 %, fix at
//! offset sd 0.8 falls below 70 %, or median regret exceeds 0.5 points.

use zeroin_core::bayes::{BayesConfig, BayesSearch, Mode};
use zeroin_core::rng::Rng;
use zeroin_core::scoring::Score;

const N_FINE: usize = 41;
const STARTS: usize = 20;

struct Player {
    curve: Vec<f64>,
    pop_centre: f64,
}

fn fine(i: usize) -> f64 {
    let (lo, hi) = (0.25f64.ln(), 4f64.ln());
    lo + (hi - lo) * i as f64 / (N_FINE - 1) as f64
}

fn interp(curve: &[f64], x: f64) -> f64 {
    let (lo, hi) = (fine(0), fine(N_FINE - 1));
    let x = x.clamp(lo, hi);
    let f = (x - lo) / (hi - lo) * (N_FINE - 1) as f64;
    let i = (f.floor() as usize).min(N_FINE - 2);
    let t = f - i as f64;
    curve[i] * (1.0 - t) + curve[i + 1] * t
}

fn players() -> Vec<Player> {
    include_str!("data/sim_curves.csv")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let f: Vec<&str> = l.split(',').collect();
            Player { pop_centre: f[1].parse().unwrap(), curve: f[2..].iter().map(|v| v.parse().unwrap()).collect() }
        })
        .collect()
}

fn gauss(r: &mut Rng) -> f64 {
    let (u1, u2) = (r.next_f64().max(1e-12), r.next_f64());
    (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
}

#[derive(Default)]
struct Stats {
    churn: Vec<bool>,
    fix: Vec<bool>,
    regret: Vec<f64>,
}

fn pct(v: &[bool]) -> f64 {
    100.0 * v.iter().filter(|b| **b).count() as f64 / v.len().max(1) as f64
}

fn quantile(v: &[f64], q: f64) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    s[((s.len() - 1) as f64 * q).round() as usize]
}

fn run(offset_sd: f64, blocks: u32, seed: u64) -> Stats {
    let mut st = Stats::default();
    let mut rng = Rng::new(seed);
    for p in players() {
        let peak_i = (0..N_FINE).max_by(|a, b| p.curve[*a].partial_cmp(&p.curve[*b]).unwrap()).unwrap();
        let (peak, x_opt) = (p.curve[peak_i], fine(peak_i));
        let plateau: Vec<f64> = (0..N_FINE).filter(|i| p.curve[*i] >= peak - 2.0).map(fine).collect();
        let (pl_lo, pl_hi) = (plateau[0], *plateau.last().unwrap());
        for _ in 0..STARTS {
            let s0 = x_opt + offset_sd * gauss(&mut rng);
            // Per-session player traits.
            let learn = rng.range(0.0, 4.0);
            let fatigue = rng.range(0.0, 0.02);
            let adapt = rng.range(0.0, 0.75);
            let cfg = BayesConfig::new(blocks, Mode::Balanced, p.pop_centre - s0, 0.3);
            let mut search = BayesSearch::new(cfg);
            let mut prev = 0.0;
            let mut i = 0;
            while let Some(x) = search.next_x() {
                let mut y = interp(&p.curve, s0 + x);
                y -= learn * (-(i as f64) / 2.5).exp();
                y -= adapt * (x - prev).abs() / 1.6f64.ln();
                y += 2.1 * (1.0 + fatigue * i as f64) * gauss(&mut rng);
                search.record(x.exp(), Score { v2: y, ..Default::default() });
                prev = x;
                i += 1;
            }
            let d = search.decision().expect("decision");
            let xr = d.mult.ln();
            let landed = s0 + xr;
            st.regret.push(peak - interp(&p.curve, landed));
            if (pl_lo..=pl_hi).contains(&s0) {
                st.churn.push(xr.abs() > 1.15f64.ln());
            } else {
                st.fix.push((pl_lo..=pl_hi).contains(&landed));
            }
        }
    }
    st
}

#[test]
fn search_meets_the_regression_gate() {
    let mut report = String::new();
    let mut fail = Vec::new();
    for (blocks, label) in [(9, "standard"), (6, "quick")] {
        let near = run(0.4, blocks, 11);
        let far = run(0.8, blocks, 12);
        let churn = pct(&near.churn);
        let fix = pct(&far.fix);
        let reg_med = quantile(&near.regret, 0.5).max(quantile(&far.regret, 0.5));
        report.push_str(&format!(
            "{label}: churn@0.4 {churn:.0}% (n={}), fix@0.8 {fix:.0}% (n={}), regret med {:.2}/{:.2} p90 {:.2}/{:.2}\n",
            near.churn.len(),
            far.fix.len(),
            quantile(&near.regret, 0.5),
            quantile(&far.regret, 0.5),
            quantile(&near.regret, 0.9),
            quantile(&far.regret, 0.9)
        ));
        if churn > 20.0 {
            fail.push(format!("{label}: churn {churn:.0}% > 20%"));
        }
        if fix < 70.0 {
            fail.push(format!("{label}: fix {fix:.0}% < 70%"));
        }
        if reg_med > 0.5 {
            fail.push(format!("{label}: median regret {reg_med:.2} > 0.5"));
        }
    }
    println!("{report}");
    assert!(fail.is_empty(), "{fail:?}\n{report}");
}

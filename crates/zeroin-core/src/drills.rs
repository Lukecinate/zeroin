//! Aim-drill simulation.
//!
//! The app feeds raw mouse counts (with timestamps in microseconds) into a
//! [`Block`], which rotates a virtual camera exactly as the chosen game would
//! at the candidate sensitivity. Targets live in world angles (yaw/pitch), so
//! the physical hand motion needed to hit them matches the game's cm/360.
//!
//! Three drill kinds are run per candidate:
//! * **Flick**: a target appears 12–45° away; move and click.
//! * **Micro-adjust**: small targets 2.5–7° away.
//! * **Tracking**: keep the crosshair on a smoothly strafing target.

use crate::rng::Rng;
use serde::{Deserialize, Serialize};

/// Camera / target orientation in degrees.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Angles {
    /// Horizontal angle; increases to the right.
    pub yaw: f64,
    /// Vertical angle; increases upward. Clamped to ±89°.
    pub pitch: f64,
}

fn wrap180(a: f64) -> f64 {
    let mut x = (a + 180.0).rem_euclid(360.0) - 180.0;
    if x == -180.0 {
        x = 180.0;
    }
    x
}

fn dir(a: Angles) -> [f64; 3] {
    let (y, p) = (a.yaw.to_radians(), a.pitch.to_radians());
    [p.cos() * y.sin(), p.sin(), p.cos() * y.cos()]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Great-circle angle between two orientations, in degrees.
pub fn angular_distance(a: Angles, b: Angles) -> f64 {
    dot(dir(a), dir(b)).clamp(-1.0, 1.0).acos().to_degrees()
}

/// Project a world orientation into normalised device coordinates of a
/// rectilinear camera. Returns `(x, y)` in `[-1, 1]` when visible (y up),
/// or `None` when the point is behind the camera.
pub fn project(cam: Angles, target: Angles, hfov_deg: f64, aspect: f64) -> Option<(f64, f64)> {
    let (cy, cp) = (cam.yaw.to_radians(), cam.pitch.to_radians());
    let f = dir(cam);
    let r = [cy.cos(), 0.0, -cy.sin()];
    let u = [-cp.sin() * cy.sin(), cp.cos(), -cp.sin() * cy.cos()];
    let t = dir(target);
    let z = dot(t, f);
    if z <= 1e-3 {
        return None;
    }
    let th = (hfov_deg.to_radians() / 2.0).tan();
    let tv = th / aspect;
    Some(((dot(t, r) / z) / th, (dot(t, u) / z) / tv))
}

/// Which drill is running.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrillKind {
    /// Large flicks.
    Flick,
    /// Small corrections.
    Micro,
    /// Continuous tracking.
    Tracking,
}

impl DrillKind {
    /// Display label.
    pub fn label(self) -> &'static str {
        match self {
            DrillKind::Flick => "Flick",
            DrillKind::Micro => "Micro-adjust",
            DrillKind::Tracking => "Tracking",
        }
    }
    /// One-line instruction.
    pub fn instruction(self) -> &'static str {
        match self {
            DrillKind::Flick => "Snap to each target and click it.",
            DrillKind::Micro => "Small targets, close together. Click each one.",
            DrillKind::Tracking => "Keep the crosshair on the moving target. No clicking.",
        }
    }
}

/// How much of each drill one candidate block contains.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DrillPlan {
    /// Number of flick targets.
    pub flicks: u32,
    /// Number of micro-adjust targets.
    pub micros: u32,
    /// Tracking duration in seconds.
    pub tracking_s: f64,
}

impl DrillPlan {
    /// About 25 s per candidate.
    pub const STANDARD: DrillPlan = DrillPlan { flicks: 10, micros: 8, tracking_s: 8.0 };
    /// About 15 s per candidate.
    pub const QUICK: DrillPlan = DrillPlan { flicks: 6, micros: 5, tracking_s: 5.0 };
}

/// A target to hit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    /// World orientation.
    pub at: Angles,
    /// Angular radius in degrees.
    pub radius_deg: f64,
}

/// Measured outcome of one click-target trial.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrialResult {
    /// Angular distance at spawn.
    pub distance_deg: f64,
    /// Target angular radius.
    pub radius_deg: f64,
    /// Clicked while on target.
    pub hit: bool,
    /// Time until the crosshair first touched the target.
    pub time_to_target_ms: Option<f64>,
    /// Time until the click (or timeout).
    pub click_ms: f64,
    /// Furthest travel past the target along the flick direction.
    pub overshoot_deg: f64,
    /// Signed error at the first stop: positive = past the target, negative = short.
    pub first_stop_error_deg: f64,
    /// Direction reversals after the first stop.
    pub corrections: u32,
}

/// Summary of a tracking run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TrackingResult {
    /// Fraction of scored time spent on target (0–1).
    pub on_target: f64,
    /// Time-weighted mean angular error in degrees.
    pub mean_error_deg: f64,
    /// Target radius used.
    pub radius_deg: f64,
}

/// Everything measured for one sensitivity candidate.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BlockResult {
    /// Flick trials.
    pub flicks: Vec<TrialResult>,
    /// Micro-adjust trials.
    pub micros: Vec<TrialResult>,
    /// Tracking summary.
    pub tracking: TrackingResult,
}

const STOP_GAP_US: u64 = 35_000;
const TRACK_GRACE_US: u64 = 600_000;

#[derive(Clone, Debug)]
struct ActiveTrial {
    start_us: u64,
    start_cam: Angles,
    target: Target,
    dist0: f64,
    dir: (f64, f64),
    first_on_us: Option<u64>,
    max_progress: f64,
    progress: f64,
    moved: bool,
    last_motion_us: u64,
    first_stop: Option<f64>,
    last_sign: i8,
    corrections: u32,
}

impl ActiveTrial {
    fn result(&self, now_us: u64, hit: bool) -> TrialResult {
        TrialResult {
            distance_deg: self.dist0,
            radius_deg: self.target.radius_deg,
            hit,
            time_to_target_ms: self.first_on_us.map(|t| (t - self.start_us) as f64 / 1000.0),
            click_ms: now_us.saturating_sub(self.start_us) as f64 / 1000.0,
            overshoot_deg: (self.max_progress - self.dist0).max(0.0),
            first_stop_error_deg: self.first_stop.unwrap_or(self.progress - self.dist0),
            corrections: self.corrections,
        }
    }
}

/// One drill of a single kind.
#[derive(Clone, Debug)]
pub struct DrillSession {
    kind: DrillKind,
    dpc: f64,
    rng: Rng,
    cam: Angles,
    target: Target,
    remaining: u32,
    trial: Option<ActiveTrial>,
    results: Vec<TrialResult>,
    // tracking
    track_start_us: u64,
    track_last_us: u64,
    track_anchor: Angles,
    track_on_us: f64,
    track_err_sum: f64,
    track_scored_us: f64,
    track_duration_us: u64,
    started: bool,
    done: bool,
}

impl DrillSession {
    /// New session. `dpc` = degrees per mouse count at the candidate sensitivity.
    pub fn new(kind: DrillKind, dpc: f64, plan: &DrillPlan, seed: u64) -> Self {
        let remaining = match kind {
            DrillKind::Flick => plan.flicks,
            DrillKind::Micro => plan.micros,
            DrillKind::Tracking => 0,
        };
        DrillSession {
            kind,
            dpc,
            rng: Rng::new(seed),
            cam: Angles::default(),
            target: Target { at: Angles::default(), radius_deg: 2.0 },
            remaining,
            trial: None,
            results: Vec::new(),
            track_start_us: 0,
            track_last_us: 0,
            track_anchor: Angles::default(),
            track_on_us: 0.0,
            track_err_sum: 0.0,
            track_scored_us: 0.0,
            track_duration_us: (plan.tracking_s.max(1.0) * 1e6) as u64,
            started: false,
            done: false,
        }
    }

    /// Drill kind.
    pub fn kind(&self) -> DrillKind {
        self.kind
    }
    /// Current camera orientation.
    pub fn camera(&self) -> Angles {
        self.cam
    }
    /// Current target, once started.
    pub fn target(&self) -> Option<Target> {
        self.started.then_some(self.target)
    }
    /// Whether the user has started this drill.
    pub fn is_started(&self) -> bool {
        self.started
    }
    /// Whether the drill is finished.
    pub fn is_done(&self) -> bool {
        self.done
    }
    /// Progress in `[0, 1]`.
    pub fn progress(&self, now_us: u64) -> f64 {
        if self.done {
            return 1.0;
        }
        if !self.started {
            return 0.0;
        }
        match self.kind {
            DrillKind::Tracking => (now_us.saturating_sub(self.track_start_us) as f64 / self.track_duration_us as f64).min(1.0),
            _ => {
                let total = self.results.len() as f64 + self.remaining as f64 + self.trial.is_some() as u8 as f64;
                if total == 0.0 {
                    1.0
                } else {
                    self.results.len() as f64 / total
                }
            }
        }
    }

    fn spawn(&mut self, now_us: u64) {
        let (dmin, dmax, radius) = match self.kind {
            DrillKind::Flick => (12.0, 45.0, 1.6),
            DrillKind::Micro => (2.5, 7.0, 0.7),
            DrillKind::Tracking => unreachable!(),
        };
        let d = self.rng.range(dmin, dmax);
        // Mostly horizontal directions, either side, with some vertical component.
        let side = if self.rng.next_f64() < 0.5 { 0.0 } else { 180.0 };
        let mut theta = (side + self.rng.range(-35.0, 35.0)).to_radians();
        // Keep targets inside a comfortable pitch band.
        if (self.cam.pitch + d * theta.sin()).abs() > 20.0 {
            theta = -theta;
        }
        let (dy, dp) = (d * theta.cos(), d * theta.sin());
        let at = Angles { yaw: wrap180(self.cam.yaw + dy), pitch: (self.cam.pitch + dp).clamp(-30.0, 30.0) };
        self.target = Target { at, radius_deg: radius };
        // Planar distance in yaw/pitch space so progress along the flick
        // direction and the reference distance use the same metric.
        let dp = at.pitch - self.cam.pitch;
        let n = (dy * dy + dp * dp).sqrt().max(1e-9);
        let dist0 = n;
        self.trial = Some(ActiveTrial {
            start_us: now_us,
            start_cam: self.cam,
            target: self.target,
            dist0,
            dir: (dy / n, dp / n),
            first_on_us: None,
            max_progress: 0.0,
            progress: 0.0,
            moved: false,
            last_motion_us: now_us,
            first_stop: None,
            last_sign: 0,
            corrections: 0,
        });
        self.remaining -= 1;
    }

    fn tracking_target(&self, now_us: u64) -> Angles {
        let t = now_us.saturating_sub(self.track_start_us) as f64 / 1e6;
        let tau = std::f64::consts::TAU;
        Angles {
            yaw: wrap180(self.track_anchor.yaw + 16.0 * (tau * 0.23 * t).sin() + 8.0 * (tau * 0.61 * t + 1.3).sin()),
            pitch: self.track_anchor.pitch + 2.5 * (tau * 0.37 * t).sin(),
        }
    }

    /// Start the drill (first click on the "click to start" prompt).
    pub fn start(&mut self, now_us: u64) {
        if self.started || self.done {
            return;
        }
        self.started = true;
        match self.kind {
            DrillKind::Tracking => {
                self.track_start_us = now_us;
                self.track_last_us = now_us;
                self.track_anchor = self.cam;
                self.target = Target { at: self.tracking_target(now_us), radius_deg: 2.0 };
            }
            _ => {
                if self.remaining == 0 {
                    self.done = true;
                } else {
                    self.spawn(now_us);
                }
            }
        }
    }

    /// Apply raw mouse counts. `dy` positive means the mouse moved toward the user (look down).
    pub fn motion(&mut self, dx: i32, dy: i32, now_us: u64) {
        if self.done {
            return;
        }
        let dyaw = dx as f64 * self.dpc;
        let dpitch = -(dy as f64) * self.dpc;
        self.cam.yaw = wrap180(self.cam.yaw + dyaw);
        self.cam.pitch = (self.cam.pitch + dpitch).clamp(-89.0, 89.0);
        if let Some(tr) = self.trial.as_mut() {
            let rel_y = wrap180(self.cam.yaw - tr.start_cam.yaw);
            let rel_p = self.cam.pitch - tr.start_cam.pitch;
            tr.progress = rel_y * tr.dir.0 + rel_p * tr.dir.1;
            tr.max_progress = tr.max_progress.max(tr.progress);
            let along = dyaw * tr.dir.0 + dpitch * tr.dir.1;
            let sign = if along > 1e-6 {
                1
            } else if along < -1e-6 {
                -1
            } else {
                0
            };
            if sign != 0 {
                if tr.first_stop.is_some() && tr.last_sign != 0 && sign != tr.last_sign {
                    tr.corrections += 1;
                }
                tr.last_sign = sign;
                tr.moved = true;
                tr.last_motion_us = now_us;
            }
            if tr.first_on_us.is_none() && angular_distance(self.cam, tr.target.at) <= tr.target.radius_deg {
                tr.first_on_us = Some(now_us);
            }
        }
    }

    /// Left click. Returns `Some(hit)` when the click resolved a trial.
    pub fn click(&mut self, now_us: u64) -> Option<bool> {
        if !self.started {
            self.start(now_us);
            return None;
        }
        if self.done || self.kind == DrillKind::Tracking {
            return None;
        }
        let tr = self.trial.take()?;
        let hit = angular_distance(self.cam, tr.target.at) <= tr.target.radius_deg;
        self.results.push(tr.result(now_us, hit));
        self.next_or_finish(now_us);
        Some(hit)
    }

    fn next_or_finish(&mut self, now_us: u64) {
        if self.remaining == 0 {
            self.done = true;
        } else {
            self.spawn(now_us);
        }
    }

    /// Advance time (call every frame and after input).
    pub fn tick(&mut self, now_us: u64) {
        if !self.started || self.done {
            return;
        }
        match self.kind {
            DrillKind::Tracking => {
                let at = self.tracking_target(now_us);
                self.target.at = at;
                let dt = now_us.saturating_sub(self.track_last_us) as f64;
                self.track_last_us = now_us;
                if now_us.saturating_sub(self.track_start_us) > TRACK_GRACE_US {
                    let err = angular_distance(self.cam, at);
                    self.track_scored_us += dt;
                    self.track_err_sum += err * dt;
                    if err <= self.target.radius_deg {
                        self.track_on_us += dt;
                    }
                }
                if now_us.saturating_sub(self.track_start_us) >= self.track_duration_us {
                    self.done = true;
                }
            }
            _ => {
                let timeout_us = if self.kind == DrillKind::Flick { 2_500_000 } else { 2_000_000 };
                let mut timed_out = false;
                if let Some(tr) = self.trial.as_mut() {
                    if tr.moved
                        && tr.first_stop.is_none()
                        && now_us.saturating_sub(tr.last_motion_us) > STOP_GAP_US
                        && tr.max_progress > 0.2 * tr.dist0
                    {
                        tr.first_stop = Some(tr.progress - tr.dist0);
                    }
                    timed_out = now_us.saturating_sub(tr.start_us) > timeout_us;
                }
                if timed_out {
                    let tr = self.trial.take().expect("trial");
                    self.results.push(tr.result(now_us, false));
                    self.next_or_finish(now_us);
                }
            }
        }
    }

    /// Click-trial results so far.
    pub fn trials(&self) -> &[TrialResult] {
        &self.results
    }

    /// Tracking summary.
    pub fn tracking_result(&self) -> TrackingResult {
        if self.track_scored_us <= 0.0 {
            return TrackingResult { on_target: 0.0, mean_error_deg: 0.0, radius_deg: self.target.radius_deg };
        }
        TrackingResult {
            on_target: self.track_on_us / self.track_scored_us,
            mean_error_deg: self.track_err_sum / self.track_scored_us,
            radius_deg: self.target.radius_deg,
        }
    }
}

/// The three drills run for one sensitivity candidate, in order.
#[derive(Clone, Debug)]
pub struct Block {
    sessions: Vec<DrillSession>,
    index: usize,
}

impl Block {
    /// Create a block for a candidate with `dpc` degrees per count.
    pub fn new(dpc: f64, plan: &DrillPlan, seed: u64) -> Self {
        let sessions = [DrillKind::Flick, DrillKind::Micro, DrillKind::Tracking]
            .iter()
            .enumerate()
            .map(|(i, k)| DrillSession::new(*k, dpc, plan, seed.wrapping_add(i as u64 * 7919)))
            .collect();
        Block { sessions, index: 0 }
    }

    /// The drill currently shown.
    pub fn current(&self) -> &DrillSession {
        &self.sessions[self.index.min(self.sessions.len() - 1)]
    }

    fn cur_mut(&mut self) -> &mut DrillSession {
        let i = self.index.min(self.sessions.len() - 1);
        &mut self.sessions[i]
    }

    /// Index of the current drill and the total count.
    pub fn position(&self) -> (usize, usize) {
        (self.index.min(self.sessions.len() - 1), self.sessions.len())
    }

    /// Forward raw motion.
    pub fn motion(&mut self, dx: i32, dy: i32, now_us: u64) {
        if !self.is_done() {
            self.cur_mut().motion(dx, dy, now_us);
        }
    }

    /// Forward a click.
    pub fn click(&mut self, now_us: u64) -> Option<bool> {
        if self.is_done() {
            return None;
        }
        let r = self.cur_mut().click(now_us);
        self.advance();
        r
    }

    /// Forward a frame tick.
    pub fn tick(&mut self, now_us: u64) {
        if !self.is_done() {
            self.cur_mut().tick(now_us);
            self.advance();
        }
    }

    fn advance(&mut self) {
        if self.index < self.sessions.len() && self.sessions[self.index].is_done() {
            self.index += 1;
        }
    }

    /// All drills finished.
    pub fn is_done(&self) -> bool {
        self.index >= self.sessions.len()
    }

    /// Overall progress in `[0, 1]`.
    pub fn progress(&self, now_us: u64) -> f64 {
        let n = self.sessions.len() as f64;
        if self.is_done() {
            return 1.0;
        }
        (self.index as f64 + self.sessions[self.index].progress(now_us)) / n
    }

    /// Collected measurements.
    pub fn result(&self) -> BlockResult {
        let mut r = BlockResult::default();
        for s in &self.sessions {
            match s.kind() {
                DrillKind::Flick => r.flicks = s.trials().to_vec(),
                DrillKind::Micro => r.micros = s.trials().to_vec(),
                DrillKind::Tracking => r.tracking = s.tracking_result(),
            }
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_and_projection_basics() {
        let a = Angles { yaw: 0.0, pitch: 0.0 };
        let b = Angles { yaw: 30.0, pitch: 0.0 };
        assert!((angular_distance(a, b) - 30.0).abs() < 1e-9);
        let (x, y) = project(a, a, 103.0, 16.0 / 9.0).unwrap();
        assert!(x.abs() < 1e-12 && y.abs() < 1e-12);
        let (x, _) = project(a, Angles { yaw: 51.5, pitch: 0.0 }, 103.0, 16.0 / 9.0).unwrap();
        assert!((x - 1.0).abs() < 1e-9, "{x}");
        let (_, y) = project(a, Angles { yaw: 0.0, pitch: 10.0 }, 103.0, 16.0 / 9.0).unwrap();
        assert!(y > 0.0);
        assert!(project(a, Angles { yaw: 180.0, pitch: 0.0 }, 103.0, 1.7).is_none());
        assert!((wrap180(190.0) + 170.0).abs() < 1e-12);
    }

    /// A perfect bot: moves straight to each target in `steps` motion events, then clicks.
    fn run_bot(kind: DrillKind, dpc: f64, overshoot: f64) -> Vec<TrialResult> {
        let plan = DrillPlan { flicks: 5, micros: 5, tracking_s: 2.0 };
        let mut s = DrillSession::new(kind, dpc, &plan, 42);
        let mut t = 0u64;
        s.click(t); // start
        while !s.is_done() {
            let tg = s.target().unwrap().at;
            let cam = s.camera();
            let dy = wrap180(tg.yaw - cam.yaw) * (1.0 + overshoot);
            let dp = (tg.pitch - cam.pitch) * (1.0 + overshoot);
            let steps = 20;
            let (tx, ty) = ((dy / dpc).round() as i32, (-dp / dpc).round() as i32);
            let (mut sx, mut sy) = (0i32, 0i32);
            for k in 1..=steps {
                t += 5_000;
                let (nx, ny) = (tx * k / steps, ty * k / steps);
                s.motion(nx - sx, ny - sy, t);
                (sx, sy) = (nx, ny);
                s.tick(t);
            }
            t += 60_000;
            s.tick(t);
            if overshoot != 0.0 {
                // correct back
                let tg = s.target().unwrap().at;
                let cam = s.camera();
                t += 5_000;
                s.motion((wrap180(tg.yaw - cam.yaw) / dpc).round() as i32, (-(tg.pitch - cam.pitch) / dpc).round() as i32, t);
            }
            t += 20_000;
            s.click(t);
        }
        s.trials().to_vec()
    }

    #[test]
    fn perfect_bot_hits_everything() {
        for kind in [DrillKind::Flick, DrillKind::Micro] {
            let r = run_bot(kind, 0.022 * 1.5, 0.0);
            assert_eq!(r.len(), 5);
            assert!(r.iter().all(|t| t.hit), "{kind:?}: {r:?}");
            assert!(r.iter().all(|t| t.overshoot_deg < 0.2));
            assert!(r.iter().all(|t| t.time_to_target_ms.is_some()));
        }
    }

    #[test]
    fn overshoot_is_measured() {
        let r = run_bot(DrillKind::Flick, 0.022, 0.25);
        assert!(r.iter().all(|t| t.hit));
        assert!(r.iter().all(|t| t.first_stop_error_deg > 1.0), "{r:?}");
        assert!(r.iter().all(|t| t.overshoot_deg > 1.0));
        assert!(r.iter().any(|t| t.corrections >= 1));
    }

    #[test]
    fn timeout_counts_as_miss() {
        let plan = DrillPlan { flicks: 2, micros: 0, tracking_s: 1.0 };
        let mut s = DrillSession::new(DrillKind::Flick, 0.02, &plan, 1);
        s.click(0);
        s.tick(3_000_000);
        s.tick(6_000_000);
        assert!(s.is_done());
        assert_eq!(s.trials().len(), 2);
        assert!(s.trials().iter().all(|t| !t.hit));
    }

    #[test]
    fn tracking_still_mouse_scores_low_and_follower_high() {
        let plan = DrillPlan { flicks: 0, micros: 0, tracking_s: 3.0 };
        let dpc = 0.01;
        // Idle
        let mut idle = DrillSession::new(DrillKind::Tracking, dpc, &plan, 3);
        idle.click(0);
        let mut t = 0;
        while !idle.is_done() {
            t += 4_000;
            idle.tick(t);
        }
        // Follower
        let mut f = DrillSession::new(DrillKind::Tracking, dpc, &plan, 3);
        f.click(0);
        let mut t = 0;
        while !f.is_done() {
            t += 4_000;
            f.tick(t);
            let tg = f.target().unwrap().at;
            let cam = f.camera();
            f.motion((wrap180(tg.yaw - cam.yaw) / dpc).round() as i32, (-(tg.pitch - cam.pitch) / dpc).round() as i32, t);
        }
        assert!(idle.tracking_result().on_target < 0.4);
        assert!(f.tracking_result().on_target > 0.95, "{:?}", f.tracking_result());
    }

    #[test]
    fn block_runs_all_three() {
        let plan = DrillPlan { flicks: 1, micros: 1, tracking_s: 1.0 };
        let mut b = Block::new(0.02, &plan, 9);
        let mut t = 0;
        let mut guard = 0;
        while !b.is_done() && guard < 10_000 {
            guard += 1;
            b.click(t);
            t += 100_000;
            b.tick(t);
        }
        assert!(b.is_done());
        let r = b.result();
        assert_eq!(r.flicks.len(), 1);
        assert_eq!(r.micros.len(), 1);
        assert_eq!(b.progress(t), 1.0);
    }
}

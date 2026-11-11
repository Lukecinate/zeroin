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

/// Target geometry for a genre of game.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    /// Small heads, short flicks, slow strafes (Valorant, CS2, Rainbow Six Siege).
    Tactical,
    /// The default geometry.
    Balanced,
    /// Large flicks and fast strafes (Apex Legends, Fortnite, The Finals).
    Tracking,
}

impl Preset {
    /// Preset used for a game id; unknown and custom games use [`Preset::Balanced`].
    pub fn for_game(id: &str) -> Preset {
        match id {
            "valorant" | "cs2" | "r6s" => Preset::Tactical,
            "apex" | "fortnite" | "the_finals" => Preset::Tracking,
            _ => Preset::Balanced,
        }
    }

    /// Display label.
    pub fn label(self) -> &'static str {
        match self {
            Preset::Tactical => "tactical",
            Preset::Balanced => "balanced",
            Preset::Tracking => "tracking-heavy",
        }
    }
}

/// How much of each drill one block contains, and the target geometry.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DrillPlan {
    /// Unscored warm-up flicks at the start of each block.
    #[serde(default)]
    pub warmup: u32,
    /// Number of scored flick targets.
    pub flicks: u32,
    /// Number of micro-adjust targets.
    pub micros: u32,
    /// Tracking duration in seconds.
    pub tracking_s: f64,
    /// Flick distance range in degrees.
    #[serde(default = "default_flick_range")]
    pub flick_range: (f64, f64),
    /// Flick target radius in degrees.
    #[serde(default = "default_flick_radius")]
    pub flick_radius: f64,
    /// Micro-adjust distance range in degrees.
    #[serde(default = "default_micro_range")]
    pub micro_range: (f64, f64),
    /// Micro-adjust target radius in degrees.
    #[serde(default = "default_micro_radius")]
    pub micro_radius: f64,
    /// Tracking path speed factor.
    #[serde(default = "default_track_speed")]
    pub track_speed: f64,
}

fn default_flick_range() -> (f64, f64) {
    (12.0, 45.0)
}
fn default_flick_radius() -> f64 {
    1.6
}
fn default_micro_range() -> (f64, f64) {
    (2.5, 7.0)
}
fn default_micro_radius() -> f64 {
    0.7
}
fn default_track_speed() -> f64 {
    1.0
}

impl DrillPlan {
    /// The 0.2 block: 2 warm-up flicks, 6 flicks, 5 micro-adjusts and 5 s of tracking.
    pub const BALANCED: DrillPlan = DrillPlan {
        warmup: 2,
        flicks: 6,
        micros: 5,
        tracking_s: 5.0,
        flick_range: (12.0, 45.0),
        flick_radius: 1.6,
        micro_range: (2.5, 7.0),
        micro_radius: 0.7,
        track_speed: 1.0,
    };

    /// Block plan for a preset.
    pub fn for_preset(p: Preset) -> DrillPlan {
        match p {
            Preset::Balanced => Self::BALANCED,
            Preset::Tactical => DrillPlan {
                flick_range: (8.0, 40.0),
                flick_radius: 0.9,
                micro_range: (1.5, 5.0),
                micro_radius: 0.45,
                track_speed: 0.6,
                ..Self::BALANCED
            },
            Preset::Tracking => DrillPlan {
                flick_range: (20.0, 90.0),
                flick_radius: 2.0,
                micro_range: (3.0, 8.0),
                micro_radius: 1.0,
                track_speed: 1.6,
                ..Self::BALANCED
            },
        }
    }

    /// Rough wall-clock length of one block in seconds, including start prompts.
    pub fn estimated_block_s(&self) -> f64 {
        (self.warmup + self.flicks) as f64 * 1.1 + self.micros as f64 * 0.9 + self.tracking_s + 6.0
    }
}

/// A target to hit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    /// World orientation.
    pub at: Angles,
    /// Angular radius in degrees.
    pub radius_deg: f64,
}

/// One planned click target, relative to where the camera is when it spawns.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TargetSpec {
    /// Angular distance in degrees.
    pub distance_deg: f64,
    /// Direction in degrees: 0 = right, 180 = left, positive = up.
    pub direction_deg: f64,
}

/// Distances stratified log-uniformly over `range` (one per bin) with
/// balanced left/right directions and a small vertical component.
pub fn stratified_targets(n: u32, range: (f64, f64), rng: &mut Rng) -> Vec<TargetSpec> {
    let (lo, hi) = (range.0.max(0.1).ln(), range.1.max(range.0 + 0.01).ln());
    (0..n)
        .map(|i| {
            let a = lo + (hi - lo) * (i as f64 + rng.next_f64()) / n as f64;
            let d = a.exp();
            let side = if i % 2 == 0 { 0.0 } else { 180.0 };
            // Vertical offset of up to ±8°, never more than 35° of direction.
            let v = rng.range(-8.0, 8.0);
            let tilt = (v / d).clamp(-0.57, 0.57).asin().to_degrees();
            let direction = if side == 0.0 { tilt } else { 180.0 - tilt };
            TargetSpec { distance_deg: d, direction_deg: direction }
        })
        .collect()
}

fn shuffle<T>(v: &mut [T], rng: &mut Rng) {
    for i in (1..v.len()).rev() {
        let j = (rng.next_u64() % (i as u64 + 1)) as usize;
        v.swap(i, j);
    }
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
    /// Clicked faster than a reaction is possible (< 120 ms); counted as a miss.
    #[serde(default)]
    pub anticipated: bool,
    /// The trial ran out of time.
    #[serde(default)]
    pub timed_out: bool,
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
    /// Time-weighted RMS angular error in degrees.
    #[serde(default)]
    pub rms_error_deg: f64,
}

/// Everything measured for one block.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BlockResult {
    /// Scored flick trials (warm-up flicks are not included).
    pub flicks: Vec<TrialResult>,
    /// Micro-adjust trials.
    pub micros: Vec<TrialResult>,
    /// Tracking summary.
    pub tracking: TrackingResult,
    /// Input gaps over 250 ms in the middle of a movement.
    #[serde(default)]
    pub input_gaps: u32,
}

/// Minimum plausible click time; faster clicks count as anticipations.
pub const ANTICIPATION_MS: f64 = 120.0;
/// Input gap in the middle of a movement that marks a block as unreliable.
pub const INPUT_GAP_US: u64 = 250_000;
/// Hand speed (fraction of the trial's peak) on both sides of a gap for it
/// to count as dropped input. Stricter than the 10 % in the spec, to avoid
/// re-running blocks after a natural pause.
const GAP_SPEED_FRAC: f64 = 0.25;
const STOP_GAP_US: u64 = 35_000;
const TRACK_GRACE_US: u64 = 600_000;
/// Tracking error is integrated in steps of at most this length.
const TRACK_STEP_US: u64 = 1_000;

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
    warmup: bool,
    peak_speed: f64,
    last_speed: f64,
    gap_pending: bool,
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
            anticipated: false,
            timed_out: false,
        }
    }
}

/// One drill of a single kind.
#[derive(Clone, Debug)]
pub struct DrillSession {
    kind: DrillKind,
    dpc: f64,
    cam: Angles,
    target: Target,
    queue: Vec<(TargetSpec, bool)>,
    next: usize,
    radius: f64,
    trial: Option<ActiveTrial>,
    results: Vec<TrialResult>,
    input_gaps: u32,
    // tracking
    track_start_us: u64,
    track_last_us: u64,
    track_anchor: Angles,
    track_speed: f64,
    track_on_us: f64,
    track_err_sum: f64,
    track_err2_sum: f64,
    track_scored_us: f64,
    track_duration_us: u64,
    started: bool,
    done: bool,
}

impl DrillSession {
    /// New session. `dpc` = degrees per mouse count at the candidate sensitivity.
    /// The target set comes from `pair_seed` (keep it fixed for a whole test);
    /// its order comes from `order_seed` (vary it per block).
    pub fn new(kind: DrillKind, dpc: f64, plan: &DrillPlan, pair_seed: u64, order_seed: u64) -> Self {
        let mut pair = Rng::new(pair_seed ^ (kind as u64 + 1).wrapping_mul(0xA24B_AED4_963E_E407));
        let mut order = Rng::new(order_seed ^ (kind as u64 + 1).wrapping_mul(0x9FB2_1C65_1E98_DF25));
        let (queue, radius) = match kind {
            DrillKind::Flick => {
                let warm = stratified_targets(plan.warmup, plan.flick_range, &mut pair);
                let mut scored = stratified_targets(plan.flicks, plan.flick_range, &mut pair);
                shuffle(&mut scored, &mut order);
                let q = warm.into_iter().map(|t| (t, true)).chain(scored.into_iter().map(|t| (t, false))).collect();
                (q, plan.flick_radius)
            }
            DrillKind::Micro => {
                let mut scored = stratified_targets(plan.micros, plan.micro_range, &mut pair);
                shuffle(&mut scored, &mut order);
                (scored.into_iter().map(|t| (t, false)).collect(), plan.micro_radius)
            }
            DrillKind::Tracking => (Vec::new(), 2.0),
        };
        DrillSession {
            kind,
            dpc,
            cam: Angles::default(),
            target: Target { at: Angles::default(), radius_deg: radius },
            queue,
            next: 0,
            radius,
            trial: None,
            results: Vec::new(),
            input_gaps: 0,
            track_start_us: 0,
            track_last_us: 0,
            track_anchor: Angles::default(),
            track_speed: plan.track_speed.clamp(0.2, 3.0),
            track_on_us: 0.0,
            track_err_sum: 0.0,
            track_err2_sum: 0.0,
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
    /// True while the current target is an unscored warm-up flick.
    pub fn is_warmup(&self) -> bool {
        self.trial.as_ref().is_some_and(|t| t.warmup)
    }
    /// Planned click targets (warm-up first), for replay and tests.
    pub fn planned(&self) -> Vec<TargetSpec> {
        self.queue.iter().map(|(t, _)| *t).collect()
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
                let total = self.queue.len() as f64;
                if total == 0.0 {
                    1.0
                } else {
                    (self.next as f64 - self.trial.is_some() as u8 as f64).max(0.0) / total
                }
            }
        }
    }

    fn spawn(&mut self, now_us: u64) {
        let (spec, warmup) = self.queue[self.next];
        self.next += 1;
        let d = spec.distance_deg;
        let mut theta = spec.direction_deg.to_radians();
        // Keep targets inside a comfortable pitch band.
        if (self.cam.pitch + d * theta.sin()).abs() > 20.0 {
            theta = -theta;
        }
        let (dy, dp) = (d * theta.cos(), d * theta.sin());
        let at = Angles { yaw: wrap180(self.cam.yaw + dy), pitch: (self.cam.pitch + dp).clamp(-30.0, 30.0) };
        self.target = Target { at, radius_deg: self.radius };
        // Planar distance in yaw/pitch space so progress along the flick
        // direction and the reference distance use the same metric.
        let dp = at.pitch - self.cam.pitch;
        let n = (dy * dy + dp * dp).sqrt().max(1e-9);
        self.trial = Some(ActiveTrial {
            start_us: now_us,
            start_cam: self.cam,
            target: self.target,
            dist0: n,
            dir: (dy / n, dp / n),
            first_on_us: None,
            max_progress: 0.0,
            progress: 0.0,
            moved: false,
            last_motion_us: now_us,
            first_stop: None,
            last_sign: 0,
            corrections: 0,
            warmup,
            peak_speed: 0.0,
            last_speed: 0.0,
            gap_pending: false,
        });
    }

    fn tracking_target(&self, at_us: u64) -> Angles {
        let t = at_us.saturating_sub(self.track_start_us) as f64 / 1e6 * self.track_speed;
        let tau = std::f64::consts::TAU;
        Angles {
            yaw: wrap180(self.track_anchor.yaw + 16.0 * (tau * 0.23 * t).sin() + 8.0 * (tau * 0.61 * t + 1.3).sin()),
            pitch: self.track_anchor.pitch + 2.5 * (tau * 0.37 * t).sin(),
        }
    }

    /// Integrate tracking error from the last update to `to_us`. The camera
    /// only changes on input events, so between two events it is constant
    /// and the target path is evaluated analytically in small steps. The
    /// result therefore depends on input timestamps, not on frame times.
    fn integrate_tracking(&mut self, to_us: u64) {
        let end = to_us.min(self.track_start_us + self.track_duration_us);
        let grace = self.track_start_us + TRACK_GRACE_US;
        let mut t = self.track_last_us.max(grace);
        while t < end {
            let step = (end - t).min(TRACK_STEP_US);
            let mid = t + step / 2;
            let err = angular_distance(self.cam, self.tracking_target(mid));
            let dt = step as f64;
            self.track_scored_us += dt;
            self.track_err_sum += err * dt;
            self.track_err2_sum += err * err * dt;
            if err <= self.target.radius_deg {
                self.track_on_us += dt;
            }
            t += step;
        }
        self.track_last_us = self.track_last_us.max(to_us);
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
            _ => self.next_or_finish(now_us),
        }
    }

    /// Apply raw mouse counts. `dy` positive means the mouse moved toward the user (look down).
    pub fn motion(&mut self, dx: i32, dy: i32, now_us: u64) {
        if self.done {
            return;
        }
        if self.kind == DrillKind::Tracking && self.started {
            // Error up to this event is measured with the camera as it was.
            self.integrate_tracking(now_us);
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
                // Hand speed in degrees per ms, used to spot dropped input.
                let gap = now_us.saturating_sub(tr.last_motion_us);
                let speed = (dyaw.hypot(dpitch)) / (gap.max(125) as f64 / 1000.0);
                // Smoothed so single fast reports at the end of a stop do not count.
                if tr.gap_pending {
                    if gap < 50_000 && speed > GAP_SPEED_FRAC * tr.peak_speed {
                        self.input_gaps += 1;
                    }
                    tr.gap_pending = false;
                }
                if tr.moved && gap > INPUT_GAP_US && tr.last_speed > GAP_SPEED_FRAC * tr.peak_speed {
                    tr.gap_pending = true;
                    tr.last_speed = 0.0;
                } else {
                    tr.last_speed = 0.7 * tr.last_speed + 0.3 * speed;
                    tr.peak_speed = tr.peak_speed.max(tr.last_speed);
                }
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
        let on = angular_distance(self.cam, tr.target.at) <= tr.target.radius_deg;
        let mut r = tr.result(now_us, on);
        if r.click_ms < ANTICIPATION_MS {
            r.anticipated = true;
            r.hit = false;
        }
        if !tr.warmup {
            self.results.push(r);
        }
        self.next_or_finish(now_us);
        Some(r.hit)
    }

    fn next_or_finish(&mut self, now_us: u64) {
        if self.next >= self.queue.len() {
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
                self.integrate_tracking(now_us);
                self.target.at = self.tracking_target(now_us);
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
                    if !tr.warmup {
                        let mut r = tr.result(now_us, false);
                        r.timed_out = true;
                        self.results.push(r);
                    }
                    self.next_or_finish(now_us);
                }
            }
        }
    }

    /// Scored click-trial results so far.
    pub fn trials(&self) -> &[TrialResult] {
        &self.results
    }

    /// Input gaps seen so far.
    pub fn input_gaps(&self) -> u32 {
        self.input_gaps
    }

    /// Tracking summary.
    pub fn tracking_result(&self) -> TrackingResult {
        if self.track_scored_us <= 0.0 {
            return TrackingResult { on_target: 0.0, mean_error_deg: 0.0, radius_deg: self.target.radius_deg, rms_error_deg: 0.0 };
        }
        TrackingResult {
            on_target: self.track_on_us / self.track_scored_us,
            mean_error_deg: self.track_err_sum / self.track_scored_us,
            radius_deg: self.target.radius_deg,
            rms_error_deg: (self.track_err2_sum / self.track_scored_us).sqrt(),
        }
    }
}

/// Why a block's measurements were not used.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvalidReason {
    /// The app stalled: long frames while a drill was running.
    Stutter,
    /// Mouse input stopped for over 250 ms in the middle of a movement.
    InputGap,
    /// More than 2 clicks faster than 120 ms.
    Anticipations,
    /// More than 3 targets timed out.
    Timeouts,
    /// Mouse reports arrived with chaotic timing (polling jitter far above
    /// the pre-test reading).
    PollingJitter,
}

impl InvalidReason {
    /// Plain-language explanation.
    pub fn message(self) -> &'static str {
        match self {
            InvalidReason::Stutter => "The app stuttered during the block (frames over 100 ms).",
            InvalidReason::InputGap => "Mouse input stopped in the middle of a movement (a gap over 250 ms).",
            InvalidReason::Anticipations => "More than two clicks came faster than a reaction is possible.",
            InvalidReason::Timeouts => "More than three targets timed out.",
            InvalidReason::PollingJitter => "Mouse reports arrived with very uneven timing during the block.",
        }
    }
}

impl InvalidReason {
    /// What the player can do about it.
    pub fn advice(self) -> &'static str {
        match self {
            InvalidReason::Stutter => "Close overlays, recording software and other heavy apps, then try again.",
            InvalidReason::InputGap => {
                "Plug the mouse straight into the PC (not a hub), close mouse or RGB software overlays, and check the wireless receiver is close."
            }
            InvalidReason::Anticipations => "Wait for each target to appear before clicking.",
            InvalidReason::Timeouts => "Click each target before it disappears: flick targets last 2.5 s and micro targets 2 s.",
            InvalidReason::PollingJitter => {
                "Close background apps, plug the mouse into a rear USB port without a hub, or lower the polling rate. Settings → Check mouse shows the details."
            }
        }
    }
}

/// A frame longer than this, while a drill runs, counts as a hitch.
pub const HITCH_US: u64 = 100_000;
/// A single frame longer than this invalidates the block.
pub const SEVERE_HITCH_US: u64 = 250_000;

/// The three drills run for one block, in order.
#[derive(Clone, Debug)]
pub struct Block {
    sessions: Vec<DrillSession>,
    index: usize,
    last_frame_us: Option<u64>,
    hitches: u32,
    severe_hitch: bool,
    /// `Some(baseline)` when the polling-jitter rule is active (raw input only).
    polling_rule: Option<Option<crate::polling::Jitter>>,
    motion_log: Vec<(u64, i32, i32)>,
}

/// Motion reports kept per block for the polling-jitter rule.
const MOTION_LOG_CAP: usize = 500_000;

impl Block {
    /// Create a block with `dpc` degrees per count. `pair_seed` fixes the
    /// target set for the whole test; `order_seed` shuffles it for this block.
    pub fn new(dpc: f64, plan: &DrillPlan, pair_seed: u64, order_seed: u64) -> Self {
        let sessions = [DrillKind::Flick, DrillKind::Micro, DrillKind::Tracking]
            .iter()
            .map(|k| DrillSession::new(*k, dpc, plan, pair_seed, order_seed))
            .collect();
        Block { sessions, index: 0, last_frame_us: None, hitches: 0, severe_hitch: false, polling_rule: None, motion_log: Vec::new() }
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

    /// Turn on the polling-jitter rule with the pre-test reading, if any.
    /// Only meaningful with raw input timestamps.
    pub fn enable_polling_rule(&mut self, baseline: Option<crate::polling::Jitter>) {
        self.polling_rule = Some(baseline);
    }

    /// Polling jitter measured in this block so far.
    pub fn polling_jitter(&self) -> Option<crate::polling::Jitter> {
        crate::polling::jitter(&self.motion_log)
    }

    /// Forward raw motion.
    pub fn motion(&mut self, dx: i32, dy: i32, now_us: u64) {
        if self.polling_rule.is_some() && self.motion_log.len() < MOTION_LOG_CAP {
            self.motion_log.push((now_us, dx, dy));
        }
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

    /// Forward a time tick.
    pub fn tick(&mut self, now_us: u64) {
        if !self.is_done() {
            self.cur_mut().tick(now_us);
            self.advance();
        }
    }

    /// Record a rendered frame. Long frames while a drill is running mark
    /// the block as stuttering. Gaps while a start prompt is shown are ignored.
    pub fn frame(&mut self, now_us: u64) {
        let live = !self.is_done() && self.current().is_started();
        if let (Some(prev), true) = (self.last_frame_us, live) {
            let dt = now_us.saturating_sub(prev);
            if dt > HITCH_US {
                self.hitches += 1;
            }
            if dt > SEVERE_HITCH_US {
                self.severe_hitch = true;
            }
        }
        self.last_frame_us = live.then_some(now_us);
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
            r.input_gaps += s.input_gaps();
            match s.kind() {
                DrillKind::Flick => r.flicks = s.trials().to_vec(),
                DrillKind::Micro => r.micros = s.trials().to_vec(),
                DrillKind::Tracking => r.tracking = s.tracking_result(),
            }
        }
        r
    }

    /// Validity check for a finished block (spec: re-run when it fails).
    pub fn validity(&self) -> Result<(), InvalidReason> {
        if self.severe_hitch || self.hitches >= 3 {
            return Err(InvalidReason::Stutter);
        }
        block_validity(&self.result())?;
        if let Some(baseline) = &self.polling_rule {
            if let Some(j) = self.polling_jitter() {
                if crate::polling::jitter_is_chaotic(&j, baseline.as_ref()) {
                    return Err(InvalidReason::PollingJitter);
                }
            }
        }
        Ok(())
    }
}

/// Validity rules that depend only on the measurements.
pub fn block_validity(r: &BlockResult) -> Result<(), InvalidReason> {
    let trials = r.flicks.iter().chain(r.micros.iter());
    let (mut ant, mut tout) = (0, 0);
    for t in trials {
        ant += t.anticipated as u32;
        tout += t.timed_out as u32;
    }
    if r.input_gaps > 0 {
        Err(InvalidReason::InputGap)
    } else if ant > 2 {
        Err(InvalidReason::Anticipations)
    } else if tout > 3 {
        Err(InvalidReason::Timeouts)
    } else {
        Ok(())
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
        let plan = DrillPlan { warmup: 0, flicks: 5, micros: 5, tracking_s: 2.0, ..DrillPlan::BALANCED };
        let mut s = DrillSession::new(kind, dpc, &plan, 42, 7);
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
        let plan = DrillPlan { warmup: 0, flicks: 2, micros: 0, tracking_s: 1.0, ..DrillPlan::BALANCED };
        let mut s = DrillSession::new(DrillKind::Flick, 0.02, &plan, 1, 1);
        s.click(0);
        s.tick(3_000_000);
        s.tick(6_000_000);
        assert!(s.is_done());
        assert_eq!(s.trials().len(), 2);
        assert!(s.trials().iter().all(|t| !t.hit && t.timed_out));
    }

    #[test]
    fn tracking_still_mouse_scores_low_and_follower_high() {
        let plan = DrillPlan { warmup: 0, flicks: 0, micros: 0, tracking_s: 3.0, ..DrillPlan::BALANCED };
        let dpc = 0.01;
        // Idle
        let mut idle = DrillSession::new(DrillKind::Tracking, dpc, &plan, 3, 3);
        idle.click(0);
        let mut t = 0;
        while !idle.is_done() {
            t += 4_000;
            idle.tick(t);
        }
        // Follower
        let mut f = DrillSession::new(DrillKind::Tracking, dpc, &plan, 3, 3);
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
        let plan = DrillPlan { warmup: 1, flicks: 1, micros: 1, tracking_s: 1.0, ..DrillPlan::BALANCED };
        let mut b = Block::new(0.02, &plan, 9, 10);
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

    #[test]
    fn targets_are_paired_across_blocks_and_stratified() {
        let plan = DrillPlan::BALANCED;
        let a = DrillSession::new(DrillKind::Flick, 0.02, &plan, 77, 1).planned();
        let b = DrillSession::new(DrillKind::Flick, 0.02, &plan, 77, 2).planned();
        assert_eq!(a.len(), 8);
        // Warm-up targets first and identical.
        assert_eq!(a[..2], b[..2]);
        let key = |v: &[TargetSpec]| {
            let mut d: Vec<u64> = v.iter().map(|t| (t.distance_deg * 1e6) as u64).collect();
            d.sort();
            d
        };
        assert_eq!(key(&a[2..]), key(&b[2..]), "same multiset");
        assert_ne!(a[2..], b[2..], "different order");
        // One distance per log bin over 12–45°.
        let mut d: Vec<f64> = a[2..].iter().map(|t| t.distance_deg).collect();
        d.sort_by(|x, y| x.partial_cmp(y).unwrap());
        let (lo, hi) = (12f64.ln(), 45f64.ln());
        for (i, x) in d.iter().enumerate() {
            let bin = ((x.ln() - lo) / (hi - lo) * 6.0).floor() as usize;
            assert_eq!(bin, i, "{d:?}");
        }
        let right = a[2..].iter().filter(|t| t.direction_deg.abs() < 90.0).count();
        assert_eq!(right, 3);
    }

    #[test]
    fn warmup_is_not_scored_and_anticipation_is_a_miss() {
        let plan = DrillPlan { warmup: 2, flicks: 3, micros: 0, tracking_s: 1.0, ..DrillPlan::BALANCED };
        let mut s = DrillSession::new(DrillKind::Flick, 0.02, &plan, 5, 5);
        s.click(0);
        assert!(s.is_warmup());
        let mut t = 0;
        // Aim perfectly and click after 300 ms, except the last scored trial (50 ms).
        for i in 0..5 {
            let tg = s.target().unwrap().at;
            let cam = s.camera();
            t += 10_000;
            let start = t;
            s.motion((wrap180(tg.yaw - cam.yaw) / 0.02).round() as i32, (-(tg.pitch - cam.pitch) / 0.02).round() as i32, t);
            t = start + if i == 4 { 40_000 } else { 300_000 };
            s.click(t);
        }
        assert!(s.is_done());
        let r = s.trials();
        assert_eq!(r.len(), 3);
        assert!(r[0].hit && r[1].hit);
        assert!(r[2].anticipated && !r[2].hit);
    }

    /// The same hand motion scores the same whether frames come at 30 or 500 Hz.
    #[test]
    fn tracking_does_not_depend_on_frame_rate() {
        let plan = DrillPlan { warmup: 0, flicks: 0, micros: 0, tracking_s: 3.0, ..DrillPlan::BALANCED };
        let dpc = 0.01;
        let run = |frame_us: u64| {
            let mut f = DrillSession::new(DrillKind::Tracking, dpc, &plan, 3, 3);
            f.click(0);
            let (mut t, mut next_frame) = (0u64, 0u64);
            while !f.is_done() {
                t += 1_000; // 1000 Hz mouse following the path with a 30 ms lag
                let tg = f.tracking_target(t.saturating_sub(30_000));
                let cam = f.camera();
                f.motion((wrap180(tg.yaw - cam.yaw) / dpc).round() as i32, (-(tg.pitch - cam.pitch) / dpc).round() as i32, t);
                if t >= next_frame {
                    f.tick(t);
                    next_frame += frame_us;
                }
            }
            f.tracking_result()
        };
        let (a, b) = (run(33_333), run(2_000));
        assert!((a.on_target - b.on_target).abs() < 0.01, "{a:?} {b:?}");
        assert!((a.rms_error_deg - b.rms_error_deg).abs() < 0.02, "{a:?} {b:?}");
    }

    #[test]
    fn validity_rules() {
        let mut r = BlockResult::default();
        let t = TrialResult {
            distance_deg: 20.0,
            radius_deg: 1.0,
            hit: false,
            time_to_target_ms: None,
            click_ms: 50.0,
            overshoot_deg: 0.0,
            first_stop_error_deg: 0.0,
            corrections: 0,
            anticipated: true,
            timed_out: false,
        };
        r.flicks = vec![t; 2];
        assert_eq!(block_validity(&r), Ok(()));
        r.flicks.push(t);
        assert_eq!(block_validity(&r), Err(InvalidReason::Anticipations));
        r.flicks = vec![TrialResult { anticipated: false, timed_out: true, ..t }; 4];
        assert_eq!(block_validity(&r), Err(InvalidReason::Timeouts));
        r.flicks.clear();
        r.input_gaps = 1;
        assert_eq!(block_validity(&r), Err(InvalidReason::InputGap));
    }

    #[test]
    fn stutter_invalidates_block() {
        let plan = DrillPlan { warmup: 0, flicks: 0, micros: 0, tracking_s: 2.0, ..DrillPlan::BALANCED };
        let mut b = Block::new(0.02, &plan, 1, 1);
        b.frame(0);
        let mut t = 0;
        let mut guard = 0;
        while !b.is_done() && guard < 100_000 {
            guard += 1;
            if !b.current().is_started() {
                b.click(t);
            }
            t += if t == 500_000 { 300_000 } else { 4_000 };
            b.frame(t);
            b.tick(t);
        }
        assert_eq!(b.validity(), Err(InvalidReason::Stutter));
    }

    fn tracking_block_with_intervals(intervals: impl Fn(u64) -> u64, rule: Option<Option<crate::polling::Jitter>>) -> Block {
        let plan = DrillPlan { warmup: 0, flicks: 0, micros: 0, tracking_s: 2.0, ..DrillPlan::BALANCED };
        let mut b = Block::new(0.02, &plan, 1, 1);
        if let Some(base) = rule {
            b.enable_polling_rule(base);
        }
        let mut t = 0;
        let mut i = 0;
        let mut guard = 0;
        while !b.is_done() && guard < 100_000 {
            guard += 1;
            if !b.current().is_started() {
                b.click(t);
            }
            t += intervals(i);
            i += 1;
            b.motion(3, if i % 2 == 0 { 2 } else { -2 }, t);
            b.frame(t);
            b.tick(t);
        }
        b
    }

    #[test]
    fn chaotic_polling_invalidates_block() {
        let even = |_: u64| 1_000u64;
        let chaotic = |i: u64| (i * 7919) % 2900 + 100;
        let baseline = crate::polling::Jitter { median_us: 1000.0, robust_cv: 0.05, n: 1000 };
        assert_eq!(tracking_block_with_intervals(even, Some(Some(baseline))).validity(), Ok(()));
        assert_eq!(tracking_block_with_intervals(chaotic, Some(Some(baseline))).validity(), Err(InvalidReason::PollingJitter));
        assert_eq!(tracking_block_with_intervals(chaotic, Some(None)).validity(), Err(InvalidReason::PollingJitter));
        // Off (no raw input timestamps): never flagged.
        assert_eq!(tracking_block_with_intervals(chaotic, None).validity(), Ok(()));
    }

    #[test]
    fn dropped_input_mid_movement_is_flagged() {
        let plan = DrillPlan { warmup: 0, flicks: 1, micros: 0, tracking_s: 1.0, ..DrillPlan::BALANCED };
        let mut s = DrillSession::new(DrillKind::Flick, 0.02, &plan, 1, 1);
        s.click(0);
        let mut t = 0;
        for _ in 0..20 {
            t += 1_000;
            s.motion(10, 0, t);
        }
        t += 300_000; // reports stop while the hand is moving fast
        for _ in 0..5 {
            s.motion(10, 0, t);
            t += 1_000;
        }
        assert_eq!(s.input_gaps(), 1);
        // A natural stop (slow down first) is not flagged.
        let mut s = DrillSession::new(DrillKind::Flick, 0.02, &plan, 1, 1);
        s.click(0);
        let mut t = 0;
        for k in 0..30 {
            t += if k < 20 { 1_000 } else { 6_000 };
            s.motion(if k < 20 { 10 } else { 1 }, 0, t);
        }
        t += 400_000;
        s.motion(10, 0, t);
        s.motion(10, 0, t + 1_000);
        assert_eq!(s.input_gaps(), 0);
    }
}

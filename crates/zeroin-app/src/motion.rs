//! Spring-driven UI motion, built on `zeroin_core::spring`.
//!
//! Every animated value is a spring keyed by an [`egui::Id`]. Springs are
//! stepped once per frame with the real frame time, and the app keeps
//! repainting (at the display's refresh rate) only while something is moving.
//! Drill rendering never uses this module.

use eframe::egui;
use std::collections::HashMap;
use zeroin_core::spring::Spring;

/// Standard settle time: fast enough to stay out of the way (≤ 250 ms).
pub const SETTLE: f64 = 0.22;
/// Slightly bouncier settle for snapping controls.
pub const SNAP_ZETA: f64 = 0.78;

#[derive(Default)]
pub struct Motion {
    springs: HashMap<egui::Id, (Spring, u64)>,
    frame: u64,
    moving: bool,
    pub reduce: bool,
}

impl Motion {
    /// Call at the start of each frame.
    pub fn begin_frame(&mut self, ctx: &egui::Context) {
        let dt = ctx.input(|i| i.unstable_dt).clamp(0.0, 0.1) as f64;
        self.frame += 1;
        self.moving = false;
        let frame = self.frame;
        self.springs.retain(|_, (_, last)| frame.saturating_sub(*last) < 600);
        for (s, _) in self.springs.values_mut() {
            if !s.is_settled(1e-3) {
                s.step(dt);
            }
        }
    }

    /// Call at the end of each frame.
    pub fn end_frame(&self, ctx: &egui::Context) {
        if self.moving {
            ctx.request_repaint();
        }
    }

    fn get(&mut self, id: egui::Id, target: f32, settle: f64, zeta: f64) -> f32 {
        let frame = self.frame;
        let reduce = self.reduce;
        let entry = self.springs.entry(id).or_insert_with(|| (Spring::with_damping(target as f64, settle, zeta), frame));
        entry.1 = frame;
        let s = &mut entry.0;
        if reduce {
            s.snap(target as f64);
        } else if (s.target - target as f64).abs() > f64::EPSILON {
            s.set_target(target as f64);
        }
        if !s.is_settled(1e-3) {
            self.moving = true;
        }
        s.value as f32
    }

    /// Critically damped value.
    pub fn value(&mut self, id: egui::Id, target: f32) -> f32 {
        self.get(id, target, SETTLE, 1.0)
    }

    /// Slightly underdamped value for snapping indicators.
    pub fn snappy(&mut self, id: egui::Id, target: f32) -> f32 {
        self.get(id, target, SETTLE, SNAP_ZETA)
    }

    /// Start a spring at `from` and send it toward `to` (for entrances).
    pub fn kick(&mut self, id: egui::Id, from: f32, to: f32) {
        let mut s = Spring::critical(from as f64, SETTLE);
        s.set_target(to as f64);
        if self.reduce {
            s.snap(to as f64);
        }
        self.springs.insert(id, (s, self.frame));
        self.moving = true;
    }

    /// 0→1 boolean blend.
    pub fn bool(&mut self, id: egui::Id, on: bool) -> f32 {
        self.value(id, if on { 1.0 } else { 0.0 })
    }
}

pub fn lerp_color(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    egui::Color32::from_rgba_unmultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

/// Stable id from any hashable key (wraps the deprecated `Id::new` in one place).
#[allow(deprecated)]
pub fn id(source: impl std::hash::Hash + std::fmt::Debug) -> egui::Id {
    egui::Id::new(source)
}

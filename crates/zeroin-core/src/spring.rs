//! Spring physics for UI motion.
//!
//! Values chase their target like a damped spring instead of following a
//! fixed-duration curve, so motion reacts instantly to new input and settles
//! softly. `settle_s` is the time to come within 5 % of a step change for a
//! critically damped spring, which keeps transitions in the ≤ 250 ms range.

/// Natural-frequency × time at which a critically damped step is within 5 %.
const CRIT_5PCT: f64 = 4.744;

/// A one-dimensional damped spring.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    /// Current value.
    pub value: f64,
    /// Current velocity (units / s).
    pub velocity: f64,
    /// Target value.
    pub target: f64,
    omega: f64,
    zeta: f64,
}

impl Spring {
    /// Critically damped spring (no overshoot) that settles in about `settle_s`.
    pub fn critical(value: f64, settle_s: f64) -> Self {
        Self::with_damping(value, settle_s, 1.0)
    }

    /// Spring with damping ratio `zeta` (< 1 bounces slightly). `settle_s` sets the
    /// natural frequency as for the critical case.
    pub fn with_damping(value: f64, settle_s: f64, zeta: f64) -> Self {
        Spring { value, velocity: 0.0, target: value, omega: CRIT_5PCT / settle_s.max(1e-3), zeta: zeta.max(0.05) }
    }

    /// Set a new target; motion continues from the current value and velocity.
    pub fn set_target(&mut self, target: f64) {
        self.target = target;
    }

    /// Jump to a value with no motion.
    pub fn snap(&mut self, v: f64) {
        self.value = v;
        self.target = v;
        self.velocity = 0.0;
    }

    /// Advance by `dt` seconds (clamped to 100 ms to survive frame hitches).
    pub fn step(&mut self, dt: f64) {
        let mut remaining = dt.clamp(0.0, 0.1);
        const H: f64 = 1.0 / 960.0;
        while remaining > 0.0 {
            let h = remaining.min(H);
            let accel = -2.0 * self.zeta * self.omega * self.velocity - self.omega * self.omega * (self.value - self.target);
            self.velocity += accel * h;
            self.value += self.velocity * h;
            remaining -= h;
        }
        if self.is_settled(1e-4) {
            self.snap(self.target);
        }
    }

    /// True when both displacement and velocity are below `eps` (relative to scale 1).
    pub fn is_settled(&self, eps: f64) -> bool {
        (self.value - self.target).abs() < eps && self.velocity.abs() < eps * 10.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simulate(mut s: Spring, secs: f64) -> (Spring, f64) {
        let mut max: f64 = s.value;
        let mut t = 0.0;
        while t < secs {
            s.step(1.0 / 144.0);
            max = max.max(s.value);
            t += 1.0 / 144.0;
        }
        (s, max)
    }

    #[test]
    fn critical_settles_without_overshoot() {
        let mut s = Spring::critical(0.0, 0.22);
        s.set_target(1.0);
        let (at_settle, _) = simulate(s, 0.22);
        assert!((at_settle.value - 1.0).abs() < 0.06, "{}", at_settle.value);
        let (_, max) = simulate(s, 1.0);
        assert!(max <= 1.0 + 1e-6);
    }

    #[test]
    fn underdamped_overshoots_a_little() {
        let mut s = Spring::with_damping(0.0, 0.22, 0.7);
        s.set_target(1.0);
        let (end, max) = simulate(s, 1.5);
        assert!(max > 1.01 && max < 1.1, "{max}");
        assert!(end.is_settled(1e-3));
    }

    #[test]
    fn hitch_is_safe() {
        let mut s = Spring::critical(0.0, 0.2);
        s.set_target(100.0);
        s.step(10.0);
        assert!(s.value.is_finite() && s.value <= 100.0);
    }
}

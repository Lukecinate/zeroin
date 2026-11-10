//! When the OS cursor is hidden and confined to the window.
//!
//! The cursor is captured only while a drill is live, the window has focus and
//! is not minimised. Everything else releases it on the same frame. The app
//! feeds the current conditions every frame and acts on the returned
//! transition; while captured it also keeps the cursor icon hidden, because
//! the UI toolkit would otherwise show it again when the hovered icon changes.

/// Conditions sampled once per frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CaptureInputs {
    /// A drill is running (not the round intro or the pause screen).
    pub drill_active: bool,
    /// The window has keyboard focus.
    pub focused: bool,
    /// The window is minimised.
    pub minimized: bool,
}

impl CaptureInputs {
    /// Whether these conditions call for a captured cursor.
    pub fn wants_capture(&self) -> bool {
        self.drill_active && self.focused && !self.minimized
    }
}

/// A change the app must apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureAction {
    /// Hide the cursor and confine it to the window.
    Capture,
    /// Show the cursor and release any confinement.
    Release,
}

/// Capture state. Starts released.
#[derive(Clone, Copy, Debug, Default)]
pub struct CursorCapture {
    captured: bool,
}

impl CursorCapture {
    /// True while the cursor should be hidden and confined.
    pub fn is_captured(&self) -> bool {
        self.captured
    }

    /// Feed this frame's conditions; returns a transition when one is needed.
    pub fn update(&mut self, inputs: CaptureInputs) -> Option<CaptureAction> {
        let want = inputs.wants_capture();
        if want == self.captured {
            return None;
        }
        self.captured = want;
        Some(if want { CaptureAction::Capture } else { CaptureAction::Release })
    }

    /// Unconditional release (exit, panic, test end). Returns `Release` only if
    /// the cursor was captured, so callers can release native state once.
    pub fn force_release(&mut self) -> Option<CaptureAction> {
        if self.captured {
            self.captured = false;
            Some(CaptureAction::Release)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DRILL: CaptureInputs = CaptureInputs { drill_active: true, focused: true, minimized: false };
    const IDLE: CaptureInputs = CaptureInputs { drill_active: false, focused: true, minimized: false };

    #[test]
    fn starts_released_and_captures_once() {
        let mut c = CursorCapture::default();
        assert!(!c.is_captured());
        assert_eq!(c.update(IDLE), None);
        assert_eq!(c.update(DRILL), Some(CaptureAction::Capture));
        assert_eq!(c.update(DRILL), None, "no repeated capture while it stays live");
        assert!(c.is_captured());
    }

    #[test]
    fn releases_on_end_focus_loss_and_minimise() {
        for lost in [
            CaptureInputs { drill_active: false, ..DRILL },
            CaptureInputs { focused: false, ..DRILL },
            CaptureInputs { minimized: true, ..DRILL },
        ] {
            let mut c = CursorCapture::default();
            c.update(DRILL);
            assert_eq!(c.update(lost), Some(CaptureAction::Release), "{lost:?}");
            assert!(!c.is_captured());
            assert_eq!(c.update(lost), None);
        }
    }

    #[test]
    fn recaptures_when_focus_returns_to_a_live_drill() {
        let mut c = CursorCapture::default();
        c.update(DRILL);
        c.update(CaptureInputs { focused: false, ..DRILL });
        assert_eq!(c.update(DRILL), Some(CaptureAction::Capture));
    }

    #[test]
    fn force_release_is_idempotent() {
        let mut c = CursorCapture::default();
        assert_eq!(c.force_release(), None);
        c.update(DRILL);
        assert_eq!(c.force_release(), Some(CaptureAction::Release));
        assert_eq!(c.force_release(), None);
        assert!(!c.is_captured());
    }

    #[test]
    fn every_capture_is_matched_by_one_release() {
        // Balanced hide/show over an arbitrary sequence, ending with exit.
        let seq = [
            IDLE,
            DRILL,
            DRILL,
            CaptureInputs { focused: false, ..DRILL },
            DRILL,
            CaptureInputs { minimized: true, ..DRILL },
            DRILL,
            IDLE,
            DRILL,
        ];
        let mut c = CursorCapture::default();
        let mut depth = 0i32;
        for s in seq {
            match c.update(s) {
                Some(CaptureAction::Capture) => depth += 1,
                Some(CaptureAction::Release) => depth -= 1,
                None => {}
            }
            assert!((0..=1).contains(&depth));
        }
        if c.force_release().is_some() {
            depth -= 1;
        }
        assert_eq!(depth, 0);
    }
}

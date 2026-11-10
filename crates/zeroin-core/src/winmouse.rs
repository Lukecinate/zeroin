//! Windows pointer settings: the neutral target ZeroIn can apply, and a
//! backup so the change can be undone.
//!
//! Only the per-user pointer speed and "Enhance pointer precision" are
//! touched. In-game sensitivity is never written; games keep it in their own
//! configuration.

use serde::{Deserialize, Serialize};

/// The values behind Windows' mouse settings (`SPI_GETMOUSE` and
/// `SPI_GETMOUSESPEED`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MouseParams {
    /// First acceleration threshold (Windows default 6 with precision on).
    pub threshold1: i32,
    /// Second acceleration threshold (Windows default 10 with precision on).
    pub threshold2: i32,
    /// Acceleration level; non-zero means "Enhance pointer precision" is on.
    pub acceleration: i32,
    /// Pointer speed slider, 1–20 (10 is the sixth notch, 1:1).
    pub speed: u32,
}

impl MouseParams {
    /// Pointer speed 10 (no scaling) with "Enhance pointer precision" off.
    pub const NEUTRAL: MouseParams = MouseParams { threshold1: 0, threshold2: 0, acceleration: 0, speed: 10 };

    /// "Enhance pointer precision" is on.
    pub fn enhance_pointer_precision(&self) -> bool {
        self.acceleration != 0
    }

    /// Raw mouse movement reaches the desktop unscaled.
    pub fn is_neutral(&self) -> bool {
        !self.enhance_pointer_precision() && self.speed == 10
    }

    /// Clamp to values Windows accepts.
    pub fn sanitized(self) -> MouseParams {
        MouseParams {
            threshold1: self.threshold1.clamp(0, 50),
            threshold2: self.threshold2.clamp(0, 50),
            acceleration: self.acceleration.clamp(0, 2),
            speed: self.speed.clamp(1, 20),
        }
    }
}

/// Saved values from before ZeroIn changed them (`mouse-backup.json`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MouseBackup {
    /// Settings to restore; `None` when there is nothing to undo.
    pub previous: Option<MouseParams>,
    /// When they were saved (Unix seconds).
    pub saved_unix: u64,
}

impl MouseBackup {
    /// There is something to restore.
    pub fn can_restore(&self) -> bool {
        self.previous.is_some()
    }
}

/// One line of the confirmation: setting name, current value, new value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// Setting name as Windows shows it.
    pub label: &'static str,
    /// Current value.
    pub from: String,
    /// Value after the change.
    pub to: String,
}

fn on_off(b: bool) -> String {
    if b { "On" } else { "Off" }.to_string()
}

fn speed_text(s: u32) -> String {
    if s == 10 {
        "10 (6 of 11, default)".to_string()
    } else {
        format!("{s}")
    }
}

/// The setting lines that differ between `from` and `to`.
pub fn changes(from: MouseParams, to: MouseParams) -> Vec<Change> {
    let mut out = Vec::new();
    if from.enhance_pointer_precision() != to.enhance_pointer_precision() {
        out.push(Change {
            label: "Enhance pointer precision",
            from: on_off(from.enhance_pointer_precision()),
            to: on_off(to.enhance_pointer_precision()),
        });
    }
    if from.speed != to.speed {
        out.push(Change { label: "Pointer speed", from: speed_text(from.speed), to: speed_text(to.speed) });
    }
    out
}

/// The backup to keep when applying the neutral values to `current`.
///
/// If `current` is already neutral, an existing backup is kept so "Restore
/// previous" still returns to the user's original settings. Otherwise the
/// current values become the backup.
pub fn backup_for_apply(existing: MouseBackup, current: MouseParams, now_unix: u64) -> MouseBackup {
    if current.is_neutral() {
        existing
    } else {
        MouseBackup { previous: Some(current), saved_unix: now_unix }
    }
}

/// Outcome of writing settings and reading them back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verify {
    /// Windows reports the expected values.
    Ok,
    /// Windows reports something else (for example, a policy overrides it).
    Mismatch {
        /// What Windows reports instead.
        actual: MouseParams,
    },
    /// The settings could not be read back.
    Unreadable,
}

/// Compare what was written with what Windows reports.
pub fn verify(expected: MouseParams, actual: Option<MouseParams>) -> Verify {
    match actual {
        None => Verify::Unreadable,
        Some(a) if a.speed == expected.speed && a.enhance_pointer_precision() == expected.enhance_pointer_precision() => Verify::Ok,
        Some(a) => Verify::Mismatch { actual: a },
    }
}

/// The backup to keep after a restore attempt: cleared only when the restore
/// is confirmed, so a failed restore can be retried.
pub fn backup_after_restore(backup: MouseBackup, result: Verify) -> MouseBackup {
    if result == Verify::Ok {
        MouseBackup::default()
    } else {
        backup
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIN_DEFAULT: MouseParams = MouseParams { threshold1: 6, threshold2: 10, acceleration: 1, speed: 10 };

    #[test]
    fn neutral_detection() {
        assert!(MouseParams::NEUTRAL.is_neutral());
        assert!(!WIN_DEFAULT.is_neutral());
        assert!(!MouseParams { speed: 12, ..MouseParams::NEUTRAL }.is_neutral());
    }

    #[test]
    fn change_lines() {
        let c = changes(MouseParams { speed: 14, ..WIN_DEFAULT }, MouseParams::NEUTRAL);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0], Change { label: "Enhance pointer precision", from: "On".into(), to: "Off".into() });
        assert_eq!(c[1].from, "14");
        assert!(c[1].to.starts_with("10"));
        assert!(changes(MouseParams::NEUTRAL, MouseParams::NEUTRAL).is_empty());
    }

    #[test]
    fn apply_saves_original_and_keeps_it_on_reapply() {
        let b = backup_for_apply(MouseBackup::default(), WIN_DEFAULT, 100);
        assert_eq!(b.previous, Some(WIN_DEFAULT));
        assert_eq!(b.saved_unix, 100);
        // Applying again once already neutral must not overwrite the original.
        let b2 = backup_for_apply(b, MouseParams::NEUTRAL, 200);
        assert_eq!(b2, b);
        // If the user changed Windows settings again, the new values are what to restore.
        let user = MouseParams { speed: 8, ..MouseParams::NEUTRAL };
        let b3 = backup_for_apply(b2, user, 300);
        assert_eq!(b3.previous, Some(user));
    }

    #[test]
    fn verify_and_restore_lifecycle() {
        assert_eq!(verify(MouseParams::NEUTRAL, Some(MouseParams::NEUTRAL)), Verify::Ok);
        // Thresholds may differ; only the user-visible settings matter.
        assert_eq!(verify(MouseParams::NEUTRAL, Some(MouseParams { threshold1: 6, ..MouseParams::NEUTRAL })), Verify::Ok);
        assert_eq!(verify(MouseParams::NEUTRAL, Some(WIN_DEFAULT)), Verify::Mismatch { actual: WIN_DEFAULT });
        assert_eq!(verify(MouseParams::NEUTRAL, None), Verify::Unreadable);

        let b = backup_for_apply(MouseBackup::default(), WIN_DEFAULT, 1);
        assert_eq!(backup_after_restore(b, Verify::Unreadable), b, "failed restore keeps the backup");
        assert!(!backup_after_restore(b, Verify::Ok).can_restore());
    }

    #[test]
    fn sanitize_and_json_round_trip() {
        let p = MouseParams { threshold1: -3, threshold2: 99, acceleration: 7, speed: 0 }.sanitized();
        assert_eq!(p, MouseParams { threshold1: 0, threshold2: 50, acceleration: 2, speed: 1 });
        let b = MouseBackup { previous: Some(WIN_DEFAULT), saved_unix: 5 };
        let s = serde_json::to_string(&b).unwrap();
        assert_eq!(serde_json::from_str::<MouseBackup>(&s).unwrap(), b);
    }
}

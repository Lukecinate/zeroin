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

// ---------------------------------------------------------------------------
// Temporary neutral settings for the length of a test

/// Pointer settings in effect while a test was played, stored with the result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PointerDuringTest {
    /// Windows was already at pointer speed 10 with precision off.
    AlreadyNeutral,
    /// ZeroIn set neutral values for the test and restored them afterwards.
    TemporarilyNeutral,
    /// The user's own settings were left as they were.
    LeftAsIs,
}

/// Written to disk before neutral values are applied for a test
/// (`mouse-temp.json`), so an interrupted run can be undone on next launch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TempJournal {
    /// The user's settings to put back, thresholds included.
    pub previous: MouseParams,
    /// What ZeroIn set.
    pub applied: MouseParams,
    /// When the journal was written (Unix seconds).
    pub saved_unix: u64,
}

/// Reads and writes pointer settings for the current sign-in session only.
pub trait PointerBackend {
    /// Current settings, or `None` when they cannot be read.
    fn read(&mut self) -> Option<MouseParams>;
    /// Set values for this session without saving them to the user profile.
    fn write_session(&mut self, p: MouseParams) -> Result<(), String>;
}

/// Storage for the [`TempJournal`].
pub trait JournalStore {
    /// Save, replacing any existing journal.
    fn save(&mut self, j: &TempJournal) -> Result<(), String>;
    /// The saved journal, if one exists and is readable.
    fn load(&mut self) -> Option<TempJournal>;
    /// Delete the journal; deleting a missing journal is not an error.
    fn remove(&mut self) -> Result<(), String>;
}

/// Outcome of [`begin_temp`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BeginTemp {
    /// Nothing to change.
    AlreadyNeutral,
    /// Neutral values are active; pass the journal to [`end_temp`].
    Applied(TempJournal),
    /// Nothing is left changed; the reason is shown to the user.
    Failed(String),
}

/// Apply neutral settings for a test: read, journal, write, verify. Any
/// failure puts the previous values back and removes the journal.
pub fn begin_temp(be: &mut dyn PointerBackend, js: &mut dyn JournalStore, now_unix: u64) -> BeginTemp {
    let Some(cur) = be.read() else { return BeginTemp::Failed("Windows pointer settings could not be read.".into()) };
    if cur.is_neutral() {
        return BeginTemp::AlreadyNeutral;
    }
    let j = TempJournal { previous: cur, applied: MouseParams::NEUTRAL, saved_unix: now_unix };
    if let Err(e) = js.save(&j) {
        return BeginTemp::Failed(format!("Nothing was changed: the restore copy could not be saved ({e})."));
    }
    let wrote = be.write_session(MouseParams::NEUTRAL);
    let check = verify(MouseParams::NEUTRAL, be.read());
    if wrote.is_ok() && check == Verify::Ok {
        return BeginTemp::Applied(j);
    }
    // Undo a partial write; keep the journal if even that fails.
    let undone = be.write_session(cur).is_ok() && verify(cur, be.read()) == Verify::Ok;
    if undone {
        let _ = js.remove();
    }
    BeginTemp::Failed(match (wrote, check) {
        (Err(e), _) => format!("Windows did not accept the change ({e})."),
        (Ok(()), Verify::Mismatch { .. }) => "Windows reports different values; a policy or mouse software may override them.".into(),
        _ => "The change could not be confirmed.".into(),
    })
}

/// Put the journalled settings back. The journal is removed only once
/// Windows reports the previous values, so a failed restore is retried on
/// next launch.
pub fn end_temp(be: &mut dyn PointerBackend, js: &mut dyn JournalStore, j: &TempJournal) -> Result<(), String> {
    be.write_session(j.previous)?;
    match verify(j.previous, be.read()) {
        Verify::Ok => js.remove(),
        Verify::Mismatch { .. } => Err("Windows reports different values after restoring.".into()),
        Verify::Unreadable => Err("The restored values could not be read back.".into()),
    }
}

/// Outcome of [`recover_temp`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Recovery {
    /// No journal: the last run ended normally.
    Nothing,
    /// The previous settings were put back.
    Restored(MouseParams),
    /// Windows no longer shows ZeroIn's values (signed out, restarted, or
    /// changed by the user), so nothing was written; the journal is removed.
    NotNeeded,
    /// Restoring failed; the journal is kept for the next launch.
    Failed(String),
}

/// Start-up check after an interrupted test.
pub fn recover_temp(be: &mut dyn PointerBackend, js: &mut dyn JournalStore) -> Recovery {
    let Some(j) = js.load() else { return Recovery::Nothing };
    let Some(cur) = be.read() else { return Recovery::Failed("Windows pointer settings could not be read.".into()) };
    if verify(j.applied, Some(cur)) != Verify::Ok {
        let _ = js.remove();
        return Recovery::NotNeeded;
    }
    match end_temp(be, js, &j) {
        Ok(()) => Recovery::Restored(j.previous),
        Err(e) => Recovery::Failed(e),
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

    #[derive(Default)]
    struct MockPc {
        cur: Option<MouseParams>,
        /// Values Windows silently keeps instead of what is written (policy).
        forced: Option<MouseParams>,
        fail_writes: bool,
        writes: Vec<MouseParams>,
    }

    impl PointerBackend for MockPc {
        fn read(&mut self) -> Option<MouseParams> {
            self.cur
        }
        fn write_session(&mut self, p: MouseParams) -> Result<(), String> {
            if self.fail_writes {
                return Err("denied".into());
            }
            self.writes.push(p);
            self.cur = Some(self.forced.unwrap_or(p));
            Ok(())
        }
    }

    #[derive(Default)]
    struct MockJournal {
        j: Option<TempJournal>,
        fail_save: bool,
    }

    impl JournalStore for MockJournal {
        fn save(&mut self, j: &TempJournal) -> Result<(), String> {
            if self.fail_save {
                return Err("disk full".into());
            }
            self.j = Some(*j);
            Ok(())
        }
        fn load(&mut self) -> Option<TempJournal> {
            self.j
        }
        fn remove(&mut self) -> Result<(), String> {
            self.j = None;
            Ok(())
        }
    }

    const USER: MouseParams = MouseParams { threshold1: 4, threshold2: 12, acceleration: 1, speed: 13 };

    #[test]
    fn temp_apply_and_exact_restore() {
        let mut pc = MockPc { cur: Some(USER), ..Default::default() };
        let mut js = MockJournal::default();
        let BeginTemp::Applied(j) = begin_temp(&mut pc, &mut js, 7) else { panic!("not applied") };
        assert_eq!(pc.cur, Some(MouseParams::NEUTRAL));
        assert_eq!(js.j, Some(j), "journal written");
        assert_eq!(j.previous, USER);
        end_temp(&mut pc, &mut js, &j).unwrap();
        assert_eq!(pc.cur, Some(USER), "thresholds restored exactly");
        assert!(js.j.is_none(), "journal removed after restore");
    }

    #[test]
    fn temp_never_writes_without_a_journal() {
        let mut pc = MockPc { cur: Some(USER), ..Default::default() };
        let mut js = MockJournal { fail_save: true, ..Default::default() };
        assert!(matches!(begin_temp(&mut pc, &mut js, 0), BeginTemp::Failed(_)));
        assert!(pc.writes.is_empty());
        assert_eq!(pc.cur, Some(USER));
    }

    #[test]
    fn temp_skips_neutral_and_unreadable() {
        let mut js = MockJournal::default();
        let mut pc = MockPc { cur: Some(MouseParams::NEUTRAL), ..Default::default() };
        assert_eq!(begin_temp(&mut pc, &mut js, 0), BeginTemp::AlreadyNeutral);
        let mut pc = MockPc::default();
        assert!(matches!(begin_temp(&mut pc, &mut js, 0), BeginTemp::Failed(_)));
        assert!(js.j.is_none() && pc.writes.is_empty());
    }

    #[test]
    fn temp_overridden_by_policy_is_undone() {
        let mut pc = MockPc { cur: Some(USER), forced: Some(USER), ..Default::default() };
        let mut js = MockJournal::default();
        assert!(matches!(begin_temp(&mut pc, &mut js, 0), BeginTemp::Failed(_)));
        assert_eq!(pc.cur, Some(USER));
        assert!(js.j.is_none());
        let mut pc = MockPc { cur: Some(USER), fail_writes: true, ..Default::default() };
        assert!(matches!(begin_temp(&mut pc, &mut js, 0), BeginTemp::Failed(_)));
        assert_eq!(js.j.map(|j| j.previous), Some(USER), "journal kept when the undo itself fails");
    }

    #[test]
    fn failed_restore_keeps_journal() {
        let mut pc = MockPc { cur: Some(USER), ..Default::default() };
        let mut js = MockJournal::default();
        let BeginTemp::Applied(j) = begin_temp(&mut pc, &mut js, 0) else { panic!() };
        pc.fail_writes = true;
        assert!(end_temp(&mut pc, &mut js, &j).is_err());
        assert!(js.j.is_some());
    }

    #[test]
    fn crash_recovery() {
        // Crash mid-test: neutral values still active, journal on disk.
        let mut pc = MockPc { cur: Some(USER), ..Default::default() };
        let mut js = MockJournal::default();
        let BeginTemp::Applied(_) = begin_temp(&mut pc, &mut js, 0) else { panic!() };
        assert_eq!(recover_temp(&mut pc, &mut js), Recovery::Restored(USER));
        assert_eq!(pc.cur, Some(USER));
        assert!(js.j.is_none());
        assert_eq!(recover_temp(&mut pc, &mut js), Recovery::Nothing);

        // Restarted since: Windows already shows the user's values again.
        let _ = begin_temp(&mut pc, &mut js, 0);
        pc.cur = Some(USER);
        let n = pc.writes.len();
        assert_eq!(recover_temp(&mut pc, &mut js), Recovery::NotNeeded);
        assert_eq!(pc.writes.len(), n, "nothing written");
        assert!(js.j.is_none());

        // Unreadable now: keep the journal for later.
        let _ = begin_temp(&mut pc, &mut js, 0);
        pc.cur = None;
        assert!(matches!(recover_temp(&mut pc, &mut js), Recovery::Failed(_)));
        assert!(js.j.is_some());
    }

    #[test]
    fn journal_and_label_json() {
        let j = TempJournal { previous: USER, applied: MouseParams::NEUTRAL, saved_unix: 9 };
        let s = serde_json::to_string(&j).unwrap();
        assert_eq!(serde_json::from_str::<TempJournal>(&s).unwrap(), j);
        let p = serde_json::to_string(&PointerDuringTest::TemporarilyNeutral).unwrap();
        assert_eq!(serde_json::from_str::<PointerDuringTest>(&p).unwrap(), PointerDuringTest::TemporarilyNeutral);
    }
}

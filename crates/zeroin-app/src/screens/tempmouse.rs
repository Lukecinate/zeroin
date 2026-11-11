//! Temporary neutral Windows pointer settings while drills run.
//!
//! Asked once before a test (or remembered). The user's values are journalled
//! to `mouse-temp.json` before anything changes, written for the current
//! sign-in session only, and put back when a drill stops for any reason:
//! block end, Esc, focus loss, leaving the test, closing the app, a panic,
//! or Windows signing out. A journal left by a crash is replayed on next
//! launch. The read/write/verify logic is shared with "Apply to Windows"
//! (`zeroin_core::winmouse`).

use crate::app::ZeroInApp;
use crate::platform;
use crate::theme::*;
use crate::widgets::{self, Tone};
use eframe::egui::{self, Key};
use std::sync::{Mutex, TryLockError};
use zeroin_core::history::{load_json, save_json};
use zeroin_core::winmouse::{
    begin_temp, changes, end_temp, recover_temp, BeginTemp, JournalStore, MouseParams, PointerBackend, PointerDuringTest, Recovery,
    TempJournal,
};

/// The question shown before a test.
pub const PROMPT: &str =
    "Temporarily set Windows to Enhance pointer precision OFF and pointer speed 10 for this test? Your settings are restored when the test ends.";

struct ThisPc;

impl PointerBackend for ThisPc {
    fn read(&mut self) -> Option<MouseParams> {
        platform::mouse_params()
    }
    fn write_session(&mut self, p: MouseParams) -> Result<(), String> {
        platform::set_mouse_params_session(p)
    }
}

struct JournalFile;

impl JournalStore for JournalFile {
    fn save(&mut self, j: &TempJournal) -> Result<(), String> {
        save_json(&crate::paths::mouse_temp_file(), j).map_err(|e| e.to_string())
    }
    fn load(&mut self) -> Option<TempJournal> {
        load_json::<Option<TempJournal>>(&crate::paths::mouse_temp_file()).ok().flatten()
    }
    fn remove(&mut self) -> Result<(), String> {
        match std::fs::remove_file(crate::paths::mouse_temp_file()) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        }
    }
}

/// The active change, reachable from the panic hook and the sign-out thread.
static ACTIVE: Mutex<Option<TempJournal>> = Mutex::new(None);

/// Put the user's settings back if a temporary change is active. Returns
/// `None` when nothing was active. Safe to call from any thread.
pub fn restore_now() -> Option<Result<(), String>> {
    let mut guard = match ACTIVE.try_lock() {
        Ok(g) => g,
        Err(TryLockError::Poisoned(p)) => p.into_inner(),
        Err(TryLockError::WouldBlock) => return None,
    };
    let j = guard.take()?;
    let r = end_temp(&mut ThisPc, &mut JournalFile, &j);
    // On failure the journal stays on disk and is replayed on next launch.
    Some(r)
}

/// Sign-out / shutdown handler.
fn on_session_end() {
    let _ = restore_now();
}

/// Holds the temporary change; dropping it restores the user's settings.
pub struct TempMouseGuard(());

impl TempMouseGuard {
    fn apply() -> Result<Option<TempMouseGuard>, String> {
        let mut active = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
        if active.is_some() {
            return Ok(Some(TempMouseGuard(())));
        }
        match begin_temp(&mut ThisPc, &mut JournalFile, crate::screens::test::unix_now()) {
            BeginTemp::Applied(j) => {
                *active = Some(j);
                Ok(Some(TempMouseGuard(())))
            }
            BeginTemp::AlreadyNeutral => Ok(None),
            BeginTemp::Failed(e) => Err(e),
        }
    }

    /// Restore now and report the outcome.
    fn release(self) -> Result<(), String> {
        std::mem::forget(self);
        restore_now().unwrap_or(Ok(()))
    }
}

impl Drop for TempMouseGuard {
    fn drop(&mut self) {
        let _ = restore_now();
    }
}

#[derive(Default)]
pub struct TempMouseUi {
    guard: Option<TempMouseGuard>,
    /// The answer for the current test.
    pub this_test: Option<bool>,
    /// Applying failed during this test; do not retry every frame.
    failed: bool,
    /// The question is open.
    pub prompt: bool,
    remember: bool,
    /// Start the block once the question is answered.
    pub start_after_prompt: bool,
    /// Stored with the result.
    pub during_test: Option<PointerDuringTest>,
}

impl TempMouseUi {
    /// Neutral values are active right now.
    pub fn is_active(&self) -> bool {
        self.guard.is_some()
    }

    /// Forget the answer and outcome for a new or resumed test.
    pub fn reset_for_test(&mut self) {
        self.release_quietly();
        self.this_test = None;
        self.failed = false;
        self.during_test = None;
        self.prompt = false;
        self.start_after_prompt = false;
    }

    fn release_quietly(&mut self) {
        if let Some(g) = self.guard.take() {
            let _ = g.release();
        }
    }
}

impl ZeroInApp {
    /// Start-up: replay a journal left by an interrupted test, and watch for
    /// Windows signing out.
    pub fn temp_mouse_startup(&mut self) {
        platform::watch_session_end(on_session_end);
        match recover_temp(&mut ThisPc, &mut JournalFile) {
            Recovery::Restored(_) => {
                self.refresh_system_info();
                self.toast("Your Windows pointer settings from an interrupted test were restored.");
            }
            Recovery::Failed(e) => self.toast(format!("Could not restore Windows pointer settings from the last test: {e}")),
            Recovery::Nothing | Recovery::NotNeeded => {}
        }
    }

    /// Settle the answer for this test before its first block. Returns false
    /// when the question has to be asked first.
    pub fn temp_mouse_ready_to_start(&mut self) -> bool {
        if self.temp_mouse.this_test.is_some() {
            return true;
        }
        let cur = platform::mouse_params();
        match (cur, self.settings.temp_mouse) {
            (None, _) => {
                self.temp_mouse.this_test = Some(false);
                true
            }
            (Some(c), _) if c.is_neutral() => {
                self.temp_mouse.this_test = Some(false);
                self.temp_mouse.during_test = Some(PointerDuringTest::AlreadyNeutral);
                true
            }
            (Some(_), Some(choice)) => {
                self.set_temp_choice(choice);
                true
            }
            (Some(_), None) => {
                self.temp_mouse.prompt = true;
                self.temp_mouse.remember = false;
                false
            }
        }
    }

    fn set_temp_choice(&mut self, yes: bool) {
        self.temp_mouse.this_test = Some(yes);
        if !yes {
            self.temp_mouse.during_test = Some(PointerDuringTest::LeftAsIs);
        }
    }

    fn answer_temp_prompt(&mut self, yes: bool) {
        self.temp_mouse.prompt = false;
        self.set_temp_choice(yes);
        if self.temp_mouse.remember {
            self.settings.temp_mouse = Some(yes);
            self.save_settings();
        }
        self.temp_mouse.start_after_prompt = true;
    }

    /// Every frame: neutral values only while a drill is live and focused.
    pub fn sync_temp_mouse(&mut self, ctx: &egui::Context) {
        let focused = ctx.input(|i| i.focused);
        let want = self.in_drill() && focused && self.temp_mouse.this_test == Some(true) && !self.temp_mouse.failed;
        if want && self.temp_mouse.guard.is_none() {
            match TempMouseGuard::apply() {
                Ok(Some(g)) => {
                    self.temp_mouse.guard = Some(g);
                    self.temp_mouse.during_test = Some(PointerDuringTest::TemporarilyNeutral);
                }
                Ok(None) => {
                    if self.temp_mouse.during_test.is_none() {
                        self.temp_mouse.during_test = Some(PointerDuringTest::AlreadyNeutral);
                    }
                }
                Err(e) => {
                    self.temp_mouse.failed = true;
                    if self.temp_mouse.during_test != Some(PointerDuringTest::TemporarilyNeutral) {
                        self.temp_mouse.during_test = Some(PointerDuringTest::LeftAsIs);
                    }
                    self.toast(format!("Windows settings were left unchanged: {e}"));
                }
            }
        } else if !want {
            if let Some(g) = self.temp_mouse.guard.take() {
                if let Err(e) = g.release() {
                    self.toast(format!("Could not restore Windows pointer settings ({e}). ZeroIn retries at next start."));
                }
            }
        }
    }

    /// App close.
    pub fn temp_mouse_exit(&mut self) {
        self.temp_mouse.release_quietly();
        let _ = restore_now();
    }

    /// Keys while the question is open: Enter yes, N no, Esc closes it.
    pub fn temp_mouse_keys(&mut self, ctx: &egui::Context) {
        if !self.temp_mouse.prompt {
            return;
        }
        let (enter, no, esc) = ctx.input_mut(|i| {
            let e = i.consume_key(egui::Modifiers::NONE, Key::Enter);
            let n = i.consume_key(egui::Modifiers::NONE, Key::N);
            let x = i.consume_key(egui::Modifiers::NONE, Key::Escape);
            let r = i.consume_key(egui::Modifiers::NONE, Key::R);
            if r {
                self.temp_mouse.remember = !self.temp_mouse.remember;
            }
            (e, n, x)
        });
        if esc {
            self.temp_mouse.prompt = false;
        } else if enter {
            self.answer_temp_prompt(true);
        } else if no {
            self.answer_temp_prompt(false);
        }
    }

    /// The question before the first block.
    pub fn temp_mouse_dialog(&mut self, ctx: &egui::Context) {
        if !self.temp_mouse.prompt {
            return;
        }
        let cur = platform::mouse_params();
        let mut answer = None;
        let frame = egui::Frame::new()
            .fill(SURFACE)
            .stroke(egui::Stroke::new(1.0, BORDER))
            .corner_radius(radius(R_CARD))
            .inner_margin(egui::Margin::same(24));
        let resp = egui::Modal::new(crate::motion::id("temp_mouse_modal")).frame(frame).show(ctx, |ui| {
            ui.set_width(480.0);
            // Short windows: the dialog scrolls instead of running off screen.
            // The buttons stay below the scrolling part, always in view.
            let max_h = (ui.ctx().content_rect().height() - 96.0 - 48.0 - 90.0).max(120.0);
            ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
            egui::ScrollArea::vertical().max_height(max_h).min_scrolled_height(max_h).show(ui, |ui| {
                ui.label(egui::RichText::new("Neutral Windows settings for this test?").font(bold(20.0)).color(TEXT));
                ui.add_space(6.0);
                widgets::dim(ui, PROMPT);
                ui.add_space(12.0);
                if let Some(c) = cur {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    egui::Grid::new(crate::motion::id("temp_mouse_grid")).num_columns(4).spacing([14.0, 8.0]).show(ui, |ui| {
                        for ch in changes(c, MouseParams::NEUTRAL) {
                            ui.label(egui::RichText::new(ch.label).font(semibold(14.0)).color(TEXT));
                            ui.label(egui::RichText::new(ch.from).color(TEXT_DIM));
                            ui.label(egui::RichText::new("→").color(TEXT_DIM));
                            ui.label(egui::RichText::new(ch.to).font(semibold(14.0)).color(CORAL));
                            ui.end_row();
                        }
                    });
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    ui.add_space(10.0);
                }
                widgets::dim(
                    ui,
                    "Only while drills run; Esc, switching away or closing ZeroIn puts your values back. The change is not saved to your Windows profile, so signing out also undoes it. The result does not depend on this: ZeroIn reads raw mouse counts either way.",
                );
                ui.add_space(10.0);
                ui.checkbox(&mut self.temp_mouse.remember, "Remember my choice (change it in Settings)");
                ui.add_space(14.0);
            });
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                if widgets::primary_button(ui, &mut self.motion, "Yes, for this test", true).clicked() {
                    answer = Some(true);
                }
                ui.add_space(8.0);
                if widgets::secondary_button(ui, &mut self.motion, "No, keep mine", None).clicked() {
                    answer = Some(false);
                }
            });
            ui.add_space(8.0);
            widgets::key_hint(ui, &[("Enter", "yes"), ("N", "no"), ("R", "remember"), ("Esc", "back")]);
        });
        if let Some(a) = answer {
            self.answer_temp_prompt(a);
        } else if resp.should_close() {
            self.temp_mouse.prompt = false;
        }
    }

    /// Settings row: ask / always / never.
    pub fn temp_mouse_setting(&mut self, ui: &mut egui::Ui) {
        widgets::setting_row(ui, "Neutral Windows settings during tests", "Precision off, speed 10 while drills run.", |ui| {
            let labels = vec!["Ask".to_string(), "Always".to_string(), "Never".to_string()];
            let sel = match self.settings.temp_mouse {
                None => 0,
                Some(true) => 1,
                Some(false) => 2,
            };
            if let Some(i) = widgets::chip_row_from(ui, &mut self.motion, crate::motion::id("temp_mouse_chips"), &labels, Some(sel), 4) {
                self.settings.temp_mouse = [None, Some(true), Some(false)][i];
                self.save_settings();
            }
        });
    }

    /// Result note about the pointer settings used for the test.
    pub fn temp_mouse_result_note(&self, ui: &mut egui::Ui, s: &zeroin_core::history::Session) {
        match s.pointer_during_test {
            Some(PointerDuringTest::TemporarilyNeutral) => {
                widgets::dim(ui, "Windows pointer settings were neutral during the drills and restored afterwards.");
            }
            Some(PointerDuringTest::LeftAsIs) => {
                let yours = s
                    .windows_mouse
                    .map(|m| {
                        format!(
                            " (Enhance pointer precision {}, pointer speed {})",
                            if m.enhance_pointer_precision { "on" } else { "off" },
                            m.pointer_speed
                        )
                    })
                    .unwrap_or_default();
                widgets::notice(
                    ui,
                    Tone::Info,
                    "Tested with your own Windows settings",
                    &format!(
                        "Your Windows pointer settings were left as they are{yours}. ZeroIn measures raw mouse counts, which these settings do not scale, so the sensitivity and cm/360 above need no conversion. They do change the desktop cursor and games that read it instead of raw input."
                    ),
                );
            }
            Some(PointerDuringTest::AlreadyNeutral) | None => {}
        }
    }
}

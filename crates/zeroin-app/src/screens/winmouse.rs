//! "Apply to Windows": set neutral pointer settings with a confirmation, keep
//! a backup in `%APPDATA%\ZeroIn`, and restore it on request.

use crate::app::ZeroInApp;
use crate::platform;
use crate::theme::*;
use crate::widgets::{self, Tone};
use eframe::egui::{self, Key};
use zeroin_core::history::{load_json, save_json};
use zeroin_core::winmouse::{backup_after_restore, backup_for_apply, changes, verify, MouseBackup, MouseParams, Verify};

/// A change waiting for the user's confirmation.
#[derive(Clone, Copy, Debug)]
pub enum Pending {
    Apply { from: MouseParams },
    Restore { from: MouseParams, to: MouseParams },
}

/// Result of the last apply or restore, shown where it was started.
#[derive(Clone, Debug)]
pub struct Outcome {
    pub ok: bool,
    pub title: String,
    pub body: String,
}

#[derive(Default)]
pub struct WinMouseUi {
    pub backup: MouseBackup,
    pub pending: Option<Pending>,
    pub outcome: Option<Outcome>,
}

impl WinMouseUi {
    pub fn load() -> Self {
        WinMouseUi { backup: load_json(&crate::paths::mouse_backup_file()).unwrap_or_default(), ..Default::default() }
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn describe(p: MouseParams) -> String {
    format!("Enhance pointer precision {}, pointer speed {}", if p.enhance_pointer_precision() { "on" } else { "off" }, p.speed)
}

impl ZeroInApp {
    /// Open the confirmation for neutral settings.
    pub fn request_apply_neutral(&mut self) {
        match platform::mouse_params() {
            Some(cur) if cur.is_neutral() => {
                self.win_mouse.outcome = Some(Outcome {
                    ok: true,
                    title: "Already neutral".into(),
                    body: "Enhance pointer precision is off and pointer speed is 10. Nothing to change.".into(),
                });
            }
            Some(cur) => self.win_mouse.pending = Some(Pending::Apply { from: cur }),
            None => self.toast("Windows pointer settings could not be read on this PC."),
        }
    }

    /// Open the confirmation for restoring the saved settings.
    pub fn request_restore(&mut self) {
        match (self.win_mouse.backup.previous, platform::mouse_params()) {
            (Some(to), Some(from)) => self.win_mouse.pending = Some(Pending::Restore { from, to }),
            (None, _) => self.toast("There are no saved settings to restore."),
            (_, None) => self.toast("Windows pointer settings could not be read on this PC."),
        }
    }

    fn save_backup(&mut self, b: MouseBackup) -> Result<(), String> {
        let path = crate::paths::mouse_backup_file();
        let r = if b.can_restore() {
            save_json(&path, &b).map_err(|e| e.to_string())
        } else {
            match std::fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
                _ => Ok(()),
            }
        };
        if r.is_ok() {
            self.win_mouse.backup = b;
        }
        r
    }

    fn do_apply(&mut self, from: MouseParams) {
        // Save the backup first; never change Windows settings without a way back.
        let backup = backup_for_apply(self.win_mouse.backup, from, unix_now());
        if let Err(e) = self.save_backup(backup) {
            self.win_mouse.outcome = Some(Outcome {
                ok: false,
                title: "Nothing was changed".into(),
                body: format!("Your current settings could not be saved for Restore previous ({e})."),
            });
            return;
        }
        let write = platform::set_mouse_params(MouseParams::NEUTRAL);
        let check = verify(MouseParams::NEUTRAL, platform::mouse_params());
        self.refresh_system_info();
        self.setup.accel = Some(false);
        self.setup.speed = 10;
        self.win_mouse.outcome = Some(match (write, check) {
            (Ok(()), Verify::Ok) => Outcome {
                ok: true,
                title: "Windows settings applied".into(),
                body: "Enhance pointer precision is off and pointer speed is 10. Restore previous puts your old settings back.".into(),
            },
            (Err(e), _) => Outcome { ok: false, title: "Windows did not accept the change".into(), body: e },
            (Ok(()), Verify::Mismatch { actual }) => Outcome {
                ok: false,
                title: "Windows reports different values".into(),
                body: format!("Now: {}. A policy or mouse software may be overriding these settings.", describe(actual)),
            },
            (Ok(()), Verify::Unreadable) => Outcome {
                ok: false,
                title: "Could not confirm the change".into(),
                body: "The settings were written but could not be read back. Check Mouse settings in Windows.".into(),
            },
        });
    }

    fn do_restore(&mut self, to: MouseParams) {
        let write = platform::set_mouse_params(to);
        let check = verify(to, platform::mouse_params());
        let ok = write.is_ok() && check == Verify::Ok;
        let kept = backup_after_restore(self.win_mouse.backup, if ok { Verify::Ok } else { check });
        let save = self.save_backup(kept);
        self.refresh_system_info();
        if let Some(m) = self.mouse_settings {
            self.setup.accel = Some(m.enhance_pointer_precision);
            self.setup.speed = m.pointer_speed;
        }
        self.win_mouse.outcome = Some(if ok {
            Outcome {
                ok: true,
                title: "Previous settings restored".into(),
                body: format!("{}.{}", describe(to), if save.is_err() { " The saved copy could not be removed." } else { "" }),
            }
        } else {
            Outcome {
                ok: false,
                title: "Restore did not complete".into(),
                body: match write {
                    Err(e) => e,
                    Ok(()) => "Windows reports different values. The saved copy is kept so you can try again.".into(),
                },
            }
        });
    }

    /// Call before screens read keys: while the confirmation is open it owns
    /// Enter (confirm) and Esc (cancel).
    pub fn win_mouse_keys(&mut self, ctx: &egui::Context) {
        let Some(p) = self.win_mouse.pending else { return };
        let (enter, esc) = ctx.input_mut(|i| {
            let e = i.consume_key(egui::Modifiers::NONE, Key::Enter);
            let x = i.consume_key(egui::Modifiers::NONE, Key::Escape);
            for k in [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9] {
                i.consume_key(egui::Modifiers::NONE, k);
            }
            (e, x)
        });
        if esc {
            self.win_mouse.pending = None;
        } else if enter {
            self.confirm_pending(p);
        }
    }

    fn confirm_pending(&mut self, p: Pending) {
        self.win_mouse.pending = None;
        match p {
            Pending::Apply { from } => self.do_apply(from),
            Pending::Restore { to, .. } => self.do_restore(to),
        }
    }

    /// The confirmation dialog.
    pub fn win_mouse_dialog(&mut self, ctx: &egui::Context) {
        let Some(p) = self.win_mouse.pending else { return };
        let (title, from, to, action) = match p {
            Pending::Apply { from } => ("Apply to Windows?", from, MouseParams::NEUTRAL, "Apply"),
            Pending::Restore { from, to } => ("Restore previous settings?", from, to, "Restore"),
        };
        let mut confirm = false;
        let mut cancel = false;
        let frame = egui::Frame::new()
            .fill(SURFACE)
            .stroke(egui::Stroke::new(1.0, BORDER))
            .corner_radius(radius(R_CARD))
            .inner_margin(egui::Margin::same(24));
        let resp = egui::Modal::new(crate::motion::id("win_mouse_modal")).frame(frame).show(ctx, |ui| {
            ui.set_width(460.0);
            // Short windows: the dialog scrolls instead of running off screen.
            // The buttons stay below the scrolling part, always in view.
            let max_h = (ui.ctx().content_rect().height() - 96.0 - 48.0 - 90.0).max(120.0);
            ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
            egui::ScrollArea::vertical().max_height(max_h).min_scrolled_height(max_h).show(ui, |ui| {
                ui.label(egui::RichText::new(title).font(bold(20.0)).color(TEXT));
                ui.add_space(6.0);
                widgets::dim(ui, "Changes the mouse settings for your Windows account. No admin rights are needed.");
                ui.add_space(14.0);
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                egui::Grid::new(crate::motion::id("win_mouse_grid")).num_columns(4).spacing([14.0, 10.0]).show(ui, |ui| {
                    let list = changes(from, to);
                    if list.is_empty() {
                        ui.label(egui::RichText::new("No differences").color(TEXT_DIM));
                        ui.end_row();
                    }
                    for c in list {
                        ui.label(egui::RichText::new(c.label).font(semibold(14.0)).color(TEXT));
                        ui.label(egui::RichText::new(c.from).color(TEXT_DIM));
                        ui.label(egui::RichText::new("→").color(TEXT_DIM));
                        ui.label(egui::RichText::new(c.to).font(semibold(14.0)).color(CORAL));
                        ui.end_row();
                    }
                });
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                ui.add_space(14.0);
                if matches!(p, Pending::Apply { .. }) {
                    widgets::dim(ui, "Your current values are saved first, so Restore previous (here or in Settings) can undo this.");
                    ui.add_space(4.0);
                    widgets::dim(ui, "In-game sensitivity is not changed; games keep it in their own settings.");
                    ui.add_space(14.0);
                }
            });
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                if widgets::primary_button(ui, &mut self.motion, action, true).clicked() {
                    confirm = true;
                }
                ui.add_space(8.0);
                if widgets::secondary_button(ui, &mut self.motion, "Cancel", None).clicked() {
                    cancel = true;
                }
            });
            ui.add_space(8.0);
            widgets::key_hint(ui, &[("Enter", action), ("Esc", "cancel")]);
        });
        if cancel || resp.should_close() {
            self.win_mouse.pending = None;
        } else if confirm {
            self.confirm_pending(p);
        }
    }

    /// One-line description of the current Windows pointer settings.
    pub fn win_mouse_summary(&self) -> String {
        match platform::mouse_params() {
            Some(c) if c.is_neutral() => "Neutral: Enhance pointer precision off, pointer speed 10.".into(),
            Some(c) => format!("{}. Neutral is precision off, speed 10.", describe(c)),
            None => "Not readable on this platform.".into(),
        }
    }

    /// Current pointer settings with Apply / Restore actions and the last outcome.
    pub fn win_mouse_panel(&mut self, ui: &mut egui::Ui, id: &str) {
        let cur = platform::mouse_params();
        let neutral = cur.is_some_and(|c| c.is_neutral());
        if let Some(o) = self.win_mouse.outcome.clone() {
            widgets::notice(ui, if o.ok { Tone::Ok } else { Tone::Warn }, &o.title, &o.body);
            ui.add_space(8.0);
        }
        ui.push_id(id, |ui| {
            ui.horizontal(|ui| {
                if cur.is_some()
                    && !neutral
                    && widgets::secondary_button(ui, &mut self.motion, "Apply to Windows", Some(widgets::Icon::Check)).clicked()
                {
                    self.win_mouse.outcome = None;
                    self.request_apply_neutral();
                }
                if self.win_mouse.backup.can_restore() {
                    if cur.is_some() && !neutral {
                        ui.add_space(8.0);
                    }
                    if widgets::secondary_button(ui, &mut self.motion, "Restore previous", Some(widgets::Icon::Rotate)).clicked() {
                        self.win_mouse.outcome = None;
                        self.request_restore();
                    }
                }
            });
        });
    }
}

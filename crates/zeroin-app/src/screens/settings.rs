//! Settings, device checks and about.

use crate::app::ZeroInApp;
use crate::hw::wooting::{WootingAnalog, WASD_HID};
use crate::llm::LlmStatus;
use crate::platform::{self, InputEvent, InputKind};
use crate::theme::*;
use crate::widgets::{self, Tone};
use eframe::egui::{self, Align2, Pos2, Rect, Stroke, StrokeKind};
use zeroin_core::bayes::Mode;
use zeroin_core::history::TestLength;

/// Raw keyboard check for Hall-effect / rapid-trigger boards.
#[derive(Default)]
pub struct KeyboardCheck {
    down: [bool; 4],
    last_up: [Option<u64>; 4],
    /// Shortest release→press gap seen per key (µs): rapid-trigger boards get very low values.
    pub fastest_repress_us: [Option<u64>; 4],
    pub events: u64,
}

const WASD_VK: [u16; 4] = [0x57, 0x41, 0x53, 0x44];

impl KeyboardCheck {
    pub fn feed(&mut self, ev: &InputEvent) {
        if let InputKind::Key { vkey, down } = ev.kind {
            if let Some(i) = WASD_VK.iter().position(|v| *v == vkey) {
                self.events += 1;
                if down && !self.down[i] {
                    if let Some(up) = self.last_up[i] {
                        let gap = ev.t_us.saturating_sub(up);
                        if gap > 0 && gap < 500_000 {
                            self.fastest_repress_us[i] = Some(self.fastest_repress_us[i].map_or(gap, |g| g.min(gap)));
                        }
                    }
                } else if !down && self.down[i] {
                    self.last_up[i] = Some(ev.t_us);
                }
                self.down[i] = down;
            }
        }
    }
}

impl ZeroInApp {
    pub fn settings_screen(&mut self, ui: &mut egui::Ui) {
        widgets::heading(ui, "Settings", "");
        let w = ui.available_width().min(680.0);

        widgets::caption(ui, "TEST");
        widgets::card(ui, |ui| {
            ui.set_width(w);
            widgets::setting_row(ui, "Test length", "Quick: 6 blocks, about 2.5 min. Standard: 9 blocks, about 4 min.", |ui| {
                let labels = vec!["Quick".to_string(), "Standard".to_string()];
                let sel = match self.settings.test_length {
                    TestLength::Quick => 0,
                    TestLength::Standard => 1,
                };
                if let Some(i) = widgets::chip_row(ui, &mut self.motion, crate::motion::id("len_chips"), &labels, Some(sel)) {
                    self.settings.test_length = if i == 0 { TestLength::Quick } else { TestLength::Standard };
                    self.save_settings();
                }
            });
            widgets::setting_row(ui, "When to suggest a change", "Predicted gain needed: 2, 1 or 0.5 points.", |ui| {
                const MODES: [Mode; 3] = [Mode::Conservative, Mode::Balanced, Mode::Explorer];
                let labels: Vec<String> = MODES.iter().map(|m| m.label().to_string()).collect();
                let sel = MODES.iter().position(|m| *m == self.settings.mode);
                if let Some(i) = widgets::chip_row(ui, &mut self.motion, crate::motion::id("mode_chips"), &labels, sel) {
                    self.settings.mode = MODES[i];
                    self.save_settings();
                }
            });
            widgets::dim(ui, "Changes apply to the next test.");
        });
        ui.add_space(12.0);

        widgets::caption(ui, "LOCAL AI");
        let status = self.llm_status();
        widgets::card(ui, |ui| {
            ui.set_width(w);
            let mut on = self.settings.ai_enabled;
            widgets::setting_row(ui, "Use the local AI model", "Writes a plain-language explanation of each result. It never picks test settings or changes a number. Runs on your CPU; nothing is sent anywhere.", |ui| {
                if widgets::toggle(ui, &mut self.motion, crate::motion::id("ai_toggle"), &mut on, true).changed() {}
            });
            if on != self.settings.ai_enabled {
                self.settings.ai_enabled = on;
                self.save_settings();
                if on {
                    self.llm.start(ui.ctx())
                } else {
                    self.llm.stop()
                }
            }
            let detail = match &status {
                LlmStatus::Ready { model } => format!("Ready · {model}"),
                LlmStatus::Failed(e) => format!("Could not load: {e}"),
                LlmStatus::NotInstalled => format!(
                    "No .gguf model found. Looked in: {}",
                    crate::paths::model_dirs().iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(" ; ")
                ),
                s => s.label(),
            };
            widgets::dim(ui, detail);
        });
        ui.add_space(12.0);

        widgets::caption(ui, "DEVICES");
        widgets::card(ui, |ui| {
            ui.set_width(w);
            // Mouse
            let poll = match &self.polling.last {
                Some(s) => format!(
                    "{:.0} reports/s{} · {:.0}% steady",
                    s.rate_hz,
                    s.nominal_hz.map(|n| format!(" ({n} Hz)")).unwrap_or_default(),
                    s.stability * 100.0
                ),
                None if self.raw.is_native() => "Move the mouse in circles to measure.".into(),
                None => "Raw input unavailable on this platform.".into(),
            };
            widgets::setting_row(ui, "Mouse polling rate", &poll, |ui| {
                if widgets::secondary_button(ui, &mut self.motion, "Check mouse", None).clicked() {
                    self.mouse_check.open = true;
                }
            });
            let disp = match self.display {
                Some(d) => format!("{}×{} at {} Hz (max {} Hz at this resolution)", d.width, d.height, d.current_hz, d.max_hz),
                None => "Unknown".into(),
            };
            widgets::setting_row(ui, "Display", &disp, |ui| {
                if widgets::secondary_button(ui, &mut self.motion, "Re-check", Some(widgets::Icon::Rotate)).clicked() {
                    self.refresh_system_info();
                }
            });
            if let Some(d) = self.display {
                if d.current_hz + 2 < d.max_hz {
                    widgets::notice(
                        ui,
                        Tone::Warn,
                        "Display is below its maximum refresh rate",
                        "Windows Settings → System → Display → Advanced display.",
                    );
                }
            }
            let ms = self.win_mouse_summary();
            widgets::setting_row(ui, "Windows mouse settings", &ms, |_| {});
            self.win_mouse_panel(ui, "settings");
            self.temp_mouse_setting(ui);
            // Haptics
            let mut hap = self.settings.haptics_enabled && self.haptics_available;
            widgets::setting_row(
                ui,
                "Haptic feedback",
                if self.haptics_available {
                    "Pulse on hits and misses."
                } else {
                    "No supported haptic mouse found. Support is added per device where an open SDK exists."
                },
                |ui| {
                    widgets::toggle(ui, &mut self.motion, crate::motion::id("hap_toggle"), &mut hap, self.haptics_available);
                },
            );
            if self.haptics_available && hap != self.settings.haptics_enabled {
                self.settings.haptics_enabled = hap;
                self.save_settings();
            }
        });
        ui.add_space(12.0);

        widgets::caption(ui, "KEYBOARD CHECK (HALL-EFFECT / RAPID TRIGGER)");
        widgets::card(ui, |ui| {
            ui.set_width(w);
            let mut an = self.settings.analog_keyboard_enabled;
            widgets::setting_row(
                ui,
                "Analog key depth (Wooting Analog SDK)",
                match &self.wooting {
                    Ok(wa) => {
                        if wa.devices > 0 {
                            "SDK loaded · analog keyboard connected"
                        } else {
                            "SDK loaded · no analog keyboard detected"
                        }
                    }
                    Err(e) => e.as_str(),
                },
                |ui| {
                    widgets::toggle(ui, &mut self.motion, crate::motion::id("an_toggle"), &mut an, true);
                },
            );
            if an != self.settings.analog_keyboard_enabled {
                self.settings.analog_keyboard_enabled = an;
                self.save_settings();
                self.wooting = if an { WootingAnalog::load() } else { Err("Turned off".into()) };
            }
            widgets::dim(ui, "Press and release W A S D quickly. Rapid-trigger boards re-press in a few milliseconds.");
            ui.add_space(6.0);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(w.min(520.0), 92.0), egui::Sense::hover());
            let names = ["W", "A", "S", "D"];
            for i in 0..4 {
                let r = Rect::from_min_size(Pos2::new(rect.left() + i as f32 * 128.0, rect.top()), egui::vec2(116.0, 88.0));
                let down = self.keyboard_down(i);
                let t = self.motion.value(crate::motion::id(("key", i)), if down { 1.0 } else { 0.0 });
                ui.painter().rect_filled(r, radius(R_CTRL), crate::motion::lerp_color(SURFACE_HI, CORAL_DIM, t));
                ui.painter().rect_stroke(
                    r,
                    radius(R_CTRL),
                    Stroke::new(1.0, crate::motion::lerp_color(BORDER, CORAL, t)),
                    StrokeKind::Inside,
                );
                ui.painter().text(r.center_top() + egui::vec2(0.0, 20.0), Align2::CENTER_CENTER, names[i], bold(18.0), TEXT);
                if let Ok(wa) = &self.wooting {
                    let depth = wa.read(WASD_HID[i]);
                    let bar = Rect::from_min_size(Pos2::new(r.left() + 12.0, r.top() + 40.0), egui::vec2((r.width() - 24.0) * depth, 6.0));
                    ui.painter().rect_filled(
                        Rect::from_min_size(Pos2::new(r.left() + 12.0, r.top() + 40.0), egui::vec2(r.width() - 24.0, 6.0)),
                        radius(3),
                        SURFACE,
                    );
                    ui.painter().rect_filled(bar, radius(3), CORAL);
                }
                let fast =
                    self.keyboard.fastest_repress_us[i].map(|g| format!("{:.1} ms", g as f64 / 1000.0)).unwrap_or_else(|| "—".into());
                ui.painter().text(
                    r.center_bottom() - egui::vec2(0.0, 16.0),
                    Align2::CENTER_CENTER,
                    format!("re-press {fast}"),
                    regular(11.5),
                    TEXT_DIM,
                );
            }
        });
        ui.add_space(12.0);

        widgets::caption(ui, "INTERFACE AND DATA");
        widgets::card(ui, |ui| {
            ui.set_width(w);
            let mut rm = self.settings.reduce_motion;
            widgets::setting_row(ui, "Reduce motion", "Turn off interface animation. Drills never animate.", |ui| {
                widgets::toggle(ui, &mut self.motion, crate::motion::id("rm_toggle"), &mut rm, true);
            });
            if rm != self.settings.reduce_motion {
                self.settings.reduce_motion = rm;
                self.save_settings();
            }
            let mut gl = self.settings.gear_lookup;
            widgets::setting_row(
                ui,
                "Gear lookup in your browser",
                "Off by default. When on, Setup offers a button that opens a web search for the mousepad name you typed, in your own browser. Only that name goes to the search site; ZeroIn itself sends nothing and reads nothing back.",
                |ui| {
                    widgets::toggle(ui, &mut self.motion, crate::motion::id("gl_toggle"), &mut gl, true);
                },
            );
            if gl != self.settings.gear_lookup {
                self.settings.gear_lookup = gl;
                self.save_settings();
            }
            let dir = crate::paths::data_dir();
            widgets::setting_row(ui, "Data folder", &dir.display().to_string(), |ui| {
                if widgets::secondary_button(ui, &mut self.motion, "Open", None).clicked() {
                    let _ = std::fs::create_dir_all(&dir);
                    platform::open_folder(&dir);
                }
            });
        });
        ui.add_space(12.0);
        widgets::caption(ui, "ABOUT");
        widgets::card(ui, |ui| {
            ui.set_width(w);
            ui.label(egui::RichText::new(format!("ZeroIn {}", env!("CARGO_PKG_VERSION"))).font(semibold(15.0)));
            widgets::dim(ui, "MIT License · Windows 10 and 11 · Fully local: no accounts, no telemetry; the app makes no network connections. Links open in your browser only when you click them.");
            widgets::dim(
                ui,
                "Third-party licences (model, font, icons, libraries) are listed in THIRD_PARTY_NOTICES.md in the install folder.",
            );
            ui.hyperlink_to("github.com/Lukecinate/zeroin", "https://github.com/Lukecinate/zeroin");
        });
    }

    fn keyboard_down(&self, i: usize) -> bool {
        self.keyboard_state()[i]
    }

    fn keyboard_state(&self) -> [bool; 4] {
        // Exposed through the check struct.
        let mut out = [false; 4];
        for (i, o) in out.iter_mut().enumerate() {
            *o = self.keyboard.is_down(i);
        }
        out
    }
}

impl KeyboardCheck {
    pub fn is_down(&self, i: usize) -> bool {
        self.down.get(i).copied().unwrap_or(false)
    }
}

//! Result: the recommended sensitivity, why, and the same feel in other games.

use crate::app::{Step, ZeroInApp};
use crate::llm::JobKind;
use crate::screens::setup::{trim_float, SetupStep};
use crate::theme::*;
use crate::widgets::{self, Tone};
use eframe::egui::{self, Align2, Key, Pos2, Rect, Stroke};
use zeroin_core::advisor::{self, ResultFacts};
use zeroin_core::games::{by_id, Confidence, GameProfile, GAMES};
use zeroin_core::history::{ExplanationSource, Session, WindowsMouse};
use zeroin_core::math;

pub struct ResultState {
    pub session: Session,
    pub explain_job: Option<u64>,
    pub from_rect: Option<Rect>,
    pub kicked: bool,
}

impl ZeroInApp {
    pub fn finish_test(&mut self) {
        let Some(t) = self.test.take() else { return };
        let Some(rec) = t.search.recommendation() else { return };
        let mut sens = t.profile.round_sens(t.start_sens * rec.mult);
        if let Some(g) = by_id(&t.profile.id) {
            sens = sens.clamp(g.sens_min, g.sens_max);
        }
        let cm = math::cm_per_360(t.dpi, t.profile.model.deg_per_count(sens));
        let start_cm = math::cm_per_360(t.dpi, t.profile.model.deg_per_count(t.start_sens));
        let obs = t.search.observations().to_vec();
        let facts = ResultFacts {
            game: &t.profile.name,
            start_sens: t.start_sens,
            recommended_sens: sens,
            cm360: cm,
            start_cm360: start_cm,
            confidence: rec.confidence,
            observations: &obs,
        };
        let explanation = advisor::fallback_explanation(&facts);
        let prompt = advisor::explanation_prompt(&facts);
        let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        let session = Session {
            id: format!("{now_ms}"),
            created_unix: (now_ms / 1000) as u64,
            game: t.profile.clone(),
            dpi: t.dpi,
            start_sens: t.start_sens,
            recommended_sens: sens,
            cm360: cm,
            confidence: rec.confidence,
            method: rec.method,
            observations: obs.clone(),
            explanation,
            explanation_source: ExplanationSource::BuiltIn,
            model_advised_rounds: t.advised_rounds.clone(),
            windows_mouse: self
                .mouse_settings
                .map(|m| WindowsMouse { pointer_speed: m.pointer_speed, enhance_pointer_precision: m.enhance_pointer_precision }),
            polling_hz: self.polling.last.map(|p| p.rate_hz),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
        };
        self.history.push(session.clone());
        self.save_history();
        let explain_job = if self.settings.ai_enabled { self.llm.submit(JobKind::Explain, prompt, 220) } else { None };
        self.result = Some(ResultState { session, explain_job, from_rect: t.best_rect, kicked: false });
        self.step = Step::Result;
    }

    pub fn poll_llm(&mut self) {
        for reply in self.llm.poll() {
            let Ok(text) = reply.text else { continue };
            match reply.kind {
                JobKind::Advice => {
                    if let Some(t) = self.test.as_mut() {
                        if let Some((id, round)) = t.advice_job {
                            // Only apply while that round has not started yet.
                            if id == reply.id && round == t.search.round() && t.round_cands.is_none() {
                                if let Some(a) = advisor::parse_advice(&text, t.search.advice_bounds()) {
                                    if let Some(m) = t.search.propose_mid(a.next_mult) {
                                        t.advised_rounds.push(round);
                                        let reason =
                                            if a.reason.is_empty() { String::new() } else { format!(" Reason given: {}", a.reason) };
                                        t.advice_note = Some(format!(
                                            "Its suggestion ({m:.2}× your start) was checked and kept within this round's range.{reason}"
                                        ));
                                    }
                                }
                                t.advice_job = None;
                            }
                        }
                    }
                }
                JobKind::Explain => {
                    let mut updated = None;
                    if let Some(r) = self.result.as_mut() {
                        if r.explain_job == Some(reply.id) {
                            r.explain_job = None;
                            let clean = advisor::sanitize_explanation(&text, advisor::MAX_EXPLANATION_CHARS);
                            if clean.chars().count() >= 40 {
                                r.session.explanation = clean;
                                r.session.explanation_source = ExplanationSource::LocalModel;
                                updated = Some(r.session.clone());
                            }
                        }
                    }
                    if let Some(s) = updated {
                        if let Some(h) = self.history.sessions.iter_mut().find(|h| h.id == s.id) {
                            *h = s;
                        }
                        self.save_history();
                    }
                }
            }
        }
    }

    pub fn result_screen(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let (esc, enter) = ctx.input(|i| (i.key_pressed(Key::Escape), i.key_pressed(Key::Enter)));
        let Some(r) = self.result.as_mut() else {
            widgets::heading(ui, "No result yet", "Run a test first.");
            return;
        };
        let s = r.session.clone();
        let dec = s.game.decimals as usize;
        widgets::caption(ui, "RESULT");
        ui.label(egui::RichText::new(format!("Your {} sensitivity", s.game.name)).font(semibold(18.0)).color(TEXT_DIM));

        // Hero number with a shared-element entrance from "Best so far".
        let text = trim_float(s.recommended_sens, dec);
        let size = 88.0;
        let (slot, _) = ui.allocate_exact_size(egui::vec2(ui.available_width().min(620.0), size * 1.1), egui::Sense::hover());
        let target = Pos2::new(slot.left(), slot.center().y);
        if !r.kicked {
            r.kicked = true;
            let (fx, fy, fs) = match r.from_rect {
                Some(fr) => (fr.left(), fr.center().y, 16.0),
                None => (target.x, target.y + 24.0, size * 0.8),
            };
            self.motion.kick(crate::motion::id("hero_x"), fx, target.x);
            self.motion.kick(crate::motion::id("hero_y"), fy, target.y);
            self.motion.kick(crate::motion::id("hero_s"), fs, size);
        }
        let x = self.motion.value(crate::motion::id("hero_x"), target.x);
        let y = self.motion.value(crate::motion::id("hero_y"), target.y);
        let fs = self.motion.value(crate::motion::id("hero_s"), size).max(8.0);
        let painter = ui.painter().clone().with_layer_id(egui::LayerId::new(egui::Order::Foreground, crate::motion::id("hero")));
        painter.text(Pos2::new(x, y), Align2::LEFT_CENTER, &text, bold(fs), CORAL);

        let dpc = s.game.model.deg_per_count(s.recommended_sens);
        ui.label(
            egui::RichText::new(format!(
                "{:.1} cm/360  ·  {:.2} in/360  ·  eDPI {:.0}  ·  {:.0} DPI",
                s.cm360,
                math::inches_per_360(s.dpi, dpc),
                math::edpi(s.dpi, s.recommended_sens),
                s.dpi
            ))
            .font(semibold(16.0)),
        );
        let change = s.recommended_sens / s.start_sens - 1.0;
        widgets::dim(
            ui,
            format!("Started at {} ({}{:.0}%)", trim_float(s.start_sens, dec), if change >= 0.0 { "+" } else { "−" }, change.abs() * 100.0),
        );
        ui.add_space(6.0);
        // Confidence bar
        let (bar, _) = ui.allocate_exact_size(egui::vec2(260.0, 6.0), egui::Sense::hover());
        ui.painter().rect_filled(bar, radius(3), SURFACE_HI);
        let w = self.motion.value(crate::motion::id("conf_bar"), bar.width() * s.confidence as f32);
        ui.painter().rect_filled(Rect::from_min_size(bar.min, egui::vec2(w, bar.height())), radius(3), TEXT_DIM);
        widgets::dim(
            ui,
            format!(
                "Confidence {:.0}%{}",
                s.confidence * 100.0,
                if s.confidence < 0.5 { " — scores were close; a second run helps" } else { "" }
            ),
        );
        ui.add_space(14.0);

        let waiting = r.explain_job.is_some();
        widgets::card(ui, |ui| {
            ui.set_width(ui.available_width().min(620.0));
            ui.horizontal(|ui| {
                let (ic, label) = match s.explanation_source {
                    ExplanationSource::LocalModel => (widgets::Icon::Cpu, "Written by the local AI model (on this PC)"),
                    ExplanationSource::BuiltIn => (widgets::Icon::Info, "Built-in summary"),
                };
                widgets::icon(ui, ic, 16.0, TEXT_FAINT);
                ui.label(egui::RichText::new(label).font(semibold(12.5)).color(TEXT_FAINT));
            });
            ui.add_space(4.0);
            ui.label(egui::RichText::new(&s.explanation).font(regular(15.0)).color(TEXT));
            if waiting {
                ui.add_space(4.0);
                widgets::dim(ui, "The local AI is writing a more detailed explanation…");
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
            }
        });
        if !s.model_advised_rounds.is_empty() {
            ui.add_space(6.0);
            widgets::dim(
                ui,
                format!(
                    "The local AI suggested the middle candidate in {} round(s); every suggestion was clamped to the algorithm's range.",
                    s.model_advised_rounds.len()
                ),
            );
        }
        ui.add_space(14.0);

        // Conversions
        ui.label(egui::RichText::new("Same feel in other games").font(semibold(16.0)));
        widgets::dim(ui, format!("At {:.0} DPI, hip-fire.", s.dpi));
        ui.add_space(4.0);
        widgets::card(ui, |ui| {
            ui.set_width(ui.available_width().min(620.0));
            egui::Grid::new("conv").num_columns(3).spacing([28.0, 6.0]).striped(false).show(ui, |ui| {
                for g in GAMES.iter().filter(|g| g.id != s.game.id) {
                    let to = GameProfile::from_game(g);
                    let v = math::convert(s.recommended_sens, &s.game, s.dpi, &to, s.dpi);
                    ui.label(egui::RichText::new(g.name).color(TEXT_DIM));
                    ui.label(egui::RichText::new(trim_float(to.round_sens(v), g.decimals as usize)).font(semibold(15.0)));
                    let est = g.confidence == Confidence::Estimate || s.game.confidence == Confidence::Estimate;
                    ui.label(egui::RichText::new(if est { "estimate" } else { "" }).font(regular(12.0)).color(WARN));
                    ui.end_row();
                }
            });
        });
        ui.add_space(18.0);
        let mut done = false;
        let mut again = false;
        ui.horizontal(|ui| {
            if widgets::primary_button(ui, &mut self.motion, "Done", true).clicked() {
                done = true;
            }
            ui.add_space(8.0);
            if widgets::secondary_button(ui, &mut self.motion, "Test again from this value", Some(widgets::Icon::Rotate)).clicked() {
                again = true;
            }
            if widgets::secondary_button(ui, &mut self.motion, "Copy value", None).clicked() {
                ui.ctx().copy_text(text.clone());
                self.toast = Some((format!("Copied {text}"), crate::platform::now_us() as f64 / 1e6 + 2.5));
            }
        });
        ui.add_space(10.0);
        widgets::key_hint(ui, &[("Enter", "done"), ("Esc", "back to setup")]);
        let _ = Stroke::NONE;
        let _ = Tone::Info;

        if enter {
            done = true;
        }
        if done || again {
            self.setup.sens_text = trim_float(s.recommended_sens, dec);
            self.setup.sub = SetupStep::Ready;
            self.step = Step::Setup;
            if again {
                self.begin_test();
            }
        } else if esc {
            self.setup.sub = SetupStep::Ready;
            self.step = Step::Setup;
        }
    }
}

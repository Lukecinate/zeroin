//! Result: keep or change, the range of about-equally-good settings, why,
//! and the same feel in other games.

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
use zeroin_core::search::RecommendationMethod;

pub struct ResultState {
    pub session: Session,
    pub explain_job: Option<u64>,
    /// Facts the model's explanation is checked against.
    pub facts: OwnedFacts,
    pub kicked: bool,
}

/// Owned copy of [`ResultFacts`] kept until the model replies.
pub struct OwnedFacts {
    pub keep: bool,
    pub start_text: String,
    pub recommended_text: String,
    pub band_text: (String, String),
    pub gain_pts: f64,
}

fn facts<'a>(s: &'a Session, f: &OwnedFacts) -> ResultFacts<'a> {
    ResultFacts {
        game: &s.game.name,
        keep: f.keep,
        start_text: f.start_text.clone(),
        recommended_text: f.recommended_text.clone(),
        start_sens: s.start_sens,
        recommended_sens: s.recommended_sens,
        band_text: f.band_text.clone(),
        gain_pts: f.gain_pts,
        cm360: s.cm360,
        start_cm360: math::cm_per_360(s.dpi, s.game.model.deg_per_count(s.start_sens)),
        observations: &s.observations,
    }
}

/// Sensitivity for a degrees-per-count multiplier, rounded and clamped to the game's limits.
pub fn sens_for_mult(p: &GameProfile, start_sens: f64, mult: f64) -> f64 {
    let mut sens = p.round_sens(p.model.sens_for_deg_per_count(p.model.deg_per_count(start_sens) * mult));
    if let Some(g) = by_id(&p.id) {
        sens = sens.clamp(g.sens_min, g.sens_max);
    }
    if sens.is_finite() && sens > 0.0 {
        sens
    } else {
        start_sens
    }
}

impl ZeroInApp {
    pub fn finish_test(&mut self) {
        let Some(t) = self.test.take() else { return };
        let Some(d) = t.search.decision() else { return };
        let p = &t.profile;
        let dec = p.decimals as usize;
        let sens = if d.keep { t.start_sens } else { sens_for_mult(p, t.start_sens, d.mult) };
        let (mut lo, mut hi) = (sens_for_mult(p, t.start_sens, d.band.0), sens_for_mult(p, t.start_sens, d.band.1));
        if lo > hi {
            std::mem::swap(&mut lo, &mut hi);
        }
        let cm = math::cm_per_360(t.dpi, p.model.deg_per_count(sens));
        let owned = OwnedFacts {
            keep: d.keep,
            start_text: trim_float(t.start_sens, dec),
            recommended_text: trim_float(sens, dec),
            band_text: (trim_float(lo, dec), trim_float(hi, dec)),
            gain_pts: d.gain_pts,
        };
        let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        let mut session = Session {
            id: format!("{now_ms}"),
            created_unix: (now_ms / 1000) as u64,
            game: t.profile.clone(),
            dpi: t.dpi,
            start_sens: t.start_sens,
            recommended_sens: sens,
            cm360: cm,
            confidence: 0.0,
            keep: d.keep,
            band: Some((lo, hi)),
            gain_pts: Some(d.gain_pts),
            mode: Some(t.search.config().mode),
            method: RecommendationMethod::Bayes,
            observations: t.search.observations().to_vec(),
            explanation: String::new(),
            explanation_source: ExplanationSource::BuiltIn,
            model_advised_rounds: Vec::new(),
            windows_mouse: self
                .mouse_settings
                .map(|m| WindowsMouse { pointer_speed: m.pointer_speed, enhance_pointer_precision: m.enhance_pointer_precision }),
            polling_hz: self.polling.last.map(|p| p.rate_hz),
            gear: {
                let g = self.settings.gear.sanitized();
                (!g.is_empty()).then_some(g)
            },
            app_version: env!("CARGO_PKG_VERSION").to_string(),
        };
        let f = facts(&session, &owned);
        let explanation = advisor::fallback_explanation(&f);
        let prompt = advisor::explanation_prompt(&f);
        session.explanation = explanation;
        self.history.push(session.clone());
        self.save_history();
        self.discard_saved_test();
        let explain_job = if self.settings.ai_enabled { self.llm.submit(JobKind::Explain, prompt, 220) } else { None };
        self.result = Some(ResultState { session, explain_job, facts: owned, kicked: false });
        self.step = Step::Result;
    }

    pub fn poll_llm(&mut self) {
        for reply in self.llm.poll() {
            let Ok(text) = reply.text else { continue };
            match reply.kind {
                JobKind::Explain => {
                    let mut updated = None;
                    if let Some(r) = self.result.as_mut() {
                        if r.explain_job == Some(reply.id) {
                            r.explain_job = None;
                            let clean = advisor::sanitize_explanation(&text, advisor::MAX_EXPLANATION_CHARS);
                            // The model may only explain: text that states other numbers is dropped.
                            let ok =
                                clean.chars().count() >= 40 && advisor::explanation_is_consistent(&clean, &facts(&r.session, &r.facts));
                            if ok {
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
        let headline =
            if s.keep { format!("Keep your current {} sensitivity", s.game.name) } else { format!("Your new {} sensitivity", s.game.name) };
        ui.label(egui::RichText::new(headline).font(semibold(18.0)).color(TEXT_DIM));

        // Hero number with a short entrance.
        let text = trim_float(s.recommended_sens, dec);
        let size = 88.0;
        let (slot, _) = ui.allocate_exact_size(egui::vec2(ui.available_width().min(620.0), size * 1.1), egui::Sense::hover());
        let target = Pos2::new(slot.left(), slot.center().y);
        if !r.kicked {
            r.kicked = true;
            self.motion.kick(crate::motion::id("hero_x"), target.x, target.x);
            self.motion.kick(crate::motion::id("hero_y"), target.y + 24.0, target.y);
            self.motion.kick(crate::motion::id("hero_s"), size * 0.8, size);
        }
        let x = self.motion.value(crate::motion::id("hero_x"), target.x);
        let y = self.motion.value(crate::motion::id("hero_y"), target.y);
        let fs = self.motion.value(crate::motion::id("hero_s"), size).max(8.0);
        let painter = ui.painter().clone().with_layer_id(egui::LayerId::new(egui::Order::Foreground, crate::motion::id("hero")));
        painter.text(Pos2::new(x, y), Align2::LEFT_CENTER, &text, bold(fs), CORAL);

        let dpc = s.game.model.deg_per_count(s.recommended_sens);
        ui.label(
            egui::RichText::new(format!(
                "about {:.1} cm/360  ·  {:.2} in/360  ·  eDPI {:.0}  ·  {:.0} DPI",
                advisor::half(s.cm360),
                math::inches_per_360(s.dpi, dpc),
                math::edpi(s.dpi, s.recommended_sens),
                s.dpi
            ))
            .font(semibold(16.0)),
        );
        if s.keep {
            widgets::dim(ui, "No tested setting scored clearly better than the one you already use.");
        } else {
            let change = s.recommended_sens / s.start_sens - 1.0;
            widgets::dim(
                ui,
                format!(
                    "Started at {} ({}{:.0}%). Predicted gain: about {} on the 100-point block score.",
                    trim_float(s.start_sens, dec),
                    if change >= 0.0 { "+" } else { "−" },
                    change.abs() * 100.0,
                    match s.gain_pts.unwrap_or(0.0).round() as i64 {
                        ..=1 => "1 point".to_string(),
                        n => format!("{n} points"),
                    }
                ),
            );
        }
        ui.add_space(10.0);
        if let Some((lo, hi)) = s.band {
            let cm_lo = math::cm_per_360(s.dpi, s.game.model.deg_per_count(hi));
            let cm_hi = math::cm_per_360(s.dpi, s.game.model.deg_per_count(lo));
            ui.label(
                egui::RichText::new(format!(
                    "About equally good: {} to {}  ({:.0}–{:.0} cm/360)",
                    trim_float(lo, dec),
                    trim_float(hi, dec),
                    cm_lo,
                    cm_hi
                ))
                .font(semibold(15.0)),
            );
            band_chart(ui, &s, lo, hi, ui.available_width().min(620.0));
            widgets::dim(ui, "Anything in this range performed within about 2 points of the best. Pick what feels right in game.");
        }
        if let Some(sum) = s.gear.as_ref().and_then(|g| g.summary()) {
            ui.add_space(6.0);
            widgets::dim(ui, format!("{sum}. Self-reported; it does not change the result."));
        }
        if let Some(adv) = s.gear.as_ref().and_then(|g| zeroin_core::gear::advice(g, &s.game.id)) {
            ui.add_space(6.0);
            let (k0, k1) = adv.try_slower;
            let slower = |k: f64| sens_for_mult(&s.game, s.recommended_sens, 1.0 / k);
            let body = format!(
                "{} For this result that is about {:.0}–{:.0} cm/360 (sens {}–{}).",
                adv.text,
                s.cm360 * k0,
                s.cm360 * k1,
                trim_float(slower(k1), dec),
                trim_float(slower(k0), dec)
            );
            widgets::notice(ui, Tone::Info, "About your mousepad and feet", &body);
            egui::CollapsingHeader::new(egui::RichText::new("Why we say this").font(semibold(13.0)).color(TEXT_DIM))
                .id_salt("gear_why")
                .show(ui, |ui| {
                    for w in adv.why {
                        widgets::dim(ui, format!("• {w}"));
                    }
                });
        }
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
        if s.explanation_source == ExplanationSource::LocalModel {
            ui.add_space(6.0);
            widgets::dim(ui, "The local AI only writes this text. Every number on this screen comes from the test itself.");
        }
        self.pixel_skip_notice(ui, &s.game, s.dpi, s.recommended_sens);
        ui.add_space(14.0);

        // Windows pointer settings and where the value goes.
        ui.label(egui::RichText::new("Use it").font(semibold(16.0)));
        widgets::dim(ui, format!("Enter {text} in {}'s own sensitivity setting. ZeroIn does not change game files.", s.game.name));
        ui.add_space(2.0);
        widgets::dim(ui, format!("Windows: {}", self.win_mouse_summary()));
        ui.add_space(6.0);
        self.win_mouse_panel(ui, "result");
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

/// Tested positions, the equally-good band and the current / recommended
/// markers on one horizontal axis (log scale).
fn band_chart(ui: &mut egui::Ui, s: &Session, lo: f64, hi: f64, width: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 46.0), egui::Sense::hover());
    let p = ui.painter();
    let tested: Vec<f64> = s.observations.iter().map(|o| o.mult.ln()).collect();
    let m = s.game.model;
    let rel = |sens: f64| (m.deg_per_count(sens) / m.deg_per_count(s.start_sens)).ln();
    let (x_lo, x_hi, x_rec) = (rel(lo), rel(hi), rel(s.recommended_sens));
    let min = tested.iter().cloned().fold(x_lo.min(0.0), f64::min) - 0.08;
    let max = tested.iter().cloned().fold(x_hi.max(0.0), f64::max) + 0.08;
    let to_x = |v: f64| rect.left() + ((v - min) / (max - min).max(1e-6)) as f32 * rect.width();
    let axis_y = rect.top() + 18.0;
    p.rect_filled(Rect::from_min_max(Pos2::new(rect.left(), axis_y - 2.0), Pos2::new(rect.right(), axis_y + 2.0)), radius(2), SURFACE_HI);
    p.rect_filled(
        Rect::from_min_max(Pos2::new(to_x(x_lo), axis_y - 5.0), Pos2::new(to_x(x_hi).max(to_x(x_lo) + 4.0), axis_y + 5.0)),
        radius(3),
        TEXT_FAINT,
    );
    for v in &tested {
        p.circle_filled(Pos2::new(to_x(*v), axis_y), 2.5, TEXT_DIM);
    }
    // Current sens: a tick; recommendation: a coral dot.
    let cx = to_x(0.0);
    p.line_segment([Pos2::new(cx, axis_y - 10.0), Pos2::new(cx, axis_y + 10.0)], Stroke::new(2.0, TEXT));
    p.circle_filled(Pos2::new(to_x(x_rec), axis_y), 6.0, CORAL);
    let lbl_y = rect.bottom() - 6.0;
    p.text(Pos2::new(cx, lbl_y), Align2::CENTER_CENTER, "current", regular(11.5), TEXT_DIM);
    if (to_x(x_rec) - cx).abs() > 50.0 {
        p.text(Pos2::new(to_x(x_rec), lbl_y), Align2::CENTER_CENTER, "recommended", regular(11.5), CORAL);
    }
}

impl ZeroInApp {
    /// Warn when one mouse count turns the view by more than a screen pixel.
    pub fn pixel_skip_notice(&mut self, ui: &mut egui::Ui, game: &GameProfile, dpi: f64, sens: f64) {
        let width_px =
            self.display.map(|d| d.width as f64).unwrap_or_else(|| (ui.ctx().content_rect().width() * ui.ctx().pixels_per_point()) as f64);
        let dpc = game.model.deg_per_count(sens);
        if !math::skips_pixels(dpc, game.hfov_deg, width_px) {
            return;
        }
        let dpp = math::deg_per_pixel_at_centre(game.hfov_deg, width_px);
        let k = if dpc / dpp <= 2.0 { 2.0 } else { 4.0 };
        let new_dpi = (dpi * k).min(32000.0);
        let new_sens = game.round_sens(math::convert(sens, game, dpi, game, new_dpi));
        widgets::notice(
            ui,
            Tone::Warn,
            "Aim steps skip pixels",
            &format!(
                "At {} and {dpi:.0} DPI one mouse count turns {dpc:.3}°, more than one pixel ({dpp:.3}°) at the centre of a {width_px:.0}-pixel-wide screen, so the finest aim adjustment jumps. \
For the same cm/360 with finer steps, set your mouse to {new_dpi:.0} DPI and the in-game sensitivity to {}.",
                trim_float(sens, game.decimals as usize),
                trim_float(new_sens, game.decimals as usize)
            ),
        );
    }
}

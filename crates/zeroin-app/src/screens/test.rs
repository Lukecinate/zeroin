//! Test: a budget of blocks, each running flick, micro-adjust and tracking
//! drills at one sensitivity chosen by the search.
//!
//! Drill frames are drawn with plain painter calls and no UI animation, vsync
//! is turned off, and input comes from the raw-input thread with per-report
//! timestamps. The player is not told which sensitivity a block uses.

use crate::app::{Step, ZeroInApp};
use crate::platform::{self, InputKind};
use crate::theme::*;
use crate::widgets::{self, Tone};
use eframe::egui::{self, Align2, Color32, Key, Pos2, Rect, Stroke};
use zeroin_core::bayes::{BayesConfig, BayesSearch};
use zeroin_core::drills::{project, Angles, Block, DrillKind, DrillPlan, InvalidReason, Preset};
use zeroin_core::games::{by_id, GameProfile};
use zeroin_core::history::{save_json, SavedTest, MAX_RERUNS, SAVED_TEST_VERSION};
use zeroin_core::math;
use zeroin_core::scoring::score_block;

#[derive(Clone, Debug, PartialEq)]
pub enum Phase {
    /// Between blocks: progress, time left and a start button.
    Intro,
    Running,
    Paused,
    /// The last block's data was not used and it will run again.
    Rerun(InvalidReason),
    /// Too many invalid blocks: no result.
    Incomplete(InvalidReason),
}

pub struct TestState {
    pub fingerprint: String,
    pub profile: GameProfile,
    pub dpi: f64,
    pub start_sens: f64,
    pub start_dpc: f64,
    pub preset: Preset,
    pub plan: DrillPlan,
    pub pair_seed: u64,
    pub search: BayesSearch,
    pub created_unix: u64,
    pub invalid_blocks: u32,
    pub block_seconds: Vec<f64>,
    pub resumed: bool,
    /// Multiplier (degrees per count) of the running block, after rounding.
    pub block_mult: f64,
    pub block_started_us: u64,
    pub block: Option<Block>,
    pub phase: Phase,
    pub flash: Option<(bool, u64)>,
}

pub fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl TestState {
    pub fn is_running(&self) -> bool {
        self.phase == Phase::Running && self.block.is_some()
    }

    pub fn planned_blocks(&self) -> usize {
        self.search.config().blocks as usize
    }

    /// Seconds per block: measured when available, otherwise estimated.
    fn block_estimate_s(&self) -> f64 {
        if self.block_seconds.is_empty() {
            self.plan.estimated_block_s()
        } else {
            (self.block_seconds.iter().sum::<f64>() / self.block_seconds.len() as f64).clamp(10.0, 120.0)
        }
    }

    pub fn seconds_left(&self) -> f64 {
        (self.planned_blocks() - self.search.blocks_done().min(self.planned_blocks())) as f64 * self.block_estimate_s()
    }

    /// Sensitivity for a log change `x`, rounded to what the game accepts and
    /// clamped to its limits, plus the multiplier actually tested.
    fn sens_for(&self, x: f64) -> (f64, f64) {
        let model = self.profile.model;
        let mut sens = self.profile.round_sens(model.sens_for_deg_per_count(self.start_dpc * x.exp()));
        if let Some(g) = by_id(&self.profile.id) {
            sens = sens.clamp(g.sens_min, g.sens_max);
        }
        if !(sens.is_finite() && sens > 0.0) {
            sens = self.start_sens;
        }
        (sens, model.deg_per_count(sens) / self.start_dpc)
    }

    pub fn start_block(&mut self, now_us: u64) {
        let Some(x) = self.search.next_x() else { return };
        let (_, mult) = self.sens_for(x);
        self.block_mult = mult;
        // Same targets every block, new order each attempt (re-runs included).
        let attempt = self.search.blocks_done() as u64 + 100 * self.invalid_blocks as u64;
        let order_seed = self.pair_seed.rotate_left(17) ^ (attempt + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        self.block = Some(Block::new(self.start_dpc * mult, &self.plan, self.pair_seed, order_seed));
        self.block_started_us = now_us;
        self.phase = Phase::Running;
        self.resumed = false;
    }

    pub fn pause(&mut self) {
        // The interrupted block restarts from its first drill on resume.
        self.block = None;
        self.phase = Phase::Paused;
    }

    pub fn to_saved(&self) -> SavedTest {
        SavedTest {
            version: SAVED_TEST_VERSION,
            created_unix: self.created_unix,
            saved_unix: unix_now(),
            fingerprint: self.fingerprint.clone(),
            game: self.profile.clone(),
            dpi: self.dpi,
            start_sens: self.start_sens,
            preset: self.preset,
            plan: self.plan,
            pair_seed: self.pair_seed,
            search: self.search.clone(),
            invalid_blocks: self.invalid_blocks,
            block_seconds: self.block_seconds.clone(),
        }
    }

    pub fn from_saved(s: SavedTest, fingerprint: String) -> TestState {
        let start_dpc = s.game.model.deg_per_count(s.start_sens);
        TestState {
            fingerprint,
            profile: s.game,
            dpi: s.dpi,
            start_sens: s.start_sens,
            start_dpc,
            preset: s.preset,
            plan: s.plan,
            pair_seed: s.pair_seed,
            search: s.search,
            created_unix: s.created_unix,
            invalid_blocks: s.invalid_blocks,
            block_seconds: s.block_seconds,
            resumed: true,
            block_mult: 1.0,
            block_started_us: 0,
            block: None,
            phase: Phase::Intro,
            flash: None,
        }
    }
}

pub fn format_time_left(s: f64) -> String {
    if s < 60.0 {
        "under a minute left".into()
    } else {
        format!("about {} min left", (s / 60.0).round().max(1.0) as u32)
    }
}

/// Segmented session progress bar: one segment per planned block.
pub fn budget_bar(ui: &mut egui::Ui, done: usize, total: usize, current: f32, width: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 6.0), egui::Sense::hover());
    let gap = 4.0;
    let n = total.max(1);
    let seg = (rect.width() - gap * (n as f32 - 1.0)) / n as f32;
    for i in 0..n {
        let r = Rect::from_min_size(Pos2::new(rect.left() + i as f32 * (seg + gap), rect.top()), egui::vec2(seg, rect.height()));
        ui.painter().rect_filled(r, radius(3), SURFACE_HI);
        let fill = if i < done {
            1.0
        } else if i == done {
            current.clamp(0.0, 1.0)
        } else {
            0.0
        };
        if fill > 0.0 {
            ui.painter().rect_filled(Rect::from_min_size(r.min, egui::vec2(seg * fill, r.height())), radius(3), CORAL);
        }
    }
}

impl ZeroInApp {
    pub fn begin_test(&mut self) {
        let fp = self.setup.fingerprint();
        if self.test.as_ref().is_some_and(|t| t.fingerprint == fp && !matches!(t.phase, Phase::Incomplete(_))) {
            self.step = Step::Test;
            return;
        }
        let (Some(profile), Some(dpi), Some(sens)) = (self.setup.profile(), self.setup.dpi(), self.setup.sens()) else { return };
        let start_dpc = profile.model.deg_per_count(sens);
        let prior = zeroin_core::priors::for_game(&profile.id);
        let cfg = BayesConfig::new(
            self.settings.test_length.blocks(),
            self.settings.mode,
            prior.theta_centre(math::cm_per_360(dpi, start_dpc)),
            prior.sd_log,
        );
        let preset = Preset::for_game(&profile.id);
        // A new test replaces any unfinished one.
        self.discard_saved_test();
        self.test = Some(TestState {
            fingerprint: fp,
            profile,
            dpi,
            start_sens: sens,
            start_dpc,
            preset,
            plan: DrillPlan::for_preset(preset),
            pair_seed: platform::now_us() ^ 0x5EED_2026,
            search: BayesSearch::new(cfg),
            created_unix: unix_now(),
            invalid_blocks: 0,
            block_seconds: Vec::new(),
            resumed: false,
            block_mult: 1.0,
            block_started_us: 0,
            block: None,
            phase: Phase::Intro,
            flash: None,
        });
        self.result = None;
        self.step = Step::Test;
    }

    pub fn discard_saved_test(&mut self) {
        self.saved_test = None;
        let _ = std::fs::remove_file(crate::paths::saved_test_file());
    }

    /// Continue an unfinished test saved on disk.
    pub fn resume_saved_test(&mut self) {
        let Some(saved) = self.saved_test.take() else { return };
        self.setup.apply(&saved.game, saved.dpi, saved.start_sens);
        let fp = self.setup.fingerprint();
        self.test = Some(TestState::from_saved(saved, fp));
        self.result = None;
        self.section = crate::app::Section::Find;
        self.step = Step::Test;
    }

    /// Hide and confine the cursor while a drill is live; release it the
    /// moment the drill ends, pauses, loses focus or is minimised.
    pub fn sync_cursor(&mut self, ctx: &egui::Context, hwnd: Option<isize>) {
        let inputs = zeroin_core::cursor::CaptureInputs {
            drill_active: self.in_drill(),
            focused: ctx.input(|i| i.focused),
            minimized: ctx.input(|i| i.viewport().minimized.unwrap_or(false)),
        };
        match self.cursor.update(inputs) {
            Some(zeroin_core::cursor::CaptureAction::Capture) => {
                if !self.raw.is_native() {
                    // Development fallback; on Windows the clip is applied natively below.
                    ctx.send_viewport_cmd(egui::ViewportCommand::CursorGrab(egui::CursorGrab::Confined));
                }
            }
            Some(zeroin_core::cursor::CaptureAction::Release) => release_cursor(ctx),
            None => {}
        }
        if self.cursor.is_captured() {
            // Every frame: egui shows the cursor again whenever the requested icon changes.
            ctx.set_cursor_icon(egui::CursorIcon::None);
            if self.raw.is_native() {
                if let Some(h) = hwnd {
                    platform::confine_cursor(h);
                }
            }
        }
    }

    /// Non-drill test screens: block intro, re-run notice, pause and "incomplete".
    pub fn test_screen(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let (enter, esc) = ctx.input(|i| (i.key_pressed(Key::Enter), i.key_pressed(Key::Escape)));
        let Some(t) = self.test.as_mut() else {
            widgets::heading(ui, "No test yet", "Finish Setup first.");
            return;
        };
        let total = t.planned_blocks();
        let done = t.search.blocks_done().min(total);
        let block_no = (done + 1).min(total);
        let w = ui.available_width().min(620.0);
        let mut start = false;
        let mut back = false;

        if let Phase::Incomplete(reason) = t.phase.clone() {
            widgets::caption(ui, "TEST INCOMPLETE");
            widgets::heading(
                ui,
                "Test incomplete",
                &format!("{done} of {total} blocks gave usable data, and at least 6 are needed for a result. Nothing was saved."),
            );
            widgets::notice(ui, Tone::Warn, "Why", reason.message());
            ui.add_space(4.0);
            widgets::dim(ui, reason.advice());
            ui.add_space(20.0);
            if widgets::primary_button(ui, &mut self.motion, "Back to setup", true).clicked() || enter || esc {
                self.test = None;
                self.step = Step::Setup;
            }
            return;
        }

        widgets::caption(ui, &format!("BLOCK {block_no} OF {total}"));
        match &t.phase {
            Phase::Paused => widgets::heading(
                ui,
                "Paused",
                "The block you were on restarts from its first drill when you continue. Finished blocks are kept.",
            ),
            Phase::Rerun(_) => widgets::heading(ui, &format!("Block {block_no} runs again"), "Its data was not used."),
            _ if done == 0 && !t.resumed => widgets::heading(
                ui,
                "Find your sensitivity",
                "Each block plays the same short drills at a sensitivity the test picks. You are not told which one, so play each block naturally.",
            ),
            _ if t.resumed => {
                widgets::heading(ui, "Welcome back", &format!("Your unfinished test continues at block {block_no}."))
            }
            _ => widgets::heading(ui, &format!("Block {block_no}"), "Same drills, a different sensitivity. Take a breath if you need one."),
        }
        if let Phase::Rerun(reason) = &t.phase {
            widgets::notice(
                ui,
                Tone::Warn,
                reason.message(),
                &format!(
                    "{} This is re-run {} of {MAX_RERUNS}; a third unusable block ends the test without a result.",
                    reason.advice(),
                    t.invalid_blocks.min(MAX_RERUNS)
                ),
            );
            ui.add_space(6.0);
        }
        widgets::card(ui, |ui| {
            ui.set_width(w);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("{done} of {total} blocks done")).font(semibold(15.0)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(format_time_left(t.seconds_left())).color(TEXT_DIM));
                });
            });
            ui.add_space(6.0);
            budget_bar(ui, done, total, 0.0, ui.available_width());
            ui.add_space(12.0);
            for k in [DrillKind::Flick, DrillKind::Micro, DrillKind::Tracking] {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(k.label()).font(semibold(15.0)));
                    ui.label(egui::RichText::new(k.instruction()).color(TEXT_DIM));
                });
            }
            ui.add_space(4.0);
            widgets::dim(
                ui,
                format!(
                    "The first two flicks of each block are warm-up and are not scored. Targets use the {} preset for {}. Your cursor is hidden during drills; Esc pauses.",
                    t.preset.label(),
                    t.profile.name
                ),
            );
        });
        ui.add_space(20.0);
        let label = match t.phase {
            Phase::Paused => "Continue".to_string(),
            Phase::Rerun(_) => format!("Run block {block_no} again"),
            _ => format!("Start block {block_no}"),
        };
        ui.horizontal(|ui| {
            if widgets::primary_button(ui, &mut self.motion, &label, true).clicked() {
                start = true;
            }
            ui.add_space(8.0);
            if widgets::secondary_button(ui, &mut self.motion, "Back to setup", Some(widgets::Icon::ArrowLeft)).clicked() {
                back = true;
            }
        });
        ui.add_space(10.0);
        widgets::key_hint(ui, &[("Enter", "start"), ("Esc", "back to setup")]);
        widgets::dim(ui, "Progress is saved after every block. If you stop, you can resume from Setup within 24 hours.");
        if enter {
            start = true;
        }
        if esc {
            back = true;
        }
        let native = self.raw.is_native();
        let baseline = self.polling.baseline;
        let Some(t) = self.test.as_mut() else { return };
        if start {
            t.start_block(platform::now_us());
            if native {
                if let Some(b) = t.block.as_mut() {
                    b.enable_polling_rule(baseline);
                }
            }
        } else if back {
            self.step = Step::Setup;
        }
    }

    /// Full-window drill renderer and input loop.
    pub fn drill_view(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ctx.request_repaint();
        let now = platform::now_us();
        // FPS meter
        self.frame_times.push_back(now as f64 / 1e6);
        while self.frame_times.front().is_some_and(|f| now as f64 / 1e6 - f > 1.0) {
            self.frame_times.pop_front();
        }
        let fps = self.frame_times.len();
        let esc = ctx.input(|i| i.key_pressed(Key::Escape));
        let focused = ctx.input(|i| i.focused);
        let events = std::mem::take(&mut self.events);
        let mut finished_block = false;
        {
            let Some(t) = self.test.as_mut() else { return };
            let minimized = ctx.input(|i| i.viewport().minimized.unwrap_or(false));
            if esc || !focused || minimized {
                t.pause();
                self.events = events;
                return;
            }
            let Some(block) = t.block.as_mut() else { return };
            block.frame(now);
            for ev in &events {
                block.tick(ev.t_us);
                match ev.kind {
                    InputKind::Motion { dx, dy } => block.motion(dx, dy, ev.t_us),
                    InputKind::LeftButton { down: true } => {
                        if let Some(hit) = block.click(ev.t_us) {
                            t.flash = Some((hit, ev.t_us));
                        }
                    }
                    _ => {}
                }
            }
            block.tick(now);
            if block.is_done() {
                finished_block = true;
            }
        }
        self.events = events;

        let full = ui.max_rect();
        let p = ui.painter();
        p.rect_filled(full, 0.0, DRILL_BG);
        let t = self.test.as_ref().expect("test");
        let block = t.block.as_ref().expect("block");
        let session = block.current();
        let cam = session.camera();
        let hfov = t.profile.hfov_deg;
        let aspect = (full.width() / full.height()) as f64;
        let to_screen = |ndc: (f64, f64)| {
            Pos2::new(full.center().x + ndc.0 as f32 * full.width() / 2.0, full.center().y - ndc.1 as f32 * full.height() / 2.0)
        };

        // World grid for a sense of rotation.
        let grid = Stroke::new(1.0, Color32::from_rgb(0x1A, 0x1C, 0x20));
        let yaw0 = (cam.yaw / 10.0).floor() * 10.0;
        for k in -8..=8 {
            let yaw = yaw0 + k as f64 * 10.0;
            let pts: Vec<Pos2> =
                (-6..=6).filter_map(|j| project(cam, Angles { yaw, pitch: j as f64 * 10.0 }, hfov, aspect).map(to_screen)).collect();
            if pts.len() > 1 {
                p.add(egui::Shape::line(pts, grid));
            }
        }
        for j in -6..=6 {
            let pitch = j as f64 * 10.0;
            let pts: Vec<Pos2> = (-16..=16)
                .filter_map(|k| project(cam, Angles { yaw: yaw0 + k as f64 * 5.0, pitch }, hfov, aspect).map(to_screen))
                .collect();
            if pts.len() > 1 {
                p.add(egui::Shape::line(pts, if j == 0 { Stroke::new(1.0, Color32::from_rgb(0x24, 0x26, 0x2B)) } else { grid }));
            }
        }
        let px_per_rad = full.width() as f64 / 2.0 / (hfov.to_radians() / 2.0).tan();

        // Target
        if let Some(target) = session.target() {
            if let Some(ndc) = project(cam, target.at, hfov, aspect) {
                let c = to_screen(ndc);
                let r = (target.radius_deg.to_radians().tan() * px_per_rad) as f32;
                let on = zeroin_core::drills::angular_distance(cam, target.at) <= target.radius_deg;
                if session.kind() == DrillKind::Tracking {
                    p.circle_filled(c, r, if on { CORAL } else { CORAL_DIM });
                    p.circle_stroke(c, r, Stroke::new(2.0, CORAL));
                } else if session.is_warmup() {
                    p.circle_filled(c, r, CORAL_DIM);
                    p.circle_stroke(c, r, Stroke::new(2.0, CORAL));
                } else {
                    p.circle_filled(c, r, CORAL);
                    p.circle_filled(c, (r * 0.28).max(1.5), INK);
                }
            }
        }
        // Crosshair, snapped to physical pixels so it stays sharp at any display scaling.
        let cc = full.center();
        crosshair(p, cc, ctx.pixels_per_point());
        if let Some((hit, at)) = t.flash {
            if now.saturating_sub(at) < 120_000 {
                p.circle_stroke(cc, 20.0, Stroke::new(2.0, if hit { OK } else { WARN }));
            }
        }

        // HUD
        let total = t.planned_blocks();
        let done = t.search.blocks_done().min(total);
        let (di, dn) = block.position();
        let trial_info = match session.kind() {
            DrillKind::Tracking => String::new(),
            _ if session.is_warmup() => " · warm-up".to_string(),
            _ => format!(" · {}", session.trials().len() + 1),
        };
        p.text(
            full.left_top() + egui::vec2(24.0, 20.0),
            Align2::LEFT_TOP,
            format!("Block {} of {} · {} {}/{}{}", done + 1, total, session.kind().label(), di + 1, dn, trial_info),
            semibold(14.0),
            TEXT_DIM,
        );
        let hz = self.display.map(|d| format!(" · display {} Hz", d.current_hz)).unwrap_or_default();
        let poll = self.polling.last.map(|s| format!(" · mouse {:.0} Hz", s.rate_hz)).unwrap_or_default();
        p.text(full.right_top() + egui::vec2(-24.0, 20.0), Align2::RIGHT_TOP, format!("{fps} fps{hz}{poll}"), regular(12.5), TEXT_FAINT);
        p.text(full.left_bottom() + egui::vec2(24.0, -20.0), Align2::LEFT_BOTTOM, "Esc  pause", regular(12.5), TEXT_FAINT);
        let left = (t.seconds_left() - block.progress(now) * t.block_estimate_s()).max(0.0);
        p.text(full.right_bottom() + egui::vec2(-24.0, -20.0), Align2::RIGHT_BOTTOM, format_time_left(left), regular(12.5), TEXT_FAINT);
        // Session progress across all blocks.
        let prog = ((done as f64 + block.progress(now)) / total.max(1) as f64) as f32;
        let bar = Rect::from_min_size(Pos2::new(full.left(), full.bottom() - 3.0), egui::vec2(full.width() * prog, 3.0));
        p.rect_filled(bar, 0.0, CORAL);
        if !session.is_started() {
            p.rect_filled(
                Rect::from_center_size(cc + egui::vec2(0.0, 90.0), egui::vec2(520.0, 96.0)),
                radius(R_CARD),
                Color32::from_rgba_unmultiplied(0x19, 0x1A, 0x1D, 235),
            );
            p.text(
                cc + egui::vec2(0.0, 66.0),
                Align2::CENTER_CENTER,
                format!("Block {} · {}", done + 1, session.kind().label()),
                semibold(18.0),
                TEXT,
            );
            p.text(cc + egui::vec2(0.0, 92.0), Align2::CENTER_CENTER, session.kind().instruction(), regular(14.0), TEXT_DIM);
            p.text(cc + egui::vec2(0.0, 116.0), Align2::CENTER_CENTER, "Click to start", semibold(14.0), CORAL);
        }

        if finished_block {
            self.finish_block(now);
        }
    }

    fn finish_block(&mut self, now: u64) {
        enum Next {
            Save(Box<SavedTest>),
            Abort,
            Finish,
        }
        let next = {
            let Some(t) = self.test.as_mut() else { return };
            let Some(block) = t.block.take() else { return };
            match block.validity() {
                Err(reason) => {
                    t.invalid_blocks += 1;
                    if t.invalid_blocks > MAX_RERUNS {
                        t.phase = Phase::Incomplete(reason);
                        Next::Abort
                    } else {
                        t.phase = Phase::Rerun(reason);
                        Next::Save(Box::new(t.to_saved()))
                    }
                }
                Ok(()) => {
                    t.search.record(t.block_mult, score_block(&block.result()));
                    t.block_seconds.push(now.saturating_sub(t.block_started_us) as f64 / 1e6);
                    t.phase = Phase::Intro;
                    if t.search.is_done() {
                        Next::Finish
                    } else {
                        Next::Save(Box::new(t.to_saved()))
                    }
                }
            }
        };
        match next {
            Next::Save(s) => self.save_test(&s),
            Next::Abort => self.discard_saved_test(),
            Next::Finish => self.finish_test(),
        }
    }

    fn save_test(&mut self, saved: &SavedTest) {
        if let Err(e) = save_json(&crate::paths::saved_test_file(), saved) {
            self.toast(format!("Could not save progress: {e}"));
        }
    }
}

/// Crosshair in whole physical pixels: arms 14 pt long with a 5 pt gap,
/// 2 pt thick, and a centre dot, each rounded to device pixels.
fn crosshair(p: &egui::Painter, centre: Pos2, ppp: f32) {
    let px = |n: f32| (n * ppp).round().max(1.0);
    let (thick, arm, gap) = (px(2.0), px(14.0), px(5.0));
    // Centre on a pixel boundary (in device pixels), then convert back to points.
    let cx = (centre.x * ppp).round();
    let cy = (centre.y * ppp).round();
    let h0 = (thick / 2.0).floor();
    let rect = |x0: f32, y0: f32, x1: f32, y1: f32| Rect::from_min_max(Pos2::new(x0 / ppp, y0 / ppp), Pos2::new(x1 / ppp, y1 / ppp));
    let rects = [
        rect(cx - arm, cy - h0, cx - gap, cy - h0 + thick),
        rect(cx + gap, cy - h0, cx + arm, cy - h0 + thick),
        rect(cx - h0, cy - arm, cx - h0 + thick, cy - gap),
        rect(cx - h0, cy + gap, cx - h0 + thick, cy + arm),
        rect(cx - h0, cy - h0, cx - h0 + thick, cy - h0 + thick),
    ];
    for r in rects {
        p.rect_filled(r, 0.0, TEXT);
    }
}

/// Show the cursor and drop any confinement (both the toolkit's and Windows').
fn release_cursor(ctx: &egui::Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::CursorGrab(egui::CursorGrab::None));
    ctx.send_viewport_cmd(egui::ViewportCommand::CursorVisible(true));
    ctx.set_cursor_icon(egui::CursorIcon::Default);
    platform::release_cursor();
}

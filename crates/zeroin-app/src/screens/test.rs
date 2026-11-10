//! Test: blind rounds of three candidates, each running flick, micro-adjust and tracking drills.
//!
//! Drill frames are drawn with plain painter calls and no UI animation, vsync
//! is turned off, and input comes from the raw-input thread with per-report
//! timestamps.

use crate::app::{Step, ZeroInApp};
use crate::llm::JobKind;
use crate::platform::{self, InputKind};
use crate::theme::*;
use crate::widgets::{self, Tone};
use eframe::egui::{self, Align2, Color32, Key, Pos2, Rect, Stroke};
use zeroin_core::advisor::{self, AdviceContext};
use zeroin_core::drills::{project, Angles, Block, DrillKind, DrillPlan};
use zeroin_core::games::GameProfile;
use zeroin_core::history::TestLength;
use zeroin_core::rng::Rng;
use zeroin_core::scoring::{score_block, Score};
use zeroin_core::search::{Search, SearchConfig};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    RoundIntro,
    Running,
    Paused,
}

pub struct TestState {
    pub fingerprint: String,
    pub profile: GameProfile,
    pub dpi: f64,
    pub start_sens: f64,
    pub search: Search,
    pub plan: DrillPlan,
    pub round_cands: Option<[f64; 3]>,
    pub order: [usize; 3],
    pub pos: usize,
    pub scores: [Option<Score>; 3],
    pub block: Option<Block>,
    pub phase: Phase,
    pub advice_job: Option<(u64, u32)>,
    pub advised_rounds: Vec<u32>,
    pub advice_note: Option<String>,
    pub seed: u64,
    pub best_rect: Option<Rect>,
    pub flash: Option<(bool, u64)>,
}

pub const CAND_LABELS: [&str; 3] = ["A", "B", "C"];

impl TestState {
    pub fn is_running(&self) -> bool {
        self.phase == Phase::Running && self.block.is_some()
    }

    fn start_round(&mut self) {
        if self.round_cands.is_none() {
            self.round_cands = Some(self.search.candidates());
            // Blind order: the player does not know which candidate is faster.
            let mut rng = Rng::new(self.seed ^ ((self.search.round() as u64 + 1) * 0x9E37));
            let mut o = [0usize, 1, 2];
            for i in (1..3).rev() {
                let j = (rng.next_u64() % (i as u64 + 1)) as usize;
                o.swap(i, j);
            }
            self.order = o;
            self.pos = 0;
            self.scores = [None; 3];
        }
        self.start_block();
    }

    fn start_block(&mut self) {
        let Some(c) = self.round_cands else { return };
        let idx = self.order[self.pos];
        let sens = self.start_sens * c[idx];
        let dpc = self.profile.model.deg_per_count(sens);
        let seed = self.seed ^ ((self.search.round() as u64) << 16) ^ (self.pos as u64) << 4;
        self.block = Some(Block::new(dpc, &self.plan, seed));
        self.phase = Phase::Running;
    }

    pub fn pause(&mut self) {
        // The interrupted candidate restarts from its first drill on resume.
        self.block = None;
        self.phase = Phase::Paused;
    }

    pub fn best_so_far(&self) -> Option<f64> {
        let o = self.search.observations().iter().max_by(|a, b| a.score.total.partial_cmp(&b.score.total).unwrap())?;
        Some(self.profile.round_sens(self.start_sens * o.mult))
    }
}

impl ZeroInApp {
    pub fn begin_test(&mut self) {
        let fp = self.setup.fingerprint();
        if self.test.as_ref().is_some_and(|t| t.fingerprint == fp) {
            self.step = Step::Test;
            return;
        }
        let (Some(profile), Some(dpi), Some(sens)) = (self.setup.profile(), self.setup.dpi(), self.setup.sens()) else { return };
        let cfg = SearchConfig { rounds: self.settings.rounds.clamp(3, 5), ..Default::default() };
        let plan = match self.settings.test_length {
            TestLength::Quick => DrillPlan::QUICK,
            TestLength::Standard => DrillPlan::STANDARD,
        };
        self.test = Some(TestState {
            fingerprint: fp,
            profile,
            dpi,
            start_sens: sens,
            search: Search::new(cfg),
            plan,
            round_cands: None,
            order: [0, 1, 2],
            pos: 0,
            scores: [None; 3],
            block: None,
            phase: Phase::RoundIntro,
            advice_job: None,
            advised_rounds: Vec::new(),
            advice_note: None,
            seed: platform::now_us() ^ 0x5EED_2026,
            best_rect: None,
            flash: None,
        });
        self.result = None;
        self.step = Step::Test;
    }

    pub fn sync_cursor(&mut self, ctx: &egui::Context) {
        let want = self.in_drill();
        if want != self.cursor_captured {
            self.cursor_captured = want;
            ctx.send_viewport_cmd(egui::ViewportCommand::CursorVisible(!want));
            ctx.send_viewport_cmd(egui::ViewportCommand::CursorGrab(if want {
                egui::CursorGrab::Confined
            } else {
                egui::CursorGrab::None
            }));
        }
    }

    /// Non-drill test screens: round intro and pause.
    pub fn test_screen(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let (enter, esc) = ctx.input(|i| (i.key_pressed(Key::Enter), i.key_pressed(Key::Escape)));
        let Some(t) = self.test.as_mut() else {
            widgets::heading(ui, "No test yet", "Finish Setup first.");
            return;
        };
        let rounds = t.search.config().rounds;
        let round = t.search.round() + 1;
        let mut start = false;
        let mut back = false;
        match t.phase {
            Phase::RoundIntro | Phase::Running => {
                widgets::caption(ui, &format!("ROUND {round} OF {rounds}"));
                widgets::heading(ui, &format!("Round {round}: three candidates"), "Candidates A, B and C are different sensitivities in a random order, so you cannot tell which is faster. Play each one naturally.");
                widgets::card(ui, |ui| {
                    ui.set_width(ui.available_width().min(620.0));
                    for k in [DrillKind::Flick, DrillKind::Micro, DrillKind::Tracking] {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(k.label()).font(semibold(15.0)));
                            ui.label(egui::RichText::new(k.instruction()).color(TEXT_DIM));
                        });
                    }
                    ui.add_space(4.0);
                    widgets::dim(ui, "Your cursor is hidden during drills. Press Esc any time to pause.");
                });
                if let Some(n) = &t.advice_note {
                    ui.add_space(8.0);
                    widgets::notice(ui, Tone::Info, "Local AI adjusted the middle candidate", n);
                }
                if round > 1 {
                    if let Some(b) = t.best_so_far() {
                        ui.add_space(8.0);
                        let r = ui.label(
                            egui::RichText::new(format!(
                                "Best so far: {}",
                                crate::screens::setup::trim_float(b, t.profile.decimals as usize)
                            ))
                            .font(semibold(16.0))
                            .color(TEXT_DIM),
                        );
                        t.best_rect = Some(r.rect);
                    }
                }
                ui.add_space(20.0);
                ui.horizontal(|ui| {
                    if widgets::primary_button(ui, &mut self.motion, &format!("Start round {round}"), true).clicked() {
                        start = true;
                    }
                    ui.add_space(8.0);
                    if widgets::secondary_button(ui, &mut self.motion, "Back to setup", Some(widgets::Icon::ArrowLeft)).clicked() {
                        back = true;
                    }
                });
            }
            Phase::Paused => {
                widgets::caption(ui, &format!("ROUND {round} OF {rounds} · PAUSED"));
                widgets::heading(
                    ui,
                    "Paused",
                    "The candidate you were on restarts from its first drill when you continue. Finished candidates are kept.",
                );
                ui.horizontal(|ui| {
                    if widgets::primary_button(ui, &mut self.motion, "Continue", true).clicked() {
                        start = true;
                    }
                    ui.add_space(8.0);
                    if widgets::secondary_button(ui, &mut self.motion, "Back to setup", Some(widgets::Icon::ArrowLeft)).clicked() {
                        back = true;
                    }
                });
            }
        }
        ui.add_space(10.0);
        widgets::key_hint(ui, &[("Enter", "start"), ("Esc", "back to setup")]);
        if enter {
            start = true;
        }
        if esc {
            back = true;
        }
        let Some(t) = self.test.as_mut() else { return };
        if start {
            match t.phase {
                Phase::Paused if t.round_cands.is_some() => t.start_block(),
                _ => t.start_round(),
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
            if esc || !focused {
                t.pause();
                self.events = events;
                return;
            }
            let Some(block) = t.block.as_mut() else { return };
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
                } else {
                    p.circle_filled(c, r, CORAL);
                    p.circle_filled(c, (r * 0.28).max(1.5), INK);
                }
            }
        }
        // Crosshair
        let cc = full.center();
        let ch = Stroke::new(2.0, TEXT);
        for (a, b) in [((-14.0, 0.0), (-5.0, 0.0)), ((5.0, 0.0), (14.0, 0.0)), ((0.0, -14.0), (0.0, -5.0)), ((0.0, 5.0), (0.0, 14.0))] {
            p.line_segment([cc + egui::vec2(a.0, a.1), cc + egui::vec2(b.0, b.1)], ch);
        }
        p.circle_filled(cc, 1.5, TEXT);
        if let Some((hit, at)) = t.flash {
            if now.saturating_sub(at) < 120_000 {
                p.circle_stroke(cc, 20.0, Stroke::new(2.0, if hit { OK } else { WARN }));
            }
        }

        // HUD
        let rounds = t.search.config().rounds;
        let (di, dn) = block.position();
        let label = CAND_LABELS[t.order[t.pos].min(2)];
        let trial_info = match session.kind() {
            DrillKind::Tracking => String::new(),
            _ => format!(" · {}", session.trials().len() + 1),
        };
        p.text(
            full.left_top() + egui::vec2(24.0, 20.0),
            Align2::LEFT_TOP,
            format!(
                "Round {} of {} · Candidate {} ({}/3) · {} {}/{}{}",
                t.search.round() + 1,
                rounds,
                label,
                t.pos + 1,
                session.kind().label(),
                di + 1,
                dn,
                trial_info
            ),
            semibold(14.0),
            TEXT_DIM,
        );
        let hz = self.display.map(|d| format!(" · display {} Hz", d.current_hz)).unwrap_or_default();
        let poll = self.polling.last.map(|s| format!(" · mouse {:.0} Hz", s.rate_hz)).unwrap_or_default();
        p.text(full.right_top() + egui::vec2(-24.0, 20.0), Align2::RIGHT_TOP, format!("{fps} fps{hz}{poll}"), regular(12.5), TEXT_FAINT);
        p.text(full.left_bottom() + egui::vec2(24.0, -20.0), Align2::LEFT_BOTTOM, "Esc  pause", regular(12.5), TEXT_FAINT);
        let prog = block.progress(now) as f32;
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
                format!("Candidate {label} · {}", session.kind().label()),
                semibold(18.0),
                TEXT,
            );
            p.text(cc + egui::vec2(0.0, 92.0), Align2::CENTER_CENTER, session.kind().instruction(), regular(14.0), TEXT_DIM);
            p.text(cc + egui::vec2(0.0, 116.0), Align2::CENTER_CENTER, "Click to start", semibold(14.0), CORAL);
        }

        if finished_block {
            self.finish_block();
        }
    }

    fn finish_block(&mut self) {
        let llm_ready = self.settings.ai_enabled && self.llm.status().is_ready();
        let Some(t) = self.test.as_mut() else { return };
        let Some(block) = t.block.take() else { return };
        let idx = t.order[t.pos];
        t.scores[idx] = Some(score_block(&block.result()));
        t.pos += 1;
        if t.pos < 3 {
            t.start_block();
            return;
        }
        let scores = [t.scores[0].unwrap_or_default(), t.scores[1].unwrap_or_default(), t.scores[2].unwrap_or_default()];
        t.search.record_round(scores);
        t.round_cands = None;
        t.advice_note = None;
        t.phase = Phase::RoundIntro;
        if t.search.is_done() {
            self.finish_test();
            return;
        }
        if llm_ready {
            let ctx = AdviceContext {
                game: &t.profile.name,
                dpi: t.dpi,
                start_sens: t.start_sens,
                observations: t.search.observations(),
                bounds: t.search.advice_bounds(),
            };
            let prompt = advisor::next_candidate_prompt(&ctx);
            let round = t.search.round();
            if let Some(id) = self.llm.submit(JobKind::Advice, prompt, 96) {
                if let Some(t) = self.test.as_mut() {
                    t.advice_job = Some((id, round));
                }
            }
        }
    }
}

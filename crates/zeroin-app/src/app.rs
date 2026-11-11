//! Application state, navigation and the per-frame loop.

use crate::hw::wooting::WootingAnalog;
use crate::llm::{Llm, LlmStatus};
use crate::motion::Motion;
use crate::platform::{self, DisplayInfo, InputEvent, MouseSettings, RawInput};
use crate::screens::{result::ResultState, setup::SetupState, test::TestState};
use crate::theme::*;
use crate::widgets::{self, Icon};
use eframe::egui::{self, Align2, Key, Pos2, Rect, Sense, Stroke, Vec2};
use zeroin_core::history::{load_json, save_json, History, SavedTest, Settings};
use zeroin_core::polling::{self, PollingStats};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Find,
    History,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Setup,
    Test,
    Result,
}

impl Step {
    pub fn index(self) -> usize {
        match self {
            Step::Setup => 0,
            Step::Test => 1,
            Step::Result => 2,
        }
    }
}

/// Polling-rate meter fed from raw input.
#[derive(Default)]
pub struct PollingMeter {
    device: Option<u64>,
    stamps: Vec<u64>,
    motion: Vec<(u64, i32, i32)>,
    pub last: Option<PollingStats>,
    /// Jitter while moving fast, the pre-test value for the block rule.
    pub baseline: Option<polling::Jitter>,
}

impl PollingMeter {
    pub fn feed(&mut self, ev: &InputEvent) {
        if let platform::InputKind::Motion { dx, dy } = ev.kind {
            if self.device != Some(ev.device) {
                // Follow whichever mouse is moving.
                self.device = Some(ev.device);
                self.stamps.clear();
                self.motion.clear();
            }
            self.stamps.push(ev.t_us);
            self.motion.push((ev.t_us, dx, dy));
            if self.stamps.len() > 4000 {
                self.stamps.drain(..2000);
            }
            if self.motion.len() > 8000 {
                self.motion.drain(..4000);
            }
            if self.stamps.len().is_multiple_of(64) {
                if let Some(s) = polling::analyze(&self.stamps) {
                    self.last = Some(s);
                }
                if let Some(j) = polling::jitter(&self.motion) {
                    self.baseline = Some(j);
                }
            }
        }
    }
}

pub struct ZeroInApp {
    pub section: Section,
    pub step: Step,
    pub motion: Motion,
    pub settings: Settings,
    pub history: History,
    pub history_error: Option<String>,
    pub setup: SetupState,
    pub test: Option<TestState>,
    pub result: Option<ResultState>,
    /// Unfinished test found on disk, offered for resuming.
    pub saved_test: Option<SavedTest>,
    pub raw: RawInput,
    pub events: Vec<InputEvent>,
    pub polling: PollingMeter,
    pub mouse_check: crate::screens::mousecheck::MouseCheck,
    pub mouse_settings: Option<MouseSettings>,
    pub display: Option<DisplayInfo>,
    pub llm: Llm,
    pub wooting: Result<WootingAnalog, String>,
    pub haptics_available: bool,
    pub keyboard: crate::screens::settings::KeyboardCheck,
    prev_section: Section,
    prev_step: Step,
    pub toast: Option<(String, f64)>,
    pub fallback_frac: (f32, f32),
    pub vsync_off: bool,
    pub cursor: zeroin_core::cursor::CursorCapture,
    pub win_mouse: crate::screens::winmouse::WinMouseUi,
    pub frame_times: std::collections::VecDeque<f64>,
}

impl ZeroInApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let settings: Settings = load_json(&crate::paths::settings_file()).unwrap_or_default();
        let (history, history_error) = match load_json::<History>(&crate::paths::history_file()) {
            Ok(h) => (h, None),
            Err(e) => {
                // Keep the unreadable file aside instead of overwriting it.
                let p = crate::paths::history_file();
                let _ = std::fs::rename(&p, p.with_extension("json.unreadable"));
                (History::default(), Some(format!("History file could not be read and was set aside ({e}).")))
            }
        };
        let llm = Llm::new(&cc.egui_ctx, settings.ai_enabled);
        let wooting = if settings.analog_keyboard_enabled { WootingAnalog::load() } else { Err("Turned off".into()) };
        let mut motion = Motion::default();
        motion.reduce = settings.reduce_motion;
        let setup = SetupState::from_settings(&settings);
        // An unreadable, outdated or stale unfinished test is simply dropped.
        let saved_test = load_json::<Option<SavedTest>>(&crate::paths::saved_test_file())
            .ok()
            .flatten()
            .filter(|s| s.is_resumable(crate::screens::test::unix_now()));
        if saved_test.is_none() {
            let _ = std::fs::remove_file(crate::paths::saved_test_file());
        }
        ZeroInApp {
            section: Section::Find,
            step: Step::Setup,
            motion,
            settings,
            history,
            history_error,
            setup,
            test: None,
            result: None,
            saved_test,
            // Registered after the main window exists, so our registration is the active one.
            raw: RawInput::start(),
            events: Vec::new(),
            polling: PollingMeter::default(),
            mouse_check: Default::default(),
            mouse_settings: platform::mouse_settings(),
            display: platform::display_info(),
            llm,
            wooting,
            haptics_available: crate::hw::haptics::detect().is_some(),
            keyboard: Default::default(),
            prev_section: Section::Find,
            prev_step: Step::Setup,
            toast: None,
            fallback_frac: (0.0, 0.0),
            vsync_off: false,
            cursor: Default::default(),
            win_mouse: crate::screens::winmouse::WinMouseUi::load(),
            frame_times: Default::default(),
        }
    }

    pub fn save_settings(&self) {
        let _ = save_json(&crate::paths::settings_file(), &self.settings);
    }

    pub fn save_history(&mut self) {
        if let Err(e) = save_json(&crate::paths::history_file(), &self.history) {
            self.toast(format!("Could not save history: {e}"));
        }
    }

    pub fn toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), platform::now_us() as f64 / 1e6 + 3.5));
    }

    pub fn refresh_system_info(&mut self) {
        self.mouse_settings = platform::mouse_settings();
        self.display = platform::display_info();
    }

    pub fn llm_status(&self) -> LlmStatus {
        if !self.settings.ai_enabled {
            return LlmStatus::Off;
        }
        self.llm.status()
    }

    /// True while a drill is live: rail hidden, cursor captured, vsync off.
    pub fn in_drill(&self) -> bool {
        self.section == Section::Find && self.step == Step::Test && self.test.as_ref().is_some_and(|t| t.is_running())
    }

    fn collect_input(&mut self, ctx: &egui::Context) {
        self.events.clear();
        self.raw.drain(&mut self.events);
        if !self.raw.is_native() {
            // Development fallback: window mouse events instead of raw input.
            let now = platform::now_us();
            let (mut fx, mut fy) = self.fallback_frac;
            ctx.input(|i| {
                for e in &i.events {
                    match e {
                        egui::Event::MouseMoved(d) => {
                            fx += d.x;
                            fy += d.y;
                            let (dx, dy) = (fx.trunc() as i32, fy.trunc() as i32);
                            fx -= dx as f32;
                            fy -= dy as f32;
                            if dx != 0 || dy != 0 {
                                self.events.push(InputEvent { t_us: now, device: 0, kind: platform::InputKind::Motion { dx, dy } });
                            }
                        }
                        egui::Event::PointerButton { button: egui::PointerButton::Primary, pressed, .. } => {
                            self.events.push(InputEvent { t_us: now, device: 0, kind: platform::InputKind::LeftButton { down: *pressed } });
                        }
                        egui::Event::Key { key, pressed, repeat: false, .. } => {
                            let vkey = match key {
                                Key::W => 0x57,
                                Key::A => 0x41,
                                Key::S => 0x53,
                                Key::D => 0x44,
                                _ => 0,
                            };
                            if vkey != 0 {
                                self.events.push(InputEvent {
                                    t_us: now,
                                    device: 0,
                                    kind: platform::InputKind::Key { vkey, down: *pressed },
                                });
                            }
                        }
                        _ => {}
                    }
                }
            });
            self.fallback_frac = (fx, fy);
        }
        let focused = ctx.input(|i| i.focused);
        if !focused {
            // RIDEV_INPUTSINK delivers input in the background too; ignore it then.
            self.events.clear();
        }
        let evs = std::mem::take(&mut self.events);
        for e in &evs {
            self.polling.feed(e);
            self.keyboard.feed(e);
        }
        self.events = evs;
    }

    fn rail(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0.0, RAIL);
        ui.painter().line_segment([rect.right_top(), rect.right_bottom()], Stroke::new(1.0, BORDER));
        let logo_rect = Rect::from_min_size(rect.min + egui::vec2(22.0, 24.0), Vec2::splat(30.0));
        widgets::logo(ui.painter(), logo_rect);
        ui.painter().text(Pos2::new(logo_rect.right() + 12.0, logo_rect.center().y), Align2::LEFT_CENTER, "ZeroIn", bold(18.0), TEXT);

        let items = [
            (Section::Find, Icon::Crosshair, "Find my sens"),
            (Section::History, Icon::History, "History"),
            (Section::Settings, Icon::Settings, "Settings"),
        ];
        let top = rect.top() + 88.0;
        let item_h = 44.0;
        let active_idx = items.iter().position(|(s, _, _)| *s == self.section).unwrap_or(0) as f32;
        let y = self.motion.snappy(crate::motion::id("rail_indicator"), top + active_idx * item_h);
        for (i, (sec, ic, label)) in items.iter().enumerate() {
            let r =
                Rect::from_min_size(Pos2::new(rect.left() + 12.0, top + i as f32 * item_h), egui::vec2(rect.width() - 24.0, item_h - 4.0));
            let resp = ui.interact(r, crate::motion::id(("rail", i)), Sense::click());
            let hover = self.motion.bool(crate::motion::id(("rail_h", i)), resp.hovered());
            let on = self.motion.bool(crate::motion::id(("rail_on", i)), *sec == self.section);
            if hover > 0.01 {
                ui.painter().rect_filled(r, radius(R_CTRL), crate::motion::lerp_color(RAIL, SURFACE, hover));
            }
            let col = crate::motion::lerp_color(TEXT_DIM, TEXT, on.max(hover * 0.5));
            widgets::paint_icon(ui, *ic, Rect::from_min_size(Pos2::new(r.left() + 12.0, r.center().y - 9.0), Vec2::splat(18.0)), col);
            ui.painter().text(Pos2::new(r.left() + 42.0, r.center().y), Align2::LEFT_CENTER, *label, semibold(14.5), col);
            ui.painter().text(
                Pos2::new(r.right() - 10.0, r.center().y),
                Align2::RIGHT_CENTER,
                format!("F{}", i + 1),
                regular(10.5),
                TEXT_FAINT,
            );
            if resp.clicked() {
                self.section = *sec;
            }
            resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        }
        let ind = Rect::from_min_size(Pos2::new(rect.left() + 12.0, y + 10.0), egui::vec2(3.0, item_h - 24.0));
        ui.painter().rect_filled(ind, radius(2), CORAL);

        // Status block: readings that matter before a test.
        let mut lines: Vec<(Icon, String, Option<egui::Color32>)> = Vec::new();
        match &self.polling.last {
            Some(p) => lines.push((
                Icon::Mouse,
                format!(
                    "Mouse {:.0} Hz{}",
                    p.nominal_hz.map(|n| n as f64).unwrap_or(p.rate_hz),
                    if p.is_stable() { "" } else { " · unstable" }
                ),
                (!p.is_stable()).then_some(WARN),
            )),
            None => lines.push((
                Icon::Mouse,
                if self.raw.is_native() { "Mouse: move to measure".into() } else { "Mouse: raw input off".into() },
                None,
            )),
        }
        match &self.display {
            Some(d) => lines.push((Icon::Monitor, format!("Display {} Hz", d.current_hz), (d.current_hz + 2 < d.max_hz).then_some(WARN))),
            None => lines.push((Icon::Monitor, "Display: unknown".into(), None)),
        }
        let st = self.llm_status();
        lines.push((Icon::Cpu, st.label(), None));
        let mut ly = rect.bottom() - 24.0 - lines.len() as f32 * 24.0;
        for (ic, text, col) in lines {
            let c = col.unwrap_or(TEXT_FAINT);
            widgets::paint_icon(ui, ic, Rect::from_min_size(Pos2::new(rect.left() + 24.0, ly - 7.0), Vec2::splat(14.0)), c);
            ui.painter().text(Pos2::new(rect.left() + 46.0, ly), Align2::LEFT_CENTER, text, regular(12.0), c);
            ly += 24.0;
        }
    }

    /// Global keys: F1–F3 switch sections; Esc/Enter are handled by each screen.
    fn global_keys(&mut self, ctx: &egui::Context) {
        if self.in_drill() {
            return;
        }
        ctx.input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, Key::F1) {
                self.section = Section::Find;
            }
            if i.consume_key(egui::Modifiers::NONE, Key::F2) {
                self.section = Section::History;
            }
            if i.consume_key(egui::Modifiers::NONE, Key::F3) {
                self.section = Section::Settings;
            }
        });
    }

    fn content(&mut self, ui: &mut egui::Ui) {
        // Screen transition: slide + fade on section/step change.
        let key = (self.section, self.step);
        if key != (self.prev_section, self.prev_step) {
            let forward = self.section != self.prev_section || self.step.index() >= self.prev_step.index();
            self.motion.kick(crate::motion::id("screen_x"), if forward { 28.0 } else { -28.0 }, 0.0);
            self.motion.kick(crate::motion::id("screen_a"), 0.0, 1.0);
            self.prev_section = self.section;
            self.prev_step = self.step;
        }
        let dx = self.motion.value(crate::motion::id("screen_x"), 0.0);
        let alpha = self.motion.value(crate::motion::id("screen_a"), 1.0).clamp(0.0, 1.0);
        let full = ui.max_rect();
        let inner = Rect::from_min_max(full.min + egui::vec2(48.0 + dx, 32.0), full.max - egui::vec2(48.0 - dx, 24.0));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::top_down(egui::Align::Min)));
        child.set_opacity(alpha);
        match self.section {
            Section::Find => {
                let reach = [true, self.test.is_some(), self.result.is_some()];
                if let Some(i) = widgets::step_bar(&mut child, &mut self.motion, self.step.index(), reach) {
                    self.step = [Step::Setup, Step::Test, Step::Result][i];
                }
                child.add_space(24.0);
                match self.step {
                    Step::Setup => self.setup_screen(&mut child),
                    Step::Test => self.test_screen(&mut child),
                    Step::Result => self.result_screen(&mut child),
                }
            }
            Section::History => self.history_screen(&mut child),
            Section::Settings => {
                if self.mouse_check.open {
                    self.mouse_check_screen(&mut child)
                } else {
                    self.settings_screen(&mut child)
                }
            }
        }
    }

    fn update_vsync(&mut self, frame: &mut eframe::Frame) {
        let want_off = self.in_drill();
        if want_off != self.vsync_off {
            self.vsync_off = want_off;
            let cfg = if want_off {
                eframe::SurfaceConfig { present_mode: eframe::wgpu::PresentMode::AutoNoVsync, desired_maximum_frame_latency: Some(1) }
            } else {
                eframe::SurfaceConfig::LOW_LATENCY
            };
            frame.set_wgpu_surface_config(cfg);
        }
    }

    fn draw_toast(&mut self, ui: &mut egui::Ui) {
        let now = platform::now_us() as f64 / 1e6;
        if let Some((msg, until)) = self.toast.clone() {
            let visible = now < until;
            let a = self.motion.bool(crate::motion::id("toast"), visible);
            if !visible && a < 0.01 {
                self.toast = None;
                return;
            }
            let screen = ui.max_rect();
            let g = ui.painter().layout_no_wrap(msg, regular(13.5), TEXT);
            let size = g.size() + egui::vec2(32.0, 20.0);
            let r = Rect::from_center_size(Pos2::new(screen.center().x, screen.bottom() - 40.0 + (1.0 - a) * 16.0), size);
            let p = ui.painter().clone().with_layer_id(egui::LayerId::new(egui::Order::Foreground, crate::motion::id("toast_layer")));
            p.rect_filled(r, radius(R_CTRL), SURFACE_HI.gamma_multiply(a));
            p.rect_stroke(r, radius(R_CTRL), Stroke::new(1.0, BORDER.gamma_multiply(a)), egui::StrokeKind::Inside);
            p.galley(r.center() - g.size() / 2.0, g, TEXT.gamma_multiply(a));
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
        }
    }
}

impl eframe::App for ZeroInApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.motion.reduce = self.settings.reduce_motion;
        self.motion.begin_frame(&ctx);
        self.collect_input(&ctx);
        self.poll_llm();
        self.win_mouse_keys(&ctx);
        self.global_keys(&ctx);
        self.update_vsync(frame);

        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0.0, BG);
        if self.in_drill() {
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(full));
            self.drill_view(&mut child);
        } else {
            let rail_w = 228.0;
            let rail_rect = Rect::from_min_max(full.min, Pos2::new(full.left() + rail_w, full.bottom()));
            let main_rect = Rect::from_min_max(Pos2::new(full.left() + rail_w, full.top()), full.max);
            let mut rail = ui.new_child(egui::UiBuilder::new().max_rect(rail_rect));
            self.rail(&mut rail);
            let mut main = ui.new_child(egui::UiBuilder::new().max_rect(main_rect));
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(&mut main, |ui| {
                let r = ui.max_rect();
                ui.set_min_height((main_rect.height() - 2.0).max(r.height()));
                self.content(ui);
            });
            // Keep the polling meter and keyboard check live on screens that show them.
            let live = (self.section == Section::Find && self.step == Step::Setup && self.setup.is_ready_step())
                || self.section == Section::Settings;
            if live {
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
            }
        }
        self.sync_cursor(&ctx, window_hwnd(frame));
        self.win_mouse_dialog(&ctx);
        self.draw_toast(ui);
        self.motion.end_frame(&ctx);
    }

    fn on_exit(&mut self) {
        self.cursor.force_release();
        crate::platform::release_cursor();
        self.save_settings();
    }
}

#[cfg(windows)]
fn window_hwnd(frame: &eframe::Frame) -> Option<isize> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match frame.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
        _ => None,
    }
}

#[cfg(not(windows))]
fn window_hwnd(_frame: &eframe::Frame) -> Option<isize> {
    None
}

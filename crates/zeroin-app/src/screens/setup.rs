//! Setup: game → DPI → current sensitivity → pointer precision → pointer speed → review.

use crate::app::{Step, ZeroInApp};
use crate::theme::*;
use crate::widgets::{self, Tone};
use eframe::egui::{self, Key};
use zeroin_core::games::{Confidence, GameProfile, GAMES};
use zeroin_core::history::Settings;
use zeroin_core::math;

pub const DPI_OPTIONS: [f64; 5] = [400.0, 800.0, 1200.0, 1600.0, 3200.0];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SetupStep {
    Game,
    Dpi,
    Sens,
    Accel,
    Speed,
    Ready,
}

impl SetupStep {
    const ALL: [SetupStep; 6] = [SetupStep::Game, SetupStep::Dpi, SetupStep::Sens, SetupStep::Accel, SetupStep::Speed, SetupStep::Ready];
    fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }
    fn prev(self) -> Option<Self> {
        self.index().checked_sub(1).map(|i| Self::ALL[i])
    }
    fn next(self) -> Option<Self> {
        Self::ALL.get(self.index() + 1).copied()
    }
}

pub struct SetupState {
    pub sub: SetupStep,
    pub game: Option<usize>,
    pub custom_name: String,
    pub custom_yaw: String,
    pub dpi_choice: Option<usize>,
    pub dpi_text: String,
    pub sens_text: String,
    sens_prefilled_for: Option<usize>,
    pub accel: Option<bool>,
    pub speed: u32,
    pub speed_touched: bool,
}

/// Index used for "Other / custom yaw" in `game`.
pub const CUSTOM_IDX: usize = GAMES.len();

fn parse_num(s: &str) -> Option<f64> {
    let v: f64 = s.trim().replace(',', ".").parse().ok()?;
    v.is_finite().then_some(v)
}

impl SetupState {
    pub fn from_settings(s: &Settings) -> Self {
        let game = s.last_game.as_deref().and_then(|id| GAMES.iter().position(|g| g.id == id));
        let dpi = s.last_dpi.unwrap_or(800.0);
        let dpi_choice = DPI_OPTIONS.iter().position(|d| (*d - dpi).abs() < 0.5);
        SetupState {
            sub: SetupStep::Game,
            game,
            custom_name: String::new(),
            custom_yaw: "0.022".into(),
            dpi_choice: dpi_choice.or(Some(1)),
            dpi_text: if dpi_choice.is_none() { format!("{dpi:.0}") } else { String::new() },
            sens_text: String::new(),
            sens_prefilled_for: None,
            accel: None,
            speed: 10,
            speed_touched: false,
        }
    }

    pub fn is_ready_step(&self) -> bool {
        self.sub == SetupStep::Ready
    }

    pub fn profile(&self) -> Option<GameProfile> {
        match self.game? {
            CUSTOM_IDX => GameProfile::custom(&self.custom_name, parse_num(&self.custom_yaw)?),
            i => GAMES.get(i).map(GameProfile::from_game),
        }
    }

    pub fn dpi(&self) -> Option<f64> {
        let v = if self.dpi_text.trim().is_empty() { DPI_OPTIONS.get(self.dpi_choice?).copied()? } else { parse_num(&self.dpi_text)? };
        (100.0..=32000.0).contains(&v).then_some(v)
    }

    pub fn sens_range(&self) -> (f64, f64) {
        match self.game {
            Some(i) if i < GAMES.len() => (GAMES[i].sens_min, GAMES[i].sens_max),
            _ => (0.0001, 10000.0),
        }
    }

    pub fn sens(&self) -> Option<f64> {
        let v = parse_num(&self.sens_text)?;
        let (lo, hi) = self.sens_range();
        (v > 0.0 && v >= lo && v <= hi).then_some(v)
    }

    /// Identity of the inputs that define a test; changing any of them restarts the test.
    pub fn fingerprint(&self) -> String {
        format!("{:?}|{:?}|{:?}|{}|{}", self.profile().map(|p| (p.id, p.name)), self.dpi(), self.sens(), self.custom_yaw, self.custom_name)
    }

    fn valid(&self, step: SetupStep) -> bool {
        match step {
            SetupStep::Game => self.profile().is_some(),
            SetupStep::Dpi => self.dpi().is_some(),
            SetupStep::Sens => self.sens().is_some(),
            SetupStep::Accel => self.accel.is_some(),
            SetupStep::Speed | SetupStep::Ready => true,
        }
    }

    fn prefill_sens(&mut self) {
        if self.sens_prefilled_for != self.game {
            self.sens_prefilled_for = self.game;
            if let Some(i) = self.game.filter(|i| *i < GAMES.len()) {
                self.sens_text = trim_float(GAMES[i].prefill_sens, GAMES[i].decimals as usize);
            }
        }
    }
}

pub fn trim_float(v: f64, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

fn key_for(i: usize) -> Option<Key> {
    const KEYS: [Key; 10] = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9, Key::Num0];
    KEYS.get(i).copied()
}

fn label_for(i: usize) -> &'static str {
    ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"].get(i).copied().unwrap_or("")
}

impl ZeroInApp {
    pub fn setup_screen(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let typing = ctx.egui_wants_keyboard_input();
        let sub = self.setup.sub;
        let (enter, esc) = ctx.input(|i| (i.key_pressed(Key::Enter), i.key_pressed(Key::Escape)));
        let pressed = |k: Key| !typing && ctx.input(|i| i.key_pressed(k));

        widgets::caption(ui, &format!("STEP {} OF 6", sub.index() + 1));
        let mut go_next = false;

        match sub {
            SetupStep::Game => {
                widgets::heading(
                    ui,
                    "Which game are you tuning for?",
                    "Each game turns a different amount per mouse count, so ZeroIn needs to know.",
                );
                let total = ui.available_width().min(720.0);
                let cw = (total - 10.0) / 2.0;
                let mut picked = None;
                let mut dbl = false;
                for row in 0..(GAMES.len() + 2) / 2 {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        for col in 0..2 {
                            let i = row * 2 + col;
                            if i < GAMES.len() {
                                let g = &GAMES[i];
                                let tag = if g.confidence == Confidence::Estimate { "estimate" } else { "" };
                                let r = widgets::option_card_w(
                                    ui,
                                    &mut self.motion,
                                    crate::motion::id(("game", i)),
                                    Some(label_for(i)),
                                    g.name,
                                    "",
                                    tag,
                                    self.setup.game == Some(i),
                                    cw,
                                );
                                if r.clicked() {
                                    picked = Some(i);
                                }
                                dbl |= r.double_clicked();
                            } else if i == GAMES.len() {
                                let r = widgets::option_card_w(
                                    ui,
                                    &mut self.motion,
                                    crate::motion::id(("game", CUSTOM_IDX)),
                                    Some("O"),
                                    "Other / custom yaw",
                                    "",
                                    "",
                                    self.setup.game == Some(CUSTOM_IDX),
                                    cw,
                                );
                                if r.clicked() {
                                    picked = Some(CUSTOM_IDX);
                                }
                            }
                        }
                    });
                }
                for i in 0..GAMES.len() {
                    if key_for(i).is_some_and(pressed) {
                        picked = Some(i);
                    }
                }
                if pressed(Key::O) {
                    picked = Some(CUSTOM_IDX);
                }
                if let Some(i) = picked {
                    self.setup.game = Some(i);
                }
                if dbl {
                    go_next = true;
                }
                if let Some(g) = self.setup.game.and_then(|i| GAMES.get(i)) {
                    if !g.note.is_empty() {
                        widgets::dim(ui, format!("{}: {}", g.name, g.note));
                    }
                }
                if self.setup.game == Some(CUSTOM_IDX) {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label("Name");
                        ui.add(egui::TextEdit::singleline(&mut self.setup.custom_name).hint_text("My game").desired_width(180.0));
                        ui.label("Yaw");
                        ui.add(egui::TextEdit::singleline(&mut self.setup.custom_yaw).desired_width(100.0));
                    });
                    if self.setup.profile().is_none() {
                        widgets::dim(ui, "Yaw must be a positive number, e.g. 0.022 for Source-engine games.");
                    }
                }
            }
            SetupStep::Dpi => {
                widgets::heading(
                    ui,
                    "What DPI is your mouse set to?",
                    "Check your mouse software if you are unsure. 800 is the most common.",
                );
                let labels: Vec<String> = DPI_OPTIONS.iter().map(|d| format!("{d:.0}")).collect();
                let sel = if self.setup.dpi_text.trim().is_empty() { self.setup.dpi_choice } else { None };
                if let Some(i) = widgets::chip_row(ui, &mut self.motion, crate::motion::id("dpi_chips"), &labels, sel) {
                    self.setup.dpi_choice = Some(i);
                    self.setup.dpi_text.clear();
                }
                for i in 0..DPI_OPTIONS.len() {
                    if key_for(i).is_some_and(pressed) {
                        self.setup.dpi_choice = Some(i);
                        self.setup.dpi_text.clear();
                    }
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Other DPI").color(TEXT_DIM));
                    ui.add(egui::TextEdit::singleline(&mut self.setup.dpi_text).hint_text("e.g. 1000").desired_width(120.0));
                });
                if !self.setup.dpi_text.trim().is_empty() && self.setup.dpi().is_none() {
                    widgets::dim(ui, "DPI must be between 100 and 32000.");
                }
            }
            SetupStep::Sens => {
                self.setup.prefill_sens();
                let p = self.setup.profile();
                let name = p.as_ref().map(|p| p.name.clone()).unwrap_or_default();
                widgets::heading(
                    ui,
                    &format!("What is your current {name} sensitivity?"),
                    "Use the hip-fire (not scoped) value from the game's settings.",
                );
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.setup.sens_text)
                        .font(bold(30.0))
                        .desired_width(220.0)
                        .margin(egui::Margin::symmetric(14, 10)),
                );
                if self.setup.sens_text.is_empty() {
                    resp.request_focus();
                }
                if let Some(i) = self.setup.game.filter(|i| *i < GAMES.len()) {
                    if !GAMES[i].note.is_empty() {
                        widgets::dim(ui, GAMES[i].note);
                    }
                }
                ui.add_space(6.0);
                match (self.setup.sens(), self.setup.dpi(), p.as_ref()) {
                    (Some(s), Some(dpi), Some(p)) => {
                        let dpc = p.model.deg_per_count(s);
                        let cm = math::cm_per_360(dpi, dpc);
                        widgets::dim(
                            ui,
                            format!("{cm:.1} cm/360 · {:.1} in/360 · eDPI {:.0}", math::inches_per_360(dpi, dpc), math::edpi(dpi, s)),
                        );
                    }
                    _ => {
                        let (lo, hi) = self.setup.sens_range();
                        widgets::dim(ui, format!("Enter a number between {} and {}.", trim_float(lo, 4), trim_float(hi, 2)));
                    }
                }
            }
            SetupStep::Accel => {
                widgets::heading(
                    ui,
                    "Is “Enhance pointer precision” on?",
                    "Windows Settings → Bluetooth & devices → Mouse → Additional mouse settings → Pointer Options.",
                );
                let r1 = widgets::option_card(
                    ui,
                    &mut self.motion,
                    crate::motion::id("accel_off"),
                    Some("1"),
                    "Off",
                    "Recommended for aiming",
                    self.setup.accel == Some(false),
                );
                let r2 = widgets::option_card(
                    ui,
                    &mut self.motion,
                    crate::motion::id("accel_on"),
                    Some("2"),
                    "On",
                    "Windows mouse acceleration",
                    self.setup.accel == Some(true),
                );
                let r3 = widgets::option_card(
                    ui,
                    &mut self.motion,
                    crate::motion::id("accel_unsure"),
                    Some("3"),
                    "Not sure — use what Windows reports",
                    "",
                    false,
                );
                if r1.clicked() || pressed(Key::Num1) {
                    self.setup.accel = Some(false);
                }
                if r2.clicked() || pressed(Key::Num2) {
                    self.setup.accel = Some(true);
                }
                if (r3.clicked() || pressed(Key::Num3)) && self.mouse_settings.is_some() {
                    self.setup.accel = self.mouse_settings.map(|m| m.enhance_pointer_precision);
                }
                ui.add_space(6.0);
                self.accel_notices(ui);
            }
            SetupStep::Speed => {
                widgets::heading(
                    ui,
                    "Which Windows pointer speed do you use?",
                    "Same Pointer Options page. 10 is the default and the middle notch.",
                );
                if !self.setup.speed_touched {
                    if let Some(m) = self.mouse_settings {
                        self.setup.speed = m.pointer_speed;
                    }
                }
                let before = self.setup.speed;
                widgets::speed_slider(ui, &mut self.motion, crate::motion::id("speed"), &mut self.setup.speed);
                if !typing {
                    ctx.input(|i| {
                        if i.key_pressed(Key::ArrowLeft) {
                            self.setup.speed = self.setup.speed.saturating_sub(1).max(1);
                        }
                        if i.key_pressed(Key::ArrowRight) {
                            self.setup.speed = (self.setup.speed + 1).min(20);
                        }
                    });
                }
                if self.setup.speed != before {
                    self.setup.speed_touched = true;
                }
                widgets::key_hint(ui, &[("← →", "adjust")]);
                ui.add_space(6.0);
                self.speed_notices(ui);
            }
            SetupStep::Ready => {
                self.ready_view(ui);
            }
        }

        ui.add_space(20.0);
        let next_label = match sub {
            SetupStep::Game => "Next: pick your DPI",
            SetupStep::Dpi => "Next: your current sensitivity",
            SetupStep::Sens => "Next: pointer precision",
            SetupStep::Accel => "Next: pointer speed",
            SetupStep::Speed => "Next: review and check devices",
            SetupStep::Ready => "Start test",
        };
        let valid = self.setup.valid(sub);
        ui.horizontal(|ui| {
            if widgets::primary_button(ui, &mut self.motion, next_label, valid).clicked() {
                go_next = true;
            }
            if sub != SetupStep::Game {
                ui.add_space(8.0);
                if widgets::secondary_button(ui, &mut self.motion, "Back", Some(widgets::Icon::ArrowLeft)).clicked() {
                    self.setup.sub = sub.prev().unwrap_or(sub);
                }
            }
        });
        ui.add_space(10.0);
        let mut hints = vec![("Enter", "next")];
        if sub != SetupStep::Game {
            hints.push(("Esc", "back"));
        }
        if matches!(sub, SetupStep::Game | SetupStep::Dpi | SetupStep::Accel) {
            hints.push(("1–9", "choose"));
        }
        widgets::key_hint(ui, &hints);

        if enter && valid {
            go_next = true;
        }
        if esc {
            if let Some(p) = sub.prev() {
                self.setup.sub = p;
            }
        }
        if go_next && valid {
            match sub.next() {
                Some(n) => self.setup.sub = n,
                None => self.begin_test(),
            }
            if sub == SetupStep::Game || sub == SetupStep::Dpi {
                self.settings.last_game = self.setup.profile().map(|p| p.id).filter(|id| id != "custom");
                self.settings.last_dpi = self.setup.dpi();
                self.save_settings();
            }
        }
    }

    fn accel_notices(&mut self, ui: &mut egui::Ui) {
        if let Some(m) = self.mouse_settings {
            let win = if m.enhance_pointer_precision { "On" } else { "Off" };
            if let Some(a) = self.setup.accel {
                if a != m.enhance_pointer_precision {
                    widgets::notice(
                        ui,
                        Tone::Info,
                        &format!("Windows reports: {win}"),
                        "Your answer differs from the current Windows setting. ZeroIn records what Windows reports.",
                    );
                } else {
                    widgets::notice(ui, Tone::Ok, &format!("Windows reports: {win}"), "Matches your answer.");
                }
            } else {
                widgets::notice(ui, Tone::Info, &format!("Windows reports: {win}"), "");
            }
        } else {
            widgets::notice(ui, Tone::Info, "Could not read the Windows setting", "Answer from Pointer Options.");
        }
        let on = self.mouse_settings.map(|m| m.enhance_pointer_precision).or(self.setup.accel).unwrap_or(false);
        if on {
            widgets::notice(
                ui,
                Tone::Warn,
                "Acceleration is on",
                "The cursor travels further when you move the mouse faster, so the same hand motion does not always land in the same place. \
Games that use raw input (CS2, Valorant, Apex, Overwatch 2 and most modern shooters) ignore it, but the desktop, menus and games without raw input do not. \
ZeroIn's drills read raw input, so your result is not affected. Turning it off keeps muscle memory consistent everywhere.",
            );
        }
    }

    fn speed_notices(&mut self, ui: &mut egui::Ui) {
        if let Some(m) = self.mouse_settings {
            if m.pointer_speed != self.setup.speed {
                widgets::notice(
                    ui,
                    Tone::Info,
                    &format!("Windows reports: {}", m.pointer_speed),
                    "Your answer differs from the current Windows setting.",
                );
            } else {
                widgets::notice(ui, Tone::Ok, &format!("Windows reports: {}", m.pointer_speed), "Matches your answer.");
            }
        }
        let speed = self.mouse_settings.map(|m| m.pointer_speed).unwrap_or(self.setup.speed);
        if speed != 10 {
            let mult = math::windows_pointer_multiplier(speed);
            widgets::notice(
                ui,
                Tone::Warn,
                &format!("Pointer speed {speed} scales the cursor by {mult}×"),
                "At 10, one mouse count moves the cursor one pixel. Other values scale counts, and values below 10 skip some of them, which loses precision on the desktop and in games without raw input. \
Raw-input games and ZeroIn's drills ignore this setting.",
            );
        }
    }

    fn ready_view(&mut self, ui: &mut egui::Ui) {
        widgets::heading(
            ui,
            "Ready to test",
            "Check the summary, then move your mouse in circles for a moment to measure its polling rate.",
        );
        let p = self.setup.profile();
        let (dpi, sens) = (self.setup.dpi().unwrap_or(800.0), self.setup.sens().unwrap_or(1.0));
        widgets::card(ui, |ui| {
            ui.set_width(ui.available_width().min(620.0));
            egui::Grid::new("summary").num_columns(2).spacing([28.0, 8.0]).show(ui, |ui| {
                let row = |ui: &mut egui::Ui, k: &str, v: String| {
                    ui.label(egui::RichText::new(k).color(TEXT_DIM));
                    ui.label(egui::RichText::new(v).font(semibold(15.0)));
                    ui.end_row();
                };
                if let Some(p) = &p {
                    row(ui, "Game", p.name.clone());
                    row(ui, "Sensitivity", format!("{} at {dpi:.0} DPI", trim_float(sens, p.decimals as usize)));
                    row(ui, "cm/360", format!("{:.1} cm", math::cm_per_360(dpi, p.model.deg_per_count(sens))));
                }
                row(
                    ui,
                    "Pointer precision",
                    match self.mouse_settings.map(|m| m.enhance_pointer_precision).or(self.setup.accel) {
                        Some(true) => "On".into(),
                        Some(false) => "Off".into(),
                        None => "Unknown".into(),
                    },
                );
                row(ui, "Pointer speed", format!("{}", self.mouse_settings.map(|m| m.pointer_speed).unwrap_or(self.setup.speed)));
                row(
                    ui,
                    "Rounds",
                    format!(
                        "{} × 3 candidates ({})",
                        self.settings.rounds,
                        match self.settings.test_length {
                            zeroin_core::history::TestLength::Quick => "quick",
                            _ => "standard",
                        }
                    ),
                );
            });
        });
        ui.add_space(10.0);
        // Polling
        match (&self.polling.last, self.raw.is_native()) {
            (Some(s), _) => {
                let name = s.nominal_hz.map(|n| format!("{n} Hz")).unwrap_or_else(|| format!("{:.0} Hz", s.rate_hz));
                if s.is_stable() {
                    widgets::notice(
                        ui,
                        Tone::Ok,
                        &format!("Mouse polling: {name}"),
                        &format!("Measured {:.0} reports/s, {:.0}% of intervals steady.", s.rate_hz, s.stability * 100.0),
                    );
                } else {
                    widgets::notice(ui, Tone::Warn, &format!("Mouse polling: {name}, unstable"), &format!("Only {:.0}% of intervals were steady. Try another USB port (directly on the PC), close RGB/mouse software overlays, or lower the polling rate. 4000–8000 Hz needs a fast CPU.", s.stability * 100.0));
                }
            }
            (None, true) => {
                widgets::notice(ui, Tone::Info, "Mouse polling: move your mouse in circles", "A reading appears after about 200 reports.")
            }
            (None, false) => widgets::notice(ui, Tone::Info, "Mouse polling: unavailable", "Raw input is only available on Windows."),
        }
        if let Some(d) = self.display {
            if d.current_hz + 2 < d.max_hz {
                widgets::notice(
                    ui,
                    Tone::Warn,
                    &format!("Display runs at {} Hz but supports {} Hz", d.current_hz, d.max_hz),
                    "Windows Settings → System → Display → Advanced display → Choose a refresh rate. Drills render uncapped either way.",
                );
            } else {
                widgets::notice(ui, Tone::Ok, &format!("Display: {} Hz", d.current_hz), "Drills render uncapped with no vsync.");
            }
        }
        if p.as_ref().is_some_and(|p| p.confidence == Confidence::Estimate) {
            widgets::notice(ui, Tone::Info, "This game's constant is an estimate", "Published values disagree. The test still finds the right feel; only cross-game conversions involving this game are approximate.");
        }
        let st = self.llm_status();
        if !st.is_ready() {
            widgets::dim(ui, format!("{} — results use the built-in algorithm and summary only.", st.label()));
        }
        let _ = Step::Setup;
    }
}

//! Development-only layout harness (debug builds off Windows).
//!
//! `ZEROIN_DEV_SCREEN=<name>` opens a given screen or state with sample
//! data, `ZEROIN_DEV_SCROLL=bottom` (or a point offset) keeps the page scrolled, and
//! every frame reports the page's scroll metrics and any content wider than
//! the page column on stderr (`LAYOUT ...` lines), so screenshots at many
//! sizes can be checked automatically. Nothing here is in release builds.

use crate::app::{Section, Step, ZeroInApp};
use crate::screens::test::Phase;
use zeroin_core::drills::InvalidReason;
use zeroin_core::gear::{Feet, Gear, PadSpeed, PadSurface};
use zeroin_core::scoring::Score;

/// Screens the harness can open.
pub const SCREENS: &[&str] = &[
    "setup-0",
    "setup-1",
    "setup-2",
    "setup-3",
    "setup-4",
    "setup-5",
    "setup-6",
    "setup-7",
    "setup-resume",
    "modal-temp",
    "test-intro",
    "test-rerun",
    "test-paused",
    "test-incomplete",
    "result",
    "history",
    "history-open",
    "settings",
    "modal-apply",
    "check-mouse",
    "check-mouse-done",
];

fn fill_setup(app: &mut ZeroInApp) {
    app.setup.game = Some(0);
    app.setup.dpi_choice = Some(1);
    app.setup.sens_text = "0.4".into();
    app.setup.accel = Some(true);
    app.setup.speed = 14;
    app.setup.gear = Gear {
        pad_surface: Some(PadSurface::Cloth),
        pad_speed: Some(PadSpeed::Control),
        pad_name: "Artisan Zero Mid".into(),
        feet: Some(Feet::Ptfe),
    };
    app.settings.gear = app.setup.gear.clone();
}

/// Run a whole synthetic test through the real search and finish it.
fn synthetic_result(app: &mut ZeroInApp, seed: f64) {
    fill_setup(app);
    app.begin_test();
    if let Some(t) = app.test.as_mut() {
        let mut guard = 0;
        while !t.search.is_done() && guard < 40 {
            guard += 1;
            let m = t.search.next_mult().unwrap_or(1.0);
            let v2 = 60.0 - 40.0 * (m.ln() - 0.12 * seed).powi(2);
            t.search.record(m, Score { v2, total: v2, ..Default::default() });
            t.block_seconds.push(25.0);
        }
    }
    app.temp_mouse.during_test = Some(zeroin_core::winmouse::PointerDuringTest::LeftAsIs);
    app.finish_test();
}

/// Called once at start-up.
pub fn startup(app: &mut ZeroInApp) {
    let Ok(name) = std::env::var("ZEROIN_DEV_SCREEN") else { return };
    eprintln!("LAYOUT screen={name}");
    match name.as_str() {
        s if s.starts_with("setup-") && s != "setup-resume" => {
            fill_setup(app);
            let i: usize = s[6..].parse().unwrap_or(0);
            app.setup.sub = crate::screens::setup::SetupStep::from_index(i);
        }
        "setup-resume" => {
            fill_setup(app);
            app.begin_test();
            if let Some(t) = app.test.take() {
                app.saved_test = Some(t.to_saved());
            }
            app.step = Step::Setup;
        }
        "modal-temp" | "test-intro" | "test-rerun" | "test-paused" | "test-incomplete" => {
            fill_setup(app);
            app.begin_test();
            if let Some(t) = app.test.as_mut() {
                t.phase = match name.as_str() {
                    "test-rerun" => Phase::Rerun(InvalidReason::InputGap),
                    "test-paused" => Phase::Paused,
                    "test-incomplete" => Phase::Incomplete(InvalidReason::InputGap),
                    _ => Phase::Intro,
                };
                if name == "test-rerun" {
                    t.invalid_blocks = 1;
                }
            }
            if name == "modal-temp" {
                app.temp_mouse.prompt = true;
            }
        }
        "result" => synthetic_result(app, 1.0),
        "history" | "history-open" => {
            for k in 0..4 {
                synthetic_result(app, k as f64 - 1.5);
                if k % 2 == 1 {
                    if let Some(s) = app.history.sessions.last_mut() {
                        s.gear = None;
                    }
                }
            }
            app.section = Section::History;
            if name == "history-open" {
                if let Some(id) = app.history.sessions.first().map(|s| s.id.clone()) {
                    app.dev_open_history_row = Some(id);
                }
            }
        }
        "settings" => app.section = Section::Settings,
        "modal-apply" => {
            app.section = Section::Settings;
            app.request_apply_neutral();
        }
        "check-mouse" => {
            app.section = Section::Settings;
            app.mouse_check.open = true;
        }
        "check-mouse-done" => {
            app.section = Section::Settings;
            app.mouse_check.open = true;
            app.mouse_check.claim = 6;
            // The mock capture runs its normal five seconds; shoot after that.
            if let Ok(check) = crate::platform::PollCheck::start() {
                app.mouse_check.phase = crate::screens::mousecheck::CheckPhase::Running { check, started_us: crate::platform::now_us() };
            }
        }
        _ => eprintln!("LAYOUT unknown screen; known: {}", SCREENS.join(", ")),
    }
}

/// Called after every frame's layout.
pub fn after_frame(app: &mut ZeroInApp, ctx: &eframe::egui::Context) {
    if std::env::var("ZEROIN_DEV_SCREEN").is_err() {
        return;
    }
    let frame = ctx.cumulative_frame_nr();
    if frame > 5 {
        match std::env::var("ZEROIN_DEV_SCROLL").ok().as_deref() {
            Some("bottom") => app.scroll_to_end(),
            Some(v) => {
                if let Ok(y) = v.parse::<f32>() {
                    app.scroll_to(y);
                }
            }
            None => {}
        }
    }
    if frame % 30 == 10 {
        let (off, view, content) = app.scroll_metrics();
        let over = app.layout_overflow.take();
        eprintln!(
            "LAYOUT page={} zoom={:.2} ppp={:.2} window={:.0}x{:.0} scroll={off:.0} view={view:.0} content={content:.0} overflow={}",
            app.page_name(),
            ctx.zoom_factor(),
            ctx.pixels_per_point(),
            ctx.content_rect().width(),
            ctx.content_rect().height(),
            over.map(|(p, px)| format!("{px:.1}px on {p}")).unwrap_or_else(|| "none".into())
        );
    }
    ctx.request_repaint();
}

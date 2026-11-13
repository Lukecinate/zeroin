//! Check mouse: real polling-rate measurement and device identity, reached
//! from Settings. Every figure comes from the reports captured during the
//! check; anything that cannot be measured says so and why.

use crate::app::ZeroInApp;
use crate::platform::{self, CapReport, DeviceIdentity, PollCheck};
use crate::screens::setup::key_for;
use crate::theme::*;
use crate::widgets::{self, Tone};
use eframe::egui::{self, Align2, Key, Pos2, Rect, Stroke};
use zeroin_core::polling::{self, CheckReport, Report, Verdict};
use zeroin_core::usbid;

/// Length of one capture.
pub const CHECK_SECONDS: f64 = 5.0;
/// Rates offered as "set in the mouse software".
const CLAIMS: [Option<u32>; 7] = [None, Some(125), Some(500), Some(1000), Some(2000), Some(4000), Some(8000)];

pub enum CheckPhase {
    Idle,
    Running { check: PollCheck, started_us: u64 },
    Done(Box<CheckDone>),
}

pub struct CheckDone {
    /// Reports of the device that moved most.
    pub reports: Vec<Report>,
    pub analysis: Result<CheckReport, String>,
    pub identity: Option<DeviceIdentity>,
    /// Devices that sent motion during the check.
    pub devices: usize,
}

pub struct MouseCheck {
    pub open: bool,
    pub claim: usize,
    pub phase: CheckPhase,
}

impl Default for MouseCheck {
    fn default() -> Self {
        MouseCheck { open: false, claim: 0, phase: CheckPhase::Idle }
    }
}

impl MouseCheck {
    fn claimed(&self) -> Option<u32> {
        CLAIMS.get(self.claim).copied().flatten()
    }

    pub fn is_running(&self) -> bool {
        matches!(self.phase, CheckPhase::Running { .. })
    }

    /// Stop a running capture without keeping its result. The capture thread
    /// is joined, which hands mouse Raw Input back to the main input thread.
    /// Returns true when a capture was running.
    pub fn cancel_running(&mut self) -> bool {
        if !self.is_running() {
            return false;
        }
        if let CheckPhase::Running { check, .. } = std::mem::replace(&mut self.phase, CheckPhase::Idle) {
            let _ = check.finish();
        }
        true
    }

    /// A capture may only run while its screen is on display.
    pub fn must_stop(&self, on_screen: bool) -> bool {
        self.is_running() && !on_screen
    }
}

fn finish_capture(caps: Vec<CapReport>, claimed: Option<u32>) -> CheckDone {
    let mut counts: Vec<(u64, usize)> = Vec::new();
    for c in &caps {
        match counts.iter_mut().find(|(d, _)| *d == c.device) {
            Some((_, n)) => *n += 1,
            None => counts.push((c.device, 1)),
        }
    }
    let device = counts.iter().max_by_key(|(_, n)| *n).map(|(d, _)| *d);
    let reports: Vec<Report> = caps
        .iter()
        .filter(|c| Some(c.device) == device)
        .map(|c| Report { t_ns: c.t_ns, dx: c.dx, dy: c.dy, batch: c.batch.max(1) })
        .collect();
    let analysis = polling::check(&reports, claimed).map_err(|e| e.message());
    let identity = device.and_then(platform::device_identity);
    CheckDone { reports, analysis, identity, devices: counts.len() }
}

fn verdict_color(v: Verdict) -> egui::Color32 {
    match v {
        Verdict::Good => OK,
        Verdict::Ok => WARN,
        Verdict::Problem => CORAL,
    }
}

fn tone(v: Verdict) -> Tone {
    match v {
        Verdict::Good => Tone::Ok,
        Verdict::Ok => Tone::Info,
        Verdict::Problem => Tone::Warn,
    }
}

fn fmt_us(v: Option<f64>) -> String {
    match v {
        Some(x) if x >= 1000.0 => format!("{:.2} ms", x / 1000.0),
        Some(x) => format!("{x:.0} µs"),
        None => "not measurable".into(),
    }
}

impl ZeroInApp {
    pub fn mouse_check_screen(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let typing = ctx.egui_wants_keyboard_input();
        let (enter, esc) = ctx.input(|i| (i.key_pressed(Key::Enter), i.key_pressed(Key::Escape)));
        let running = matches!(self.mouse_check.phase, CheckPhase::Running { .. });

        if widgets::secondary_button(ui, &mut self.motion, "Settings", Some(widgets::Icon::ArrowLeft)).clicked() || (esc && !running) {
            self.mouse_check.open = false;
            self.mouse_check.phase = CheckPhase::Idle;
            return;
        }
        ui.add_space(10.0);
        widgets::heading(
            ui,
            "Check mouse",
            "Measures the polling rate your mouse really delivers, from the reports Windows hands to ZeroIn. Nothing is sent anywhere.",
        );
        let w = ui.available_width();

        // Rate set in the mouse software.
        widgets::caption(ui, "RATE SET IN YOUR MOUSE SOFTWARE");
        let labels: Vec<String> = CLAIMS.iter().map(|c| c.map(|v| format!("{v} Hz")).unwrap_or_else(|| "Not sure".into())).collect();
        let mut pick = widgets::chip_row(ui, &mut self.motion, crate::motion::id("claim_chips"), &labels, Some(self.mouse_check.claim));
        if !typing && !running {
            for i in 0..CLAIMS.len() {
                if key_for(i).is_some_and(|k| ctx.input(|inp| inp.key_pressed(k))) {
                    pick = Some(i);
                }
            }
        }
        if let Some(i) = pick {
            if i != self.mouse_check.claim {
                self.mouse_check.claim = i;
                let claimed = self.mouse_check.claimed();
                if let CheckPhase::Done(d) = &mut self.mouse_check.phase {
                    d.analysis = polling::check(&d.reports, claimed).map_err(|e| e.message());
                }
            }
        }
        ui.add_space(12.0);

        // Advance a running capture.
        let mut finished: Option<Result<Vec<CapReport>, String>> = None;
        let mut cancel = false;
        if let CheckPhase::Running { check, started_us } = &self.mouse_check.phase {
            let elapsed = platform::now_us().saturating_sub(*started_us) as f64 / 1e6;
            let frac = (elapsed / CHECK_SECONDS).clamp(0.0, 1.0) as f32;
            widgets::card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new("Move the mouse quickly in wide circles").font(semibold(16.0)));
                widgets::dim(ui, format!("{} reports so far · {:.1} s left", check.count(), (CHECK_SECONDS - elapsed).max(0.0)));
                ui.add_space(8.0);
                crate::screens::test::budget_bar(ui, 0, 1, frac, w - 40.0);
                ui.add_space(4.0);
                widgets::key_hint(ui, &[("Esc", "cancel")]);
            });
            ctx.request_repaint();
            if esc {
                cancel = true;
            } else if elapsed >= CHECK_SECONDS {
                finished = Some(Ok(Vec::new()));
            }
        }
        if cancel || finished.is_some() {
            if let CheckPhase::Running { check, .. } = std::mem::replace(&mut self.mouse_check.phase, CheckPhase::Idle) {
                let r = check.finish();
                if !cancel {
                    let claimed = self.mouse_check.claimed();
                    self.mouse_check.phase = match r {
                        Ok(caps) => CheckPhase::Done(Box::new(finish_capture(caps, claimed))),
                        Err(e) => {
                            CheckPhase::Done(Box::new(CheckDone { reports: Vec::new(), analysis: Err(e), identity: None, devices: 0 }))
                        }
                    };
                }
            }
            return;
        }

        let mut start = false;
        match &self.mouse_check.phase {
            CheckPhase::Running { .. } => {}
            CheckPhase::Idle => {
                widgets::dim(
                    ui,
                    format!(
                        "Press Start, then keep moving the mouse quickly in wide circles for the whole {CHECK_SECONDS:.0} seconds. Slow movement cannot show high polling rates."
                    ),
                );
                ui.add_space(8.0);
                if widgets::primary_button(ui, &mut self.motion, "Start check", true).clicked() || (enter && !typing) {
                    start = true;
                }
                widgets::key_hint(ui, &[("Enter", "start"), ("1–7", "rate set"), ("Esc", "back")]);
            }
            CheckPhase::Done(_) => {
                if widgets::primary_button(ui, &mut self.motion, "Run again", true).clicked() || (enter && !typing) {
                    start = true;
                }
                ui.add_space(12.0);
            }
        }
        if !start && matches!(self.mouse_check.phase, CheckPhase::Done(_)) {
            if let CheckPhase::Done(d) = std::mem::replace(&mut self.mouse_check.phase, CheckPhase::Idle) {
                let eloshapes = self.mouse_check_results(ui, &d, w);
                self.mouse_check.phase = CheckPhase::Done(d);
                if eloshapes && !platform::open_https(usbid::ELOSHAPES_BROWSE_URL) {
                    self.toast("Could not open the browser.");
                }
            }
        }
        if start {
            match PollCheck::start() {
                Ok(check) => self.mouse_check.phase = CheckPhase::Running { check, started_us: platform::now_us() },
                Err(e) => {
                    self.mouse_check.phase =
                        CheckPhase::Done(Box::new(CheckDone { reports: Vec::new(), analysis: Err(e), identity: None, devices: 0 }))
                }
            }
        }
    }

    /// Draw a finished check. Returns true when the EloShapes link was clicked.
    fn mouse_check_results(&mut self, ui: &mut egui::Ui, d: &CheckDone, w: f32) -> bool {
        let mut link = false;
        if d.identity.as_ref().is_some_and(|i| i.mock) {
            widgets::notice(
                ui,
                Tone::Warn,
                "Development mock data",
                "These figures come from a synthetic stream (ZEROIN_DEV_POLL), not from a real device.",
            );
            ui.add_space(6.0);
        }
        // Device identity.
        widgets::caption(ui, "DEVICE");
        widgets::card(ui, |ui| {
            ui.set_width(ui.available_width());
            match &d.identity {
                Some(id) => {
                    let ids = usbid::parse_device_path(&id.path);
                    let title = id.product.clone().unwrap_or_else(|| "Name not reported by the device".into());
                    ui.label(egui::RichText::new(&title).font(semibold(16.0)));
                    let mut lines = Vec::new();
                    if let Some(m) = &id.manufacturer {
                        lines.push(format!("Manufacturer string: {m}"));
                    }
                    match ids {
                        Some(u) => {
                            let vendor = usbid::vendor(u.vid).map(|v| match v.kind {
                                usbid::VendorKind::Brand => v.name.to_string(),
                                usbid::VendorKind::ChipMaker => {
                                    format!("{} (chip maker used by many brands; the brand may differ)", v.name)
                                }
                            });
                            lines.push(format!(
                                "{} {} · vendor {}",
                                if u.bluetooth { "Bluetooth" } else { "USB" },
                                u.hex(),
                                vendor.unwrap_or_else(|| "not in ZeroIn's vendor table".into())
                            ));
                        }
                        None if !id.mock => lines.push("Vendor and product id: not present in the device path.".into()),
                        None => {}
                    }
                    for l in lines {
                        widgets::dim(ui, l);
                    }
                    if id.product.as_deref().is_some_and(usbid::looks_like_receiver) {
                        widgets::dim(
                            ui,
                            "This is the wireless receiver's name. The receiver does not report which mouse is paired with it.",
                        );
                    }
                    if d.devices > 1 {
                        widgets::dim(ui, format!("{} pointing devices sent motion; the one that moved most was measured.", d.devices));
                    }
                    if !id.mock {
                        ui.add_space(6.0);
                        if widgets::secondary_button(ui, &mut self.motion, "View specs on EloShapes", None).clicked() {
                            link = true;
                        }
                        widgets::dim(
                            ui,
                            format!(
                                "Opens eloshapes.com in your browser; search there for {}. ZeroIn does not fetch or store anything from it.",
                                id.product.as_deref().map(|p| format!("“{p}”")).unwrap_or_else(|| "your model".into())
                            ),
                        );
                    }
                }
                None => {
                    widgets::dim(ui, "Device name: not measurable here (needs Windows Raw Input and HID access).");
                }
            }
        });
        ui.add_space(12.0);

        let r = match &d.analysis {
            Ok(r) => r,
            Err(e) => {
                widgets::notice(ui, Tone::Warn, "No result", e);
                return link;
            }
        };
        // Verdict.
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(r.verdict.label()).font(bold(34.0)).color(verdict_color(r.verdict)));
            ui.add_space(14.0);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(format!("{:.0} Hz effective", r.effective_hz)).font(semibold(18.0)));
                widgets::dim(ui, format!("{} reports · {:.1} s of fast movement analysed", r.reports, r.fast_seconds));
            });
        });
        if r.gimmick_suspect {
            widgets::notice(
                ui,
                Tone::Warn,
                "High-rate mode not delivered",
                "The rate that is set is far from what arrives, and neither slow movement nor the PC explains it. See the findings below for how to rule out the port.",
            );
        }
        ui.add_space(8.0);
        for f in &r.findings {
            let body = if f.level == Verdict::Good { f.detail.clone() } else { format!("{} {}.", f.detail, f.source.label()) };
            widgets::notice(ui, tone(f.level), &f.title, &body);
        }
        ui.add_space(10.0);

        // Figures.
        widgets::caption(ui, "MEASURED");
        widgets::card(ui, |ui| {
            ui.set_width(ui.available_width());
            widgets::kv_rows(
                ui,
                &[
                    ("Effective rate (reports per second)", format!("{:.0} Hz", r.effective_hz)),
                    ("Rate from median interval", r.interval_hz.map(|h| format!("{h:.0} Hz")).unwrap_or_else(|| "not measurable".into())),
                    ("Nearest standard rate", r.standard_hz.map(|h| format!("{h} Hz")).unwrap_or_else(|| "none within 12 %".into())),
                    ("Interval 1st / 99th percentile", format!("{} / {}", fmt_us(r.p1_us), fmt_us(r.p99_us))),
                    ("Jitter (std / MAD)", format!("{} / {}", fmt_us(r.std_us), fmt_us(r.mad_us))),
                    (
                        "Skipped polls",
                        r.skipped_frac
                            .map(|s| format!("{:.1} %", s * 100.0))
                            .unwrap_or_else(|| "not measurable (timing too uneven to tell skips from jitter)".into()),
                    ),
                    ("Bunched reports (Windows batched)", format!("{:.1} % ({:.1} %)", r.bunched_frac * 100.0, r.batched_frac * 100.0)),
                    ("Reports with only 1 count", format!("{:.0} %", r.low_count_frac * 100.0)),
                    ("Rate stability over time (CV)", format!("{:.1} %", r.rate_cv * 100.0)),
                ],
            );
        });
        ui.add_space(10.0);
        widgets::caption(ui, "RATE OVER TIME");
        rate_chart(ui, r, w);
        ui.add_space(10.0);
        widgets::caption(ui, "INTERVALS");
        histogram(ui, r, w);
        ui.add_space(10.0);
        widgets::dim(
            ui,
            "Limits: times are when Windows handed each report to ZeroIn (high-resolution performance counter), not when the mouse sent it. At high rates Windows can hand over several reports in one read; ZeroIn counts reports for the rate and leaves grouped reports out of the interval figures. Wireless and USB timing on the device side cannot be seen from Windows.",
        );
        link
    }
}

fn rate_chart(ui: &mut egui::Ui, r: &CheckReport, w: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 120.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, radius(R_CTRL), SURFACE);
    if r.windows.len() < 2 {
        return;
    }
    let inner = rect.shrink2(egui::vec2(48.0, 14.0));
    let max_rate = r.windows.iter().map(|w| w.rate_hz).fold(r.effective_hz, f64::max) * 1.15;
    let t_max = r.windows.last().map(|w| w.t_s).unwrap_or(1.0).max(0.1);
    let pt = |t: f64, v: f64| {
        Pos2::new(inner.left() + (t / t_max) as f32 * inner.width(), inner.bottom() - (v / max_rate) as f32 * inner.height())
    };
    // Effective-rate guide.
    let y = pt(0.0, r.effective_hz).y;
    p.line_segment([Pos2::new(inner.left(), y), Pos2::new(inner.right(), y)], Stroke::new(1.0, BORDER));
    p.text(Pos2::new(rect.left() + 8.0, y), Align2::LEFT_CENTER, format!("{:.0}", r.effective_hz), regular(10.5), TEXT_FAINT);
    for pair in r.windows.windows(2) {
        let col = if pair[1].fast { TEXT } else { TEXT_FAINT };
        p.line_segment([pt(pair[0].t_s, pair[0].rate_hz), pt(pair[1].t_s, pair[1].rate_hz)], Stroke::new(1.5, col));
    }
    p.text(Pos2::new(inner.right(), rect.bottom() - 4.0), Align2::RIGHT_BOTTOM, format!("{t_max:.1} s"), regular(10.5), TEXT_FAINT);
    p.text(
        Pos2::new(inner.left(), rect.top() + 4.0),
        Align2::LEFT_TOP,
        "Bright: fast movement (used). Dim: slower movement.",
        regular(10.5),
        TEXT_FAINT,
    );
}

fn histogram(ui: &mut egui::Ui, r: &CheckReport, w: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 120.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, radius(R_CTRL), SURFACE);
    let total: u32 = r.hist.iter().sum();
    if total == 0 {
        p.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "Interval histogram: not measurable (too few separately delivered reports).",
            regular(12.0),
            TEXT_DIM,
        );
        return;
    }
    let inner = rect.shrink2(egui::vec2(16.0, 22.0));
    let n = r.hist.len();
    let bw = inner.width() / n as f32;
    let max = *r.hist.iter().max().unwrap_or(&1) as f32;
    for (i, c) in r.hist.iter().enumerate() {
        let h = (*c as f32 / max) * inner.height();
        let x = inner.left() + i as f32 * bw;
        let col = if i == 10 { CORAL } else { TEXT_DIM };
        p.rect_filled(Rect::from_min_max(Pos2::new(x + 1.0, inner.bottom() - h), Pos2::new(x + bw - 1.0, inner.bottom())), radius(1), col);
    }
    let label = |b: usize| fmt_us(Some(b as f64 * r.hist_bin_us));
    p.text(Pos2::new(inner.left(), rect.bottom() - 4.0), Align2::LEFT_BOTTOM, "0", regular(10.5), TEXT_FAINT);
    p.text(
        Pos2::new(inner.left() + 10.5 * bw, rect.bottom() - 4.0),
        Align2::CENTER_BOTTOM,
        format!("{} expected", label(10)),
        regular(10.5),
        TEXT_FAINT,
    );
    p.text(Pos2::new(inner.right(), rect.bottom() - 4.0), Align2::RIGHT_BOTTOM, format!("{}+", label(30)), regular(10.5), TEXT_FAINT);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaving_the_screen_stops_a_running_capture() {
        // The development mock stands in for Raw Input off Windows.
        std::env::set_var("ZEROIN_DEV_POLL", "1000:0.02");
        let mut mc = MouseCheck { open: true, ..Default::default() };
        mc.phase = CheckPhase::Running { check: PollCheck::start().expect("mock capture"), started_us: 0 };
        assert!(!mc.must_stop(true), "keeps running while shown");
        assert!(mc.must_stop(false), "must stop once the user navigates away");
        assert!(mc.cancel_running());
        assert!(matches!(mc.phase, CheckPhase::Idle));
        assert!(!mc.cancel_running(), "second cancel is a no-op");
        assert!(!mc.must_stop(false));
    }
}

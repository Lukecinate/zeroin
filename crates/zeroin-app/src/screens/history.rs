//! History: past results stored locally in %APPDATA%\ZeroIn\history.json.

use crate::app::ZeroInApp;
use crate::screens::setup::trim_float;
use crate::theme::*;
use crate::widgets::{self, Icon, Tone};
use eframe::egui::{self, Align2, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

/// Date and time in the PC's local time zone (UTC where it cannot be read).
pub fn format_date(unix: u64) -> String {
    format_date_with(unix, crate::platform::local_offset_secs(unix))
}

/// Civil date from a Unix timestamp shifted by `offset` seconds; without an
/// offset the time is shown as UTC and labelled so.
pub fn format_date_with(unix: u64, offset: Option<i64>) -> String {
    let (unix, suffix) = match offset {
        Some(o) => ((unix as i64 + o).max(0) as u64, ""),
        None => (unix, " UTC"),
    };
    let days = (unix / 86_400) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let secs = unix % 86_400;
    format!("{d} {} {y}, {:02}:{:02}{suffix}", MONTHS[(m - 1) as usize], secs / 3600, (secs % 3600) / 60)
}

impl ZeroInApp {
    pub fn history_screen(&mut self, ui: &mut egui::Ui) {
        widgets::heading(ui, "History", "Results are saved on this PC only.");
        if let Some(e) = self.history_error.clone() {
            widgets::notice(ui, Tone::Warn, "History reset", &e);
            ui.add_space(8.0);
        }
        if self.history.sessions.is_empty() {
            widgets::dim(ui, "No results yet. Finish a test in Find my sens and it appears here.");
            return;
        }
        let open_id = ui.ctx().data(|d| d.get_temp::<String>(crate::motion::id("hist_open")));
        let confirm_id = ui.ctx().data(|d| d.get_temp::<String>(crate::motion::id("hist_confirm")));
        let mut toggle: Option<String> = None;
        let mut delete: Option<String> = None;
        let mut ask: Option<String> = None;
        let w = ui.available_width().min(680.0);
        for s in self.history.sessions.iter().rev() {
            let open = open_id.as_deref() == Some(s.id.as_str());
            let t = self.motion.value(crate::motion::id(("hist_open", &s.id)), if open { 1.0 } else { 0.0 });
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 68.0), Sense::click());
            let hover = self.motion.bool(crate::motion::id(("hist_h", &s.id)), resp.hovered());
            ui.painter().rect_filled(rect, radius(R_CTRL), crate::motion::lerp_color(SURFACE, SURFACE_HI, hover.max(t * 0.5)));
            ui.painter().rect_stroke(rect, radius(R_CTRL), Stroke::new(1.0, BORDER), StrokeKind::Inside);
            let dec = s.game.decimals as usize;
            ui.painter().text(rect.left_center() + egui::vec2(18.0, -11.0), Align2::LEFT_CENTER, &s.game.name, semibold(15.0), TEXT);
            ui.painter().text(
                rect.left_center() + egui::vec2(18.0, 12.0),
                Align2::LEFT_CENTER,
                format!("{} · {:.0} DPI", format_date(s.created_unix), s.dpi),
                regular(12.5),
                TEXT_DIM,
            );
            ui.painter().text(
                rect.right_center() + egui::vec2(-48.0, -9.0),
                Align2::RIGHT_CENTER,
                trim_float(s.recommended_sens, dec),
                bold(22.0),
                CORAL,
            );
            ui.painter().text(
                rect.right_center() + egui::vec2(-48.0, 14.0),
                Align2::RIGHT_CENTER,
                format!("{:.1} cm/360", s.cm360),
                regular(12.0),
                TEXT_DIM,
            );
            let chev = Rect::from_center_size(rect.right_center() - egui::vec2(22.0, 0.0), Vec2::splat(16.0));
            widgets::paint_icon(ui, if open { Icon::ArrowLeft } else { Icon::ArrowRight }, chev, TEXT_FAINT);
            if resp.clicked() {
                toggle = Some(s.id.clone());
            }
            resp.on_hover_cursor(egui::CursorIcon::PointingHand);
            if t > 0.01 {
                let full_h = 230.0;
                let (area, _) = ui.allocate_exact_size(egui::vec2(w, full_h * t), Sense::hover());
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area.shrink2(egui::vec2(18.0, 6.0))));
                child.set_clip_rect(area);
                child.set_opacity(t);
                child.label(egui::RichText::new(&s.explanation).color(TEXT));
                child.add_space(4.0);
                child.label(
                    egui::RichText::new(format!(
                        "Started at {}{} · {} blocks{}",
                        trim_float(s.start_sens, dec),
                        match (s.keep, s.band) {
                            (true, Some((lo, hi))) => format!(" · kept · equally good {}–{}", trim_float(lo, dec), trim_float(hi, dec)),
                            (false, Some((lo, hi))) => format!(" · equally good {}–{}", trim_float(lo, dec), trim_float(hi, dec)),
                            _ => String::new(),
                        },
                        s.observations.len(),
                        s.windows_mouse
                            .map(|m| format!(
                                " · pointer speed {} · precision {}",
                                m.pointer_speed,
                                if m.enhance_pointer_precision { "on" } else { "off" }
                            ))
                            .unwrap_or_default()
                    ))
                    .font(regular(12.5))
                    .color(TEXT_DIM),
                );
                child.add_space(6.0);
                let confirming = confirm_id.as_deref() == Some(s.id.as_str());
                if widgets::secondary_button(
                    &mut child,
                    &mut self.motion,
                    if confirming { "Click again to delete" } else { "Delete" },
                    Some(Icon::Trash),
                )
                .clicked()
                {
                    if confirming {
                        delete = Some(s.id.clone());
                    } else {
                        ask = Some(s.id.clone());
                    }
                }
            }
            ui.add_space(6.0);
        }
        let _ = Pos2::ZERO;
        ui.ctx().data_mut(|d| {
            if let Some(id) = toggle {
                if open_id.as_deref() == Some(id.as_str()) {
                    d.remove::<String>(crate::motion::id("hist_open"));
                } else {
                    d.insert_temp(crate::motion::id("hist_open"), id);
                }
                d.remove::<String>(crate::motion::id("hist_confirm"));
            }
            if let Some(id) = ask {
                d.insert_temp(crate::motion::id("hist_confirm"), id);
            }
        });
        if let Some(id) = delete {
            self.history.remove(&id);
            self.save_history();
            self.toast("Result deleted");
            ui.ctx().data_mut(|d| d.remove::<String>(crate::motion::id("hist_confirm")));
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn dates() {
        use super::format_date_with as f;
        assert_eq!(f(0, None), "1 Jan 1970, 00:00 UTC");
        assert_eq!(f(1_794_268_800, None), "10 Nov 2026, 00:00 UTC");
        // 11:36 UTC is 18:36 in Jakarta (UTC+7); offsets can cross midnight.
        assert_eq!(f(1_794_268_800 + 11 * 3600 + 36 * 60, Some(7 * 3600)), "10 Nov 2026, 18:36");
        assert_eq!(f(1_794_268_800 + 20 * 3600, Some(7 * 3600)), "11 Nov 2026, 03:00");
        assert_eq!(f(1_794_268_800 + 3600, Some(-5 * 3600)), "9 Nov 2026, 20:00");
    }
}

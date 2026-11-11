//! Interface scale and page layout rules, kept free of app state so they can
//! be tested without a window.

/// Scale choices offered in Settings, in percent. `None` in the settings is Auto.
pub const SCALE_CHOICES: [u32; 5] = [90, 100, 110, 125, 150];

/// Automatic interface zoom on top of the display's DPI setting.
///
/// `w` and `h` are the window size in Windows' logical pixels (after the
/// display scale, before ZeroIn's zoom). The design size is 1400 × 860; the
/// zoom follows the smaller ratio, from 0.85 (small laptop windows at 150 %)
/// to 2.0 (a 55" 4K TV at 100 %), in steps of 5 %.
pub fn auto_zoom(w: f32, h: f32) -> f32 {
    let z = (w / 1400.0).min(h / 860.0);
    if !z.is_finite() {
        return 1.0;
    }
    (z.clamp(0.85, 2.0) * 20.0).round() / 20.0
}

/// Zoom for the setting: a fixed percentage (clamped to 90–150) or Auto.
/// A fixed percentage is lowered when the window is too small for it, so the
/// page keeps at least 900 × 560 points.
pub fn zoom_for(setting: Option<u32>, w: f32, h: f32) -> f32 {
    match setting {
        Some(p) => {
            let fit = (w / 900.0).min(h / 560.0);
            let fit = if fit.is_finite() { fit.max(0.75) } else { 1.0 };
            (p.clamp(90, 150) as f32 / 100.0).min(fit)
        }
        None => auto_zoom(w, h),
    }
}

/// Rail width for a window `width` points wide.
pub fn rail_width(width: f32) -> f32 {
    if width < 1000.0 {
        196.0
    } else {
        228.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets;
    use eframe::egui;

    /// The four test resolutions at four Windows scales.
    const SIZES: [(f32, f32); 4] = [(1366.0, 768.0), (1920.0, 1080.0), (2560.0, 1440.0), (3840.0, 2160.0)];
    const SCALES: [f32; 4] = [1.0, 1.25, 1.5, 2.0];

    #[test]
    fn auto_zoom_examples() {
        assert_eq!(auto_zoom(1366.0, 768.0), 0.9);
        assert_eq!(auto_zoom(1920.0, 1080.0), 1.25);
        assert_eq!(auto_zoom(2560.0, 1440.0), 1.65);
        assert_eq!(auto_zoom(3840.0, 2160.0), 2.0);
        assert_eq!(auto_zoom(3840.0 / 2.0, 2160.0 / 2.0), 1.25);
        assert_eq!(auto_zoom(1366.0 / 1.5, 768.0 / 1.5), 0.85);
        assert_eq!(auto_zoom(f32::NAN, f32::NAN), 1.0);
        assert_eq!(zoom_for(Some(200), 1920.0, 1080.0), 1.5);
        assert_eq!(zoom_for(Some(50), 1920.0, 1080.0), 0.9);
        assert_eq!(zoom_for(Some(110), 1920.0, 1080.0), 1.1);
        // Too small for 150 %: lowered to fit.
        assert!(zoom_for(Some(150), 1366.0 / 1.5, 768.0 / 1.5) < 1.0);
    }

    #[test]
    fn every_size_leaves_a_usable_page() {
        for (pw, ph) in SIZES {
            for s in SCALES {
                for setting in [None, Some(90), Some(150)] {
                    let (lw, lh) = (pw / s, ph / s);
                    let z = zoom_for(setting, lw, lh);
                    // Window size in points after the zoom.
                    let (w, h) = (lw / z, lh / z);
                    let main = w - rail_width(w);
                    let (x, col) = widgets::page_column(rail_width(w), main);
                    // Auto keeps a comfortable column; a manual 150 % on a small screen may be narrower.
                    let min_col = if setting.is_none() { 380.0 } else { 300.0 };
                    assert!(col >= min_col, "{pw}x{ph}@{s} {setting:?}: column {col}");
                    assert!(x >= rail_width(w) && x + col <= w + 0.01);
                    // Centred: equal space either side.
                    let left = x - rail_width(w);
                    let right = w - (x + col);
                    assert!((left - right).abs() < 0.5);
                    assert!(h >= 300.0, "{pw}x{ph}@{s}: height {h}");
                }
            }
        }
    }

    #[test]
    fn page_column_is_capped_and_centred() {
        let (x, w) = widgets::page_column(200.0, 3000.0);
        assert_eq!(w, widgets::PAGE_MAX_W);
        assert!((x - (200.0 + (3000.0 - w) / 2.0)).abs() < 0.01);
        let (_, w) = widgets::page_column(0.0, 500.0);
        assert!(w <= 460.0);
    }

    /// Lay out `add` in a `width`-point column and return the width it used.
    fn used_width(width: f32, add: impl FnOnce(&mut egui::Ui)) -> f32 {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width + 200.0, 2000.0))),
            ..Default::default()
        };
        let mut add = Some(add);
        let mut used: f32 = 0.0;
        // Two passes: fonts are ready from the second frame.
        for _ in 0..2 {
            let mut out = ctx.run_ui(raw.clone(), |ui| {
                let rect = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(width, 1900.0));
                let r = ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    if let Some(f) = add.take() {
                        f(ui);
                    } else {
                        ui.label("second pass");
                    }
                });
                used = used.max(r.response.rect.width());
            });
            out.textures_delta.clear();
        }
        used
    }

    #[test]
    fn shared_widgets_stay_inside_a_narrow_column() {
        let narrow = 380.0;
        let mut m = crate::motion::Motion::default();
        let labels: Vec<String> = ["Not sure", "125 Hz", "500 Hz", "1000 Hz", "2000 Hz", "4000 Hz", "8000 Hz"].map(String::from).to_vec();
        let w = used_width(narrow, |ui| {
            widgets::heading(ui, "Check mouse", &"Measures the polling rate your mouse really delivers. ".repeat(4));
            widgets::chip_row(ui, &mut m, crate::motion::id("c"), &labels, Some(3));
            widgets::setting_row(ui, "Neutral Windows settings during tests", &"Long description. ".repeat(8), |ui| {
                let l = vec!["Ask".to_string(), "Always".to_string(), "Never".to_string()];
                widgets::chip_row(ui, &mut crate::motion::Motion::default(), crate::motion::id("d"), &l, Some(0));
            });
            widgets::notice(
                ui,
                widgets::Tone::Warn,
                "Mouse input stopped in the middle of a movement",
                &"Plug the mouse in directly. ".repeat(6),
            );
            widgets::option_card(
                ui,
                &mut crate::motion::Motion::default(),
                crate::motion::id("o"),
                Some("3"),
                "Not sure — use what Windows reports",
                &"A long subtitle. ".repeat(6),
                false,
            );
            widgets::key_hint(ui, &[("Enter", "start"), ("Esc", "back to setup"), ("1–7", "choose"), ("R", "remember")]);
            widgets::card(ui, |ui| {
                ui.set_width(ui.available_width());
                widgets::dim(ui, "Card text ".repeat(20));
            });
        });
        assert!(w <= narrow + 0.5, "used {w} of {narrow}");
    }
}

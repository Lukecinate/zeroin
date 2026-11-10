//! Visual language: a dark neutral instrument with one coral accent.
//!
//! Direction: "a calm, precise instrument — one decision per screen, the
//! number is the hero." Coral is reserved for the primary action, the active
//! navigation marker, targets in drills and the result number. No gradients,
//! no glass, shadows only where hierarchy needs lift.

use eframe::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke};
use std::sync::Arc;

pub const BG: Color32 = Color32::from_rgb(0x11, 0x12, 0x14);
pub const RAIL: Color32 = Color32::from_rgb(0x0D, 0x0E, 0x10);
pub const SURFACE: Color32 = Color32::from_rgb(0x19, 0x1A, 0x1D);
pub const SURFACE_HI: Color32 = Color32::from_rgb(0x22, 0x23, 0x27);
pub const BORDER: Color32 = Color32::from_rgb(0x2B, 0x2D, 0x32);
pub const TEXT: Color32 = Color32::from_rgb(0xEC, 0xEA, 0xE6);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x9C, 0x9D, 0xA3);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(0x6A, 0x6C, 0x73);
pub const CORAL: Color32 = Color32::from_rgb(0xFF, 0x5C, 0x47);
pub const CORAL_DIM: Color32 = Color32::from_rgb(0x5A, 0x2A, 0x24);
pub const INK: Color32 = Color32::from_rgb(0x11, 0x11, 0x11);
pub const WARN: Color32 = Color32::from_rgb(0xE8, 0xB0, 0x4A);
pub const OK: Color32 = Color32::from_rgb(0x6C, 0xC7, 0x93);
pub const DRILL_BG: Color32 = Color32::from_rgb(0x0B, 0x0C, 0x0E);

pub const R_CTRL: u8 = 8;
pub const R_CARD: u8 = 12;

pub fn radius(r: u8) -> CornerRadius {
    CornerRadius::same(r)
}

pub fn regular(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}
pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts
        .font_data
        .insert("jakarta".into(), Arc::new(FontData::from_static(include_bytes!("../../../assets/fonts/plus_jakarta_sans_regular.ttf"))));
    fonts.font_data.insert(
        "jakarta-semibold".into(),
        Arc::new(FontData::from_static(include_bytes!("../../../assets/fonts/plus_jakarta_sans_semibold.ttf"))),
    );
    fonts
        .font_data
        .insert("jakarta-bold".into(), Arc::new(FontData::from_static(include_bytes!("../../../assets/fonts/plus_jakarta_sans_bold.ttf"))));
    let fallback: Vec<String> = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut prop = vec!["jakarta".to_string()];
    prop.extend(fallback.iter().cloned());
    fonts.families.insert(FontFamily::Proportional, prop);
    for (fam, name) in [("semibold", "jakarta-semibold"), ("bold", "jakarta-bold")] {
        let mut v = vec![name.to_string()];
        v.extend(fallback.iter().cloned());
        fonts.families.insert(FontFamily::Name(fam.into()), v);
    }
    ctx.set_fonts(fonts);

    let mut style = (*ctx.global_style()).clone();
    style.visuals = egui::Visuals::dark();
    let v = &mut style.visuals;
    v.panel_fill = BG;
    v.window_fill = SURFACE;
    v.extreme_bg_color = Color32::from_rgb(0x0F, 0x10, 0x12);
    v.faint_bg_color = SURFACE;
    v.override_text_color = Some(TEXT);
    v.hyperlink_color = CORAL;
    v.selection.bg_fill = CORAL_DIM;
    v.selection.stroke = Stroke::new(1.0, CORAL);
    v.text_cursor.stroke = Stroke::new(2.0, CORAL);
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.noninteractive, &mut v.widgets.open] {
        w.corner_radius = radius(R_CTRL);
    }
    v.widgets.inactive.bg_fill = SURFACE_HI;
    v.widgets.inactive.weak_bg_fill = SURFACE_HI;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, TEXT_FAINT);
    v.widgets.active.bg_stroke = Stroke::new(1.0, CORAL);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    v.window_shadow = egui::epaint::Shadow::NONE;
    v.popup_shadow = egui::epaint::Shadow::NONE;
    style.text_styles.insert(egui::TextStyle::Body, regular(15.0));
    style.text_styles.insert(egui::TextStyle::Button, semibold(15.0));
    style.text_styles.insert(egui::TextStyle::Small, regular(12.5));
    style.text_styles.insert(egui::TextStyle::Heading, bold(26.0));
    style.text_styles.insert(egui::TextStyle::Monospace, FontId::monospace(13.0));
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(14.0, 8.0);
    style.spacing.interact_size.y = 36.0;
    ctx.set_global_style(style);
}

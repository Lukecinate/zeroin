//! Custom-painted controls. One primary action per screen, plain labels,
//! keyboard hints on options, and spring motion on every state change.

use crate::motion::{lerp_color, Motion};
use crate::theme::*;
use eframe::egui::{self, Align2, Color32, Id, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Crosshair,
    History,
    Settings,
    ArrowLeft,
    ArrowRight,
    Mouse,
    Monitor,
    Check,
    Alert,
    Cpu,
    Rotate,
    Trash,
    Info,
}

impl Icon {
    fn source(self) -> egui::ImageSource<'static> {
        match self {
            Icon::Crosshair => egui::include_image!("../../../assets/icons/crosshair.svg"),
            Icon::History => egui::include_image!("../../../assets/icons/history.svg"),
            Icon::Settings => egui::include_image!("../../../assets/icons/settings.svg"),
            Icon::ArrowLeft => egui::include_image!("../../../assets/icons/arrow-left.svg"),
            Icon::ArrowRight => egui::include_image!("../../../assets/icons/arrow-right.svg"),
            Icon::Mouse => egui::include_image!("../../../assets/icons/mouse.svg"),
            Icon::Monitor => egui::include_image!("../../../assets/icons/monitor.svg"),
            Icon::Check => egui::include_image!("../../../assets/icons/check.svg"),
            Icon::Alert => egui::include_image!("../../../assets/icons/triangle-alert.svg"),
            Icon::Cpu => egui::include_image!("../../../assets/icons/cpu.svg"),
            Icon::Rotate => egui::include_image!("../../../assets/icons/rotate-ccw.svg"),
            Icon::Trash => egui::include_image!("../../../assets/icons/trash-2.svg"),
            Icon::Info => egui::include_image!("../../../assets/icons/info.svg"),
        }
    }
}

pub fn paint_icon(ui: &Ui, icon: Icon, rect: Rect, tint: Color32) {
    egui::Image::new(icon.source()).tint(tint).fit_to_exact_size(rect.size()).paint_at(ui, rect);
}

pub fn icon(ui: &mut Ui, icon: Icon, size: f32, tint: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint_icon(ui, icon, rect, tint);
    resp
}

/// The ZeroIn mark ("z." on a coral tile), painted from the same geometry as the SVG.
pub fn logo(painter: &egui::Painter, rect: Rect) {
    let s = rect.width() / 1024.0;
    let p = |x: f32, y: f32| Pos2::new(rect.min.x + x * s, rect.min.y + y * s);
    painter.rect_filled(
        Rect::from_min_max(p(32.0, 32.0), p(992.0, 992.0)),
        egui::CornerRadius::same((216.0 * s).round().min(255.0) as u8),
        CORAL,
    );
    // The "z" as three convex pieces (top bar, diagonal, bottom bar).
    let ink = INK;
    let top = vec![p(318.0, 236.0), p(742.0, 236.0), p(742.0, 343.0), p(276.0, 343.0)];
    let diag = vec![p(530.0, 343.0), p(742.0, 343.0), p(466.0, 668.0), p(252.0, 668.0)];
    let bottom = vec![p(252.0, 668.0), p(716.0, 668.0), p(679.0, 765.0), p(672.0, 776.0), p(252.0, 776.0)];
    for poly in [top, diag, bottom] {
        painter.add(egui::Shape::convex_polygon(poly, ink, Stroke::NONE));
    }
    painter.circle_filled(p(796.0, 719.0), 57.0 * s, ink);
}

pub fn heading(ui: &mut Ui, title: &str, subtitle: &str) {
    ui.label(egui::RichText::new(title).font(bold(28.0)).color(TEXT));
    if !subtitle.is_empty() {
        ui.add_space(-4.0);
        ui.label(egui::RichText::new(subtitle).font(regular(15.0)).color(TEXT_DIM));
    }
    ui.add_space(12.0);
}

pub fn caption(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).font(semibold(12.0)).color(TEXT_FAINT));
}

pub fn dim(ui: &mut Ui, text: impl Into<String>) -> Response {
    ui.label(egui::RichText::new(text.into()).font(regular(14.0)).color(TEXT_DIM))
}

/// Big coral button. Exactly one per screen.
pub fn primary_button(ui: &mut Ui, motion: &mut Motion, label: &str, enabled: bool) -> Response {
    let id = ui.scope_id().with(("primary", label));
    let font = semibold(16.0);
    let galley = ui.painter().layout_no_wrap(label.to_string(), font.clone(), INK);
    let w = (galley.size().x + 64.0).max(220.0);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 50.0), sense);
    let hover = motion.bool(id.with("h"), enabled && resp.hovered());
    let press = motion.bool(id.with("p"), enabled && resp.is_pointer_button_down_on());
    let rect = rect.shrink(press * 1.5);
    let fill = if enabled { lerp_color(CORAL, Color32::from_rgb(0xFF, 0x74, 0x62), hover) } else { SURFACE_HI };
    let text = if enabled { INK } else { TEXT_FAINT };
    ui.painter().rect_filled(rect, radius(R_CTRL), fill);
    ui.painter().text(rect.center() - egui::vec2(10.0, 0.0), Align2::CENTER_CENTER, label, font, text);
    let arrow = Rect::from_center_size(Pos2::new(rect.right() - 26.0 + hover * 3.0, rect.center().y), Vec2::splat(18.0));
    paint_icon(ui, Icon::ArrowRight, arrow, text);
    resp.on_hover_cursor(if enabled { egui::CursorIcon::PointingHand } else { egui::CursorIcon::NotAllowed })
}

pub fn secondary_button(ui: &mut Ui, motion: &mut Motion, label: &str, icon_opt: Option<Icon>) -> Response {
    let id = ui.scope_id().with(("secondary", label));
    let font = semibold(14.0);
    let galley = ui.painter().layout_no_wrap(label.to_string(), font.clone(), TEXT);
    let icon_w = if icon_opt.is_some() { 24.0 } else { 0.0 };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(galley.size().x + 32.0 + icon_w, 40.0), Sense::click());
    let hover = motion.bool(id.with("h"), resp.hovered());
    ui.painter().rect_filled(rect, radius(R_CTRL), lerp_color(SURFACE, SURFACE_HI, hover));
    ui.painter().rect_stroke(rect, radius(R_CTRL), Stroke::new(1.0, lerp_color(BORDER, TEXT_FAINT, hover)), StrokeKind::Inside);
    let mut x = rect.left() + 16.0;
    if let Some(ic) = icon_opt {
        paint_icon(ui, ic, Rect::from_min_size(Pos2::new(x, rect.center().y - 8.0), Vec2::splat(16.0)), TEXT_DIM);
        x += icon_w;
    }
    ui.painter().text(Pos2::new(x, rect.center().y), Align2::LEFT_CENTER, label, font, TEXT);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Selectable full-width option with a number-key badge.
pub fn option_card(ui: &mut Ui, motion: &mut Motion, id: Id, key: Option<&str>, title: &str, subtitle: &str, selected: bool) -> Response {
    let w = ui.available_width().min(560.0);
    option_card_w(ui, motion, id, key, title, subtitle, "", selected, w)
}

/// Option card with explicit width and an optional right-aligned tag.
#[allow(clippy::too_many_arguments)]
pub fn option_card_w(
    ui: &mut Ui,
    motion: &mut Motion,
    id: Id,
    key: Option<&str>,
    title: &str,
    subtitle: &str,
    tag: &str,
    selected: bool,
    w: f32,
) -> Response {
    let h = if subtitle.is_empty() { 52.0 } else { 64.0 };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, h), Sense::click());
    let hover = motion.bool(id.with("h"), resp.hovered());
    let sel = motion.bool(id.with("s"), selected);
    let p = ui.painter();
    p.rect_filled(rect, radius(R_CTRL), lerp_color(SURFACE, SURFACE_HI, hover.max(sel * 0.6)));
    p.rect_stroke(rect, radius(R_CTRL), Stroke::new(1.0 + sel * 0.5, lerp_color(BORDER, CORAL, sel)), StrokeKind::Inside);
    let mut x = rect.left() + 16.0;
    if let Some(k) = key {
        let badge = Rect::from_min_size(Pos2::new(x, rect.center().y - 12.0), Vec2::splat(24.0));
        p.rect_filled(badge, radius(6), lerp_color(SURFACE_HI, CORAL, sel));
        p.text(badge.center(), Align2::CENTER_CENTER, k, semibold(12.0), lerp_color(TEXT_DIM, INK, sel));
        x += 38.0;
    }
    if subtitle.is_empty() {
        p.text(Pos2::new(x, rect.center().y), Align2::LEFT_CENTER, title, semibold(15.0), TEXT);
    } else {
        p.text(Pos2::new(x, rect.center().y - 10.0), Align2::LEFT_CENTER, title, semibold(15.0), TEXT);
        p.text(Pos2::new(x, rect.center().y + 11.0), Align2::LEFT_CENTER, subtitle, regular(12.5), TEXT_DIM);
    }
    if !tag.is_empty() && sel < 0.5 {
        p.text(Pos2::new(rect.right() - 16.0, rect.center().y), Align2::RIGHT_CENTER, tag, regular(11.5), WARN);
    }
    if sel > 0.01 {
        let c = Pos2::new(rect.right() - 24.0, rect.center().y);
        paint_icon(ui, Icon::Check, Rect::from_center_size(c, Vec2::splat(18.0 * sel)), CORAL);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Row of chips with a highlight that springs between choices.
pub fn chip_row(ui: &mut Ui, motion: &mut Motion, id: Id, labels: &[String], selected: Option<usize>) -> Option<usize> {
    chip_row_from(ui, motion, id, labels, selected, 1)
}

/// [`chip_row`] with key numbers starting at `first` (for a second row on the same screen).
pub fn chip_row_from(ui: &mut Ui, motion: &mut Motion, id: Id, labels: &[String], selected: Option<usize>, first: usize) -> Option<usize> {
    let mut clicked = None;
    let mut rects = Vec::new();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        // In right-to-left parents, add in reverse so chips still read left to right.
        let rtl = ui.layout().prefer_right_to_left();
        let order: Vec<usize> = if rtl { (0..labels.len()).rev().collect() } else { (0..labels.len()).collect() };
        let mut placed: Vec<(usize, Rect)> = Vec::new();
        for i in order {
            let l = &labels[i];
            let g = ui.painter().layout_no_wrap(l.clone(), semibold(15.0), TEXT);
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(g.size().x + 44.0, 44.0), Sense::click());
            placed.push((i, rect));
            if resp.clicked() {
                clicked = Some(i);
            }
            let hover = motion.bool(id.with(("ch", i)), resp.hovered());
            ui.painter().rect_filled(rect, radius(R_CTRL), lerp_color(SURFACE, SURFACE_HI, hover));
            ui.painter().rect_stroke(rect, radius(R_CTRL), Stroke::new(1.0, BORDER), StrokeKind::Inside);
            resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        }
        placed.sort_by_key(|(i, _)| *i);
        rects = placed.into_iter().map(|(_, r)| r).collect();
    });
    if let Some(sel) = selected.and_then(|s| rects.get(s).copied()) {
        let x0 = motion.snappy(id.with("x0"), sel.left());
        let x1 = motion.snappy(id.with("x1"), sel.right());
        let r = Rect::from_min_max(Pos2::new(x0, sel.top()), Pos2::new(x1, sel.bottom()));
        ui.painter().rect_filled(r, radius(R_CTRL), CORAL);
    }
    for (i, (l, rect)) in labels.iter().zip(rects.iter()).enumerate() {
        let on = selected == Some(i);
        let t = motion.bool(id.with(("ct", i)), on);
        ui.painter().text(rect.center() + egui::vec2(6.0, 0.0), Align2::CENTER_CENTER, l, semibold(15.0), lerp_color(TEXT, INK, t));
        ui.painter().text(
            Pos2::new(rect.left() + 12.0, rect.center().y),
            Align2::LEFT_CENTER,
            format!("{}", (i + first) % 10),
            regular(11.0),
            lerp_color(TEXT_FAINT, INK, t),
        );
    }
    clicked
}

/// Three-step progress bar. Returns a step index when a reachable step is clicked.
pub fn step_bar(ui: &mut Ui, motion: &mut Motion, current: usize, reachable: [bool; 3]) -> Option<usize> {
    let labels = ["Setup", "Test", "Result"];
    let w = ui.available_width().min(560.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 40.0), Sense::hover());
    let seg = w / 3.0;
    let mut clicked = None;
    let line_y = rect.bottom() - 4.0;
    ui.painter().line_segment([Pos2::new(rect.left(), line_y), Pos2::new(rect.right(), line_y)], Stroke::new(2.0, BORDER));
    let fill = motion.value(crate::motion::id("stepbar_fill"), (current as f32 + 1.0) * seg);
    ui.painter().line_segment([Pos2::new(rect.left(), line_y), Pos2::new(rect.left() + fill, line_y)], Stroke::new(2.0, CORAL));
    for (i, l) in labels.iter().enumerate() {
        let r = Rect::from_min_size(Pos2::new(rect.left() + seg * i as f32, rect.top()), egui::vec2(seg, 32.0));
        let resp =
            ui.interact(r, crate::motion::id(("stepbar", i)), if reachable[i] && i != current { Sense::click() } else { Sense::hover() });
        if resp.clicked() {
            clicked = Some(i);
        }
        let active = motion.bool(crate::motion::id(("stepbar_a", i)), i == current);
        let done = i < current;
        let col = if i == current {
            lerp_color(TEXT_DIM, TEXT, active)
        } else if done || reachable[i] {
            TEXT_DIM
        } else {
            TEXT_FAINT
        };
        let num_rect = Rect::from_min_size(Pos2::new(r.left(), r.center().y - 11.0), Vec2::splat(22.0));
        ui.painter().rect_filled(num_rect, radius(6), lerp_color(SURFACE_HI, CORAL, active));
        if done {
            paint_icon(ui, Icon::Check, num_rect.shrink(4.0), TEXT_DIM);
        } else {
            ui.painter().text(
                num_rect.center(),
                Align2::CENTER_CENTER,
                format!("{}", i + 1),
                semibold(12.0),
                lerp_color(TEXT_DIM, INK, active),
            );
        }
        ui.painter().text(Pos2::new(num_rect.right() + 10.0, r.center().y), Align2::LEFT_CENTER, *l, semibold(14.0), col);
        if resp.hovered() && reachable[i] && i != current {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    }
    clicked
}

/// On/off switch with a spring-driven knob.
pub fn toggle(ui: &mut Ui, motion: &mut Motion, id: Id, on: &mut bool, enabled: bool) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(egui::vec2(44.0, 24.0), if enabled { Sense::click() } else { Sense::hover() });
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = motion.snappy(id.with("knob"), if *on { 1.0 } else { 0.0 });
    let track = if enabled { lerp_color(SURFACE_HI, CORAL, t) } else { SURFACE };
    ui.painter().rect_filled(rect, radius(12), track);
    ui.painter().rect_stroke(rect, radius(12), Stroke::new(1.0, BORDER), StrokeKind::Inside);
    let x = rect.left() + 12.0 + t * (rect.width() - 24.0);
    ui.painter().circle_filled(Pos2::new(x, rect.center().y), 8.0, if enabled { lerp_color(TEXT_DIM, INK, t) } else { TEXT_FAINT });
    resp
}

/// Settings row: title + description on the left, control on the right.
pub fn setting_row(ui: &mut Ui, title: &str, desc: &str, control: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        let w = ui.available_width();
        ui.allocate_ui_with_layout(egui::vec2((w - 160.0).max(200.0), 48.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(egui::RichText::new(title).font(semibold(15.0)).color(TEXT));
            if !desc.is_empty() {
                ui.label(egui::RichText::new(desc).font(regular(13.0)).color(TEXT_DIM));
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), control);
    });
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Warn,
    Info,
    Ok,
}

pub fn notice(ui: &mut Ui, tone: Tone, title: &str, body: &str) {
    let (ic, col) = match tone {
        Tone::Warn => (Icon::Alert, WARN),
        Tone::Info => (Icon::Info, TEXT_DIM),
        Tone::Ok => (Icon::Check, OK),
    };
    egui::Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(radius(R_CTRL))
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_max_width(ui.available_width().min(620.0));
            ui.horizontal_top(|ui| {
                icon(ui, ic, 18.0, col);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 3.0;
                    ui.label(egui::RichText::new(title).font(semibold(14.0)).color(TEXT));
                    if !body.is_empty() {
                        ui.label(egui::RichText::new(body).font(regular(13.0)).color(TEXT_DIM));
                    }
                });
            });
        });
}

pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(radius(R_CARD))
        .inner_margin(egui::Margin::same(20))
        .show(ui, add)
        .inner
}

/// Windows pointer-speed slider (1–20) with a springy thumb.
pub fn speed_slider(ui: &mut Ui, motion: &mut Motion, id: Id, value: &mut u32) -> Response {
    let w = ui.available_width().min(560.0);
    let (rect, mut resp) = ui.allocate_exact_size(egui::vec2(w, 56.0), Sense::click_and_drag());
    let track =
        Rect::from_min_max(Pos2::new(rect.left() + 12.0, rect.center().y - 2.0), Pos2::new(rect.right() - 12.0, rect.center().y + 2.0));
    let pos_for = |v: u32| track.left() + (v as f32 - 1.0) / 19.0 * track.width();
    if let Some(p) = resp.interact_pointer_pos() {
        let v = (((p.x - track.left()) / track.width()) * 19.0 + 1.0).round().clamp(1.0, 20.0) as u32;
        if v != *value {
            *value = v;
            resp.mark_changed();
        }
    }
    let p = ui.painter();
    p.rect_filled(track, radius(2), SURFACE_HI);
    for v in 1..=20u32 {
        let x = pos_for(v);
        let tall = v == 10;
        p.line_segment(
            [Pos2::new(x, track.bottom() + 8.0), Pos2::new(x, track.bottom() + if tall { 18.0 } else { 12.0 })],
            Stroke::new(1.0, if tall { TEXT_DIM } else { BORDER }),
        );
    }
    p.text(Pos2::new(pos_for(10), track.bottom() + 28.0), Align2::CENTER_CENTER, "10 (default)", regular(11.0), TEXT_FAINT);
    let x = motion.snappy(id.with("thumb"), pos_for(*value));
    p.rect_filled(Rect::from_min_max(track.left_top(), Pos2::new(x, track.bottom())), radius(2), CORAL);
    p.circle_filled(Pos2::new(x, track.center().y), 10.0, CORAL);
    p.circle_filled(Pos2::new(x, track.center().y), 4.0, INK);
    p.text(Pos2::new(x, track.top() - 18.0), Align2::CENTER_CENTER, format!("{value}"), semibold(14.0), TEXT);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Key hint text like "Enter  Next".
pub fn key_hint(ui: &mut Ui, keys: &[(&str, &str)]) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        for (k, label) in keys {
            let g = ui.painter().layout_no_wrap(k.to_string(), semibold(11.0), TEXT_DIM);
            let (r, _) = ui.allocate_exact_size(egui::vec2(g.size().x + 12.0, 20.0), Sense::hover());
            ui.painter().rect_stroke(r, radius(4), Stroke::new(1.0, BORDER), StrokeKind::Inside);
            ui.painter().galley(r.center() - g.size() / 2.0, g, TEXT_DIM);
            ui.label(egui::RichText::new(*label).font(regular(12.0)).color(TEXT_FAINT));
            ui.add_space(8.0);
        }
    });
}

//! ZeroIn — find your mouse sensitivity with aim drills (Windows 10/11).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod hw;
mod llm;
mod motion;
mod paths;
mod platform;
mod screens;
mod theme;
mod widgets;

fn load_icon() -> eframe::egui::IconData {
    let png = include_bytes!("../../../assets/brand/png/zeroin-256.png");
    eframe::icon_data::from_png_bytes(png).unwrap_or_default()
}

fn main() -> eframe::Result {
    platform::init_clock();
    let viewport = eframe::egui::ViewportBuilder::default()
        .with_title("ZeroIn")
        .with_app_id("ZeroIn")
        .with_inner_size([1200.0, 780.0])
        .with_min_inner_size([980.0, 660.0])
        .with_icon(load_icon());
    let options = eframe::NativeOptions { viewport, centered: true, system_font_fallback: false, ..Default::default() };
    eframe::run_native("ZeroIn", options, Box::new(|cc| Ok(Box::new(app::ZeroInApp::new(cc)))))
}

//! The editor: an egui window over [`lineit_core`].

mod app;
mod canvas;
mod runner;
mod subgraph;
mod tab;
mod theme;

use std::path::PathBuf;

pub fn run(files: Vec<PathBuf>) -> Result<(), String> {
    let cfg = lineit_core::config::Config::load(&lineit_core::paths::config_dir());
    let size = [cfg.num("window.width") as f32, cfg.num("window.height") as f32];
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../../../../assets/lineit.png")).ok();
    let mut viewport =
        eframe::egui::ViewportBuilder::default().with_inner_size(size).with_title("lineit").with_app_id("lineit");
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("lineit", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, files)))))
        .map_err(|e| format!("couldn't open the window: {e}"))
}

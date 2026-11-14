//! The editor: an egui window over [`lineit_core`].

mod app;
mod canvas;
mod dialog;
mod nav;
mod runner;
mod session;
mod single;
mod subgraph;
mod tab;
mod tests;
mod theme;
mod vim;

use std::path::PathBuf;

/// Opens FILES (none: a new canvas) in the editor running, in tabs, or in a
/// new editor if there's none or NEW.
pub fn run(files: Vec<PathBuf>, new: bool) -> Result<(), String> {
    if !new && single::send(&files) {
        return Ok(());
    }
    open(app::Start::Files(files))
}

/// A read-only tab to open: NEW (called LABEL), or what changed from OLD to it.
pub struct View {
    pub label: String,
    pub old: Option<lineit_core::model::Canvas>,
    pub new: lineit_core::model::Canvas,
}

/// Opens VIEWS in a new editor (never in one running: they aren't files).
pub fn views(views: Vec<View>) -> Result<(), String> {
    open(app::Start::Views(views))
}

/// Carries on from a session an older editor wrote, when it was rebuilt.
pub fn restore(session: &std::path::Path) -> Result<(), String> {
    open(app::Start::Session(session::read(session)?))
}

fn open(start: app::Start) -> Result<(), String> {
    let cfg = lineit_core::config::Config::load(&lineit_core::paths::config_dir());
    let mut size = [cfg.num("window.width") as f32, cfg.num("window.height") as f32];
    let mut pos = None;
    if let app::Start::Session(s) = &start {
        size = s.window.size.unwrap_or(size);
        pos = s.window.pos;
    }
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../../../../assets/lineit.png")).ok();
    let mut viewport =
        eframe::egui::ViewportBuilder::default().with_inner_size(size).with_title("lineit").with_app_id("lineit");
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    if let Some(p) = pos {
        viewport = viewport.with_position(p);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("lineit", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, start)))))
        .map_err(|e| format!("couldn't open the window: {e}"))
}

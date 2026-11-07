//! A tab: a canvas, its file, its history, what's selected and its run.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lineit_core::lower::{self, Lowered};
use lineit_core::model::{Canvas, Link, SquareId};
use lineit_core::registry::Registry;

use super::runner::Runner;

/// How many changes undo remembers.
const HISTORY: usize = 500;

pub struct Tab {
    pub canvas: Canvas,
    pub path: Option<PathBuf>,
    /// The canvas as it was saved (or opened).
    saved: Canvas,
    undo: Vec<Canvas>,
    redo: Vec<Canvas>,
    /// The canvas the last change in the history ends with.
    committed: Canvas,
    /// What the canvas means, for the canvas it was worked out for.
    pub lowered: Lowered,
    lowered_for: Option<Canvas>,
    pub selected: BTreeSet<SquareId>,
    pub selected_links: BTreeSet<Link>,
    pub run: Option<Runner>,
    pub output: Vec<String>,
    /// A message for the bar, and whether it's an error.
    pub status: Option<(String, bool)>,
    /// Goes up each time the canvas is lowered again.
    pub generation: u64,
}

impl Tab {
    pub fn new(canvas: Canvas, path: Option<PathBuf>, reg: &Registry) -> Tab {
        let lowered = lower::lower(&canvas, reg);
        Tab {
            saved: canvas.clone(),
            committed: canvas.clone(),
            lowered_for: Some(canvas.clone()),
            canvas,
            path,
            undo: Vec::new(),
            redo: Vec::new(),
            lowered,
            selected: BTreeSet::new(),
            selected_links: BTreeSet::new(),
            run: None,
            output: Vec::new(),
            status: None,
            generation: 0,
        }
    }

    /// A new canvas, with an entry.
    pub fn empty(reg: &Registry) -> Tab {
        let mut c = Canvas::default();
        c.add(lineit_core::model::Square::new(1, "flow/entry", -100.0, -40.0));
        let mut t = Tab::new(c, None, reg);
        t.saved = Canvas::default();
        t
    }

    pub fn open(path: &Path, reg: &Registry) -> Result<Tab, String> {
        let src = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut canvas = lineit_core::file::read_canvas(&src, &path.display().to_string())?;
        if canvas.needs_arranging() {
            lower::arrange(&mut canvas, reg);
        }
        // no camera saved: all the squares in view (in a window of about
        // 1200x560 for the canvas), not bigger than they are
        if canvas.camera == lineit_core::model::Camera::default() && !canvas.squares.is_empty() {
            let (x0, x1) =
                canvas.squares.iter().fold((f32::MAX, f32::MIN), |(a, b), s| (a.min(s.x), b.max(s.x + 200.0)));
            let (y0, y1) =
                canvas.squares.iter().fold((f32::MAX, f32::MIN), |(a, b), s| (a.min(s.y), b.max(s.y + 150.0)));
            // and room for the doc, in the corner
            let top = if canvas.doc.is_empty() { 0.0 } else { 110.0 };
            let zoom = (1200.0 / (x1 - x0)).min((560.0 - top) / (y1 - y0)).clamp(0.3, 1.0);
            canvas.camera.x = (x0 + x1) / 2.0;
            canvas.camera.y = (y0 + y1) / 2.0 - top / 2.0 / zoom;
            canvas.camera.zoom = zoom;
        }
        Ok(Tab::new(canvas, Some(path.to_path_buf()), reg))
    }

    pub fn title(&self) -> String {
        let name = self.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string());
        let name = name.unwrap_or_else(|| "new canvas".into());
        if self.dirty() { format!("{name} •") } else { name }
    }

    pub fn dirty(&self) -> bool {
        strip_camera(&self.canvas) != strip_camera(&self.saved)
    }

    pub fn dir(&self) -> PathBuf {
        self.path
            .as_ref()
            .and_then(|p| p.parent())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
    }

    pub fn save_to(&mut self, path: &Path) -> Result<(), String> {
        let text = lineit_core::file::write_canvas(&self.canvas);
        let tmp = path.with_extension("lua.tmp");
        std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))?;
        self.path = Some(path.to_path_buf());
        self.saved = self.canvas.clone();
        Ok(())
    }

    /// Called every frame: a change since the last one goes in the history
    /// (unless SETTLING: something is being dragged or typed), and the
    /// canvas is lowered again if it changed.
    pub fn update(&mut self, reg: &Arc<Registry>, settling: bool, force: bool) {
        if !settling && strip_camera(&self.canvas) != strip_camera(&self.committed) {
            self.undo.push(std::mem::replace(&mut self.committed, self.canvas.clone()));
            if self.undo.len() > HISTORY {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
        let changed = self.lowered_for.as_ref().map(|c| strip_camera(c) != strip_camera(&self.canvas)).unwrap_or(true);
        if changed || force {
            self.lowered = lower::lower(&self.canvas, reg);
            self.lowered_for = Some(self.canvas.clone());
            self.generation += 1;
            // squares that are gone aren't selected
            let ids: BTreeSet<SquareId> = self.canvas.squares.iter().map(|s| s.id).collect();
            self.selected.retain(|s| ids.contains(s));
            self.selected_links.retain(|l| self.canvas.links.contains(l));
        }
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            let camera = self.canvas.camera.clone();
            self.redo.push(std::mem::replace(&mut self.canvas, prev));
            self.canvas.camera = camera;
            self.committed = self.canvas.clone();
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            let camera = self.canvas.camera.clone();
            self.undo.push(std::mem::replace(&mut self.canvas, next));
            self.canvas.camera = camera;
            self.committed = self.canvas.clone();
        }
    }

    pub fn is_running(&self) -> bool {
        self.run.as_ref().is_some_and(|r| r.is_live())
    }
}

/// The camera isn't a change worth saving or undoing.
fn strip_camera(c: &Canvas) -> (&str, &[lineit_core::model::Square], &[Link]) {
    (&c.doc, &c.squares, &c.links)
}

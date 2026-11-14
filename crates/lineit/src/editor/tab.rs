//! A tab: a canvas, its file, its history, what's selected and its run.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use lineit_core::file::{quote, read_canvas, write_canvas};

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
    /// When its file was last changed, as far as this tab knows: if it
    /// changes again, something else changed it.
    pub disk: Option<SystemTime>,
    /// It's the Settings tab, not a canvas.
    pub settings: bool,
}

fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

impl Tab {
    pub fn new(mut canvas: Canvas, path: Option<PathBuf>, reg: &Registry) -> Tab {
        canvas.dir = path.as_deref().and_then(Path::parent).map(Path::to_path_buf);
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
            disk: None,
            settings: false,
        }
    }

    /// The Settings tab (see `App::settings_page`).
    pub fn settings_tab(reg: &Registry) -> Tab {
        let mut t = Tab::new(Canvas::default(), None, reg);
        t.settings = true;
        t
    }

    /// A new canvas, with an entry.
    pub fn empty(reg: &Registry) -> Tab {
        let mut c = Canvas::default();
        c.add(lineit_core::model::Square::new(1, "flow/entry", -100.0, -40.0));
        // its entry isn't a change: nothing to save until something is done
        Tab::new(c, None, reg)
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
        let mut t = Tab::new(canvas, Some(path.to_path_buf()), reg);
        t.disk = mtime(path);
        Ok(t)
    }

    /// If its file was changed by something else: read it again, keeping the
    /// camera, as a change that undo takes back. With unsaved changes it
    /// isn't read, and the bar says so. Whether it was read.
    pub fn reload_from_disk(&mut self, reg: &Arc<Registry>) -> bool {
        let Some(path) = self.path.clone() else { return false };
        let now = mtime(&path);
        if now.is_none() || now == self.disk {
            return false;
        }
        self.disk = now;
        if self.dirty() {
            self.status = Some((
                format!("{} changed on disk; you have changes here: saving overwrites it", path.display()),
                true,
            ));
            return false;
        }
        let read = std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|src| read_canvas(&src, &path.display().to_string()));
        match read {
            Ok(mut c) => {
                c.camera = self.canvas.camera.clone();
                self.undo.push(self.canvas.clone());
                self.redo.clear();
                self.canvas = c;
                self.saved = self.canvas.clone();
                self.committed = self.canvas.clone();
                self.update(reg, false, true);
                self.status =
                    Some((format!("{} changed on disk: read again (Ctrl+Z takes it back)", path.display()), false));
                true
            }
            Err(e) => {
                self.status = Some((format!("{} changed on disk, and can't be read: {e}", path.display()), true));
                false
            }
        }
    }

    /// The tab as a Lua table, for a session (see `session.rs`): its
    /// canvas, file, what was saved, selection, history and output.
    pub fn session(&self) -> String {
        if self.settings {
            return "{ settings = true }".into();
        }
        let list = |v: &[Canvas]| v.iter().map(|c| quote(&write_canvas(c))).collect::<Vec<_>>().join(", ");
        let mut t = String::from("{ ");
        if let Some(p) = &self.path {
            t.push_str(&format!("path = {}, ", quote(&p.to_string_lossy())));
        }
        t.push_str(&format!("canvas = {}, ", quote(&write_canvas(&self.canvas))));
        t.push_str(&format!("saved = {}, ", quote(&write_canvas(&self.saved))));
        let sel: Vec<String> = self.selected.iter().map(|s| s.to_string()).collect();
        t.push_str(&format!("selected = {{ {} }}, ", sel.join(", ")));
        t.push_str(&format!("running = {}, ", self.is_running()));
        t.push_str(&format!("undo = {{ {} }}, redo = {{ {} }}, ", list(&self.undo), list(&self.redo)));
        let out: Vec<String> = self.output.iter().map(|l| quote(l)).collect();
        t.push_str(&format!("output = {{ {} }} }}", out.join(", ")));
        t
    }

    /// A tab from what [`Tab::session`] wrote. Whether it was running.
    pub fn from_session(t: &mlua::Table, reg: &Registry) -> Result<(Tab, bool), String> {
        if t.get::<Option<bool>>("settings").ok().flatten() == Some(true) {
            return Ok((Tab::settings_tab(reg), false));
        }
        let text = |k: &str| t.get::<Option<String>>(k).map_err(|e| e.to_string());
        let canvas = |src: &str| read_canvas(src, "the session");
        let canvas_text = text("canvas")?.ok_or("a tab with no canvas")?;
        let path = text("path")?.map(PathBuf::from);
        let mut tab = Tab::new(canvas(&canvas_text)?, path.clone(), reg);
        if let Some(saved) = text("saved")? {
            tab.saved = canvas(&saved)?;
        }
        let list = |k: &str| -> Result<Vec<Canvas>, String> {
            match t.get::<Option<Vec<String>>>(k).map_err(|e| e.to_string())? {
                Some(v) => v.iter().map(|c| canvas(c)).collect(),
                None => Ok(Vec::new()),
            }
        };
        tab.undo = list("undo")?;
        tab.redo = list("redo")?;
        tab.selected =
            t.get::<Option<Vec<SquareId>>>("selected").ok().flatten().unwrap_or_default().into_iter().collect();
        tab.output = t.get::<Option<Vec<String>>>("output").ok().flatten().unwrap_or_default();
        tab.disk = path.as_deref().and_then(mtime);
        let running = t.get::<Option<bool>>("running").ok().flatten().unwrap_or(false);
        Ok((tab, running))
    }

    pub fn title(&self) -> String {
        if self.settings {
            return "Settings".into();
        }
        let name = self.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string());
        let name = name.unwrap_or_else(|| "new canvas".into());
        if self.dirty() { format!("{name} •") } else { name }
    }

    pub fn dirty(&self) -> bool {
        !self.settings && strip_camera(&self.canvas) != strip_camera(&self.saved)
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
        self.disk = mtime(path);
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
        // where the file is says what its paths mean (saved as another: it changes)
        let dir = self.path.as_deref().and_then(Path::parent).map(Path::to_path_buf);
        let moved = self.canvas.dir != dir;
        if moved {
            self.canvas.dir = dir.clone();
            self.committed.dir = dir.clone();
            self.saved.dir = dir;
        }
        // a file a square's pins came from (a shader) was changed
        let stale = self.lowered.deps.iter().any(|(p, t)| mtime(p) != *t);
        let changed = self.lowered_for.as_ref().map(|c| strip_camera(c) != strip_camera(&self.canvas)).unwrap_or(true);
        if changed || force || moved || stale {
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

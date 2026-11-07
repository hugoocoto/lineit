//! The editor's window: tabs at the top, the canvas, the output and the bar
//! at the bottom, and the popups (module search, path prompt, settings).

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::{self, Key, Modifiers, Pos2, vec2};
use lineit_core::config::{Config, Kind as SKind, SETTINGS, SValue};
use lineit_core::model::{Canvas, Link, Square, SquareId};
use lineit_core::registry::Registry;
use lineit_rt::Value;

use super::canvas::{self, Action, CanvasState, Pending};
use super::runner::{Cmd, Runner, State};
use super::session::{self, Exe, Ready};
use super::tab::Tab;
use super::theme::{self, Theme};
use super::vim::{SAct, Vim};

mod vimctl;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    Run,
    Step,
    Compile,
}

struct Search {
    at: Pos2,
    screen: Pos2,
    query: String,
    pending: Option<Pending>,
    selected: usize,
    opened: Instant,
}

#[derive(Clone, PartialEq)]
enum PromptKind {
    Open,
    SaveAs,
    SubgraphName,
}

struct Prompt {
    kind: PromptKind,
    title: String,
    text: String,
    error: Option<String>,
    focus: bool,
}

/// Previews being worked out in the background, for a tab's canvas as it
/// was at `generation`.
struct PreviewJob {
    tab: u64,
    generation: u64,
    result: Arc<Mutex<Option<Vec<((SquareId, String), Value)>>>>,
}

/// A compile going on in the background.
struct Compiling {
    log: Arc<Mutex<Vec<String>>>,
    done: Arc<Mutex<Option<Result<PathBuf, String>>>>,
    tab: u64,
}

pub struct App {
    tabs: Vec<(u64, Tab, CanvasState)>,
    active: usize,
    next_tab: u64,
    closed: Vec<(Canvas, Option<PathBuf>)>,
    reg: Arc<Registry>,
    cfg: Config,
    theme: Theme,
    mode: RunMode,
    show_output: bool,
    settings_query: String,
    /// The section of the Settings tab to scroll to, chosen on its left.
    settings_jump: Option<&'static str>,
    /// The Settings tab's search takes the keyboard on the next frame.
    settings_focus: bool,
    /// The tab the strip last scrolled to.
    tab_in_view: Option<u64>,
    search: Option<Search>,
    prompt: Option<Prompt>,
    compiling: Option<Compiling>,
    last_autosave: Instant,
    /// When the module folders last changed, to reload them.
    modules_stamp: Option<std::time::SystemTime>,
    last_stamp_check: Instant,
    /// The binary, to hand over to a rebuilt one (see session.rs).
    exe: Option<Exe>,
    /// Started by an older editor: tell it when this one is drawing.
    ready: Ready,
    /// What other `lineit FILE` commands send to this editor.
    single: Option<super::single::Single>,
    /// When config.lua and settings.lua last changed, to read them again.
    config_stamp: (Option<std::time::SystemTime>, Option<std::time::SystemTime>),
    font_applied: String,
    /// The squares being made into a subgraph, waiting for its name.
    subgraph_of: BTreeSet<SquareId>,
    preview_job: Option<PreviewJob>,
    /// Per tab, the generation its previews are for.
    previewed: HashMap<u64, u64>,
    /// Vim mode: the keys' state (see vim.rs and app/vimctl.rs).
    vim: Vim,
    /// What `yy` and `dd` took, as a canvas, for `p`: egui can't read the
    /// clipboard on its own.
    register: Option<String>,
    /// `:help` is open.
    help: bool,
    /// The tab the vim keys were last for.
    vim_tab: u64,
    /// What `/` looked for last, for `n`.
    last_search: String,
    /// The setting the Settings tab's cursor is on, and what the keys asked
    /// of it since the last frame.
    settings_sel: usize,
    settings_ops: Vec<SAct>,
    /// Tab and arrow keys taken from the input before egui could use them
    /// to move its focus (see `vim_hook`).
    vim_held: Vec<egui::Event>,
}

/// The newest change in DIR and its folders.
fn stamp(dir: &Path) -> Option<std::time::SystemTime> {
    let mut newest = std::fs::metadata(dir).and_then(|m| m.modified()).ok();
    let mut stack = vec![dir.to_path_buf()];
    let mut seen = 0;
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            seen += 1;
            if seen > 5000 {
                return newest;
            }
            let Ok(m) = e.metadata() else { continue };
            if m.is_dir() {
                stack.push(e.path());
            }
            if let Ok(t) = m.modified() {
                newest = newest.max(Some(t));
            }
        }
    }
    newest
}

impl App {
    pub fn new(cc: &eframe::CreationContext, start: Start) -> App {
        let cfg = Config::load(&lineit_core::paths::config_dir());
        let dir = match &start {
            Start::Files(files) => files.first().and_then(|f| f.parent()).map(Path::to_path_buf),
            Start::Session(_) => None,
        };
        let reg = Arc::new(lineit_core::registry(dir.as_deref()));
        let theme = Theme::from_config(&cfg);
        let mut app = App {
            tabs: Vec::new(),
            active: 0,
            next_tab: 1,
            closed: Vec::new(),
            reg,
            theme,
            mode: RunMode::Run,
            show_output: true,
            settings_query: String::new(),
            settings_jump: None,
            settings_focus: false,
            tab_in_view: None,
            search: None,
            prompt: None,
            compiling: None,
            last_autosave: Instant::now(),
            modules_stamp: None,
            last_stamp_check: Instant::now(),
            font_applied: String::new(),
            subgraph_of: BTreeSet::new(),
            preview_job: None,
            previewed: HashMap::new(),
            vim: Vim::new(),
            register: None,
            help: false,
            vim_tab: 0,
            last_search: String::new(),
            settings_sel: 0,
            settings_ops: Vec::new(),
            vim_held: Vec::new(),
            exe: Exe::current(),
            ready: Ready::from_env(),
            single: super::single::listen(cc.egui_ctx.clone()),
            config_stamp: config_stamp(),
            cfg,
        };
        for e in app.cfg.errors.clone() {
            app.status(&format!("config: {e}"), true);
        }
        match start {
            Start::Files(files) => {
                for f in files {
                    app.open(&f);
                }
            }
            Start::Session(s) => {
                if let Err(e) = app.restore(&s.source, &cc.egui_ctx) {
                    eprintln!("lineit: the session of the editor before: {e}");
                }
            }
        }
        if app.tabs.is_empty() {
            let t = Tab::empty(&app.reg);
            app.add_tab(t);
        }
        for e in app.reg.errors.clone() {
            app.tab().output.push(format!("lineit: {e}"));
        }
        app.modules_stamp = app.module_stamp();
        app.apply_style(&cc.egui_ctx);
        app
    }

    /// The tabs of a session an older editor wrote, and what was shown.
    fn restore(&mut self, source: &str, ctx: &egui::Context) -> Result<(), String> {
        let reg = self.reg.clone();
        let (tabs, active, show_output) = lineit_core::file::with_data(source, "the session", |t| {
            let mut tabs = Vec::new();
            let list: mlua::Table = t.get("tabs").map_err(|e| e.to_string())?;
            for tab in list.sequence_values::<mlua::Table>() {
                tabs.push(Tab::from_session(&tab.map_err(|e| e.to_string())?, &reg)?);
            }
            let active = t.get::<Option<usize>>("active").ok().flatten().unwrap_or(1).saturating_sub(1);
            let show_output = t.get::<Option<bool>>("show_output").ok().flatten().unwrap_or(true);
            Ok((tabs, active, show_output))
        })?;
        let mut running = Vec::new();
        for (t, run) in tabs {
            self.add_tab(t);
            if run {
                running.push(self.tabs.len() - 1);
            }
        }
        self.show_output = show_output;
        // what was running runs again, from the start
        for i in running {
            self.active = i;
            self.start(RunMode::Run, ctx);
        }
        self.active = active.min(self.tabs.len().saturating_sub(1));
        Ok(())
    }

    /// Hands over to the rebuilt binary; closes this window if it took over.
    fn hand_over(&mut self, ctx: &egui::Context) {
        let Some(exe) = &self.exe else { return };
        let tabs: Vec<String> = self.tabs.iter().map(|(_, t, _)| t.session()).collect();
        let (pos, size) = ctx.input(|i| {
            let v = i.viewport();
            (v.outer_rect.map(|r| [r.min.x, r.min.y]), v.inner_rect.map(|r| [r.width(), r.height()]))
        });
        let text = session::write(&tabs, self.active, self.show_output, &session::Window { pos, size });
        match exe.hand_over(&text) {
            Ok(()) => {
                // the runs stop as their tabs go
                self.tabs.clear();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Err(e) => {
                self.status(&format!("lineit was rebuilt, and the new one didn't start ({e}): keeping this one"), true)
            }
        }
    }

    /// Reads config.lua and settings.lua again, and applies them.
    fn reload_config(&mut self, ctx: &egui::Context) {
        self.cfg = Config::load(&lineit_core::paths::config_dir());
        self.theme = Theme::from_config(&self.cfg);
        self.apply_style(ctx);
        match self.cfg.errors.first().cloned() {
            Some(e) => self.status(&format!("config: {e}"), true),
            None => self.status("config read again", false),
        }
    }

    fn tab(&mut self) -> &mut Tab {
        &mut self.tabs[self.active].1
    }

    fn add_tab(&mut self, t: Tab) {
        self.tabs.push((self.next_tab, t, CanvasState::default()));
        self.next_tab += 1;
        self.active = self.tabs.len() - 1;
    }

    fn status(&mut self, msg: &str, error: bool) {
        if !self.tabs.is_empty() {
            self.tab().status = Some((msg.to_string(), error));
        } else {
            eprintln!("lineit: {msg}");
        }
    }

    fn open(&mut self, path: &Path) {
        if let Some(i) = self.tabs.iter().position(|(_, t, _)| t.path.as_deref() == Some(path)) {
            self.active = i;
            return;
        }
        if !path.exists() {
            // a new file, saved there
            let mut t = Tab::empty(&self.reg);
            t.path = Some(path.to_path_buf());
            self.add_tab(t);
            return;
        }
        match Tab::open(path, &self.reg) {
            Ok(t) => {
                // an empty tab nothing was done in is replaced
                if self.tabs.len() == 1 && self.tabs[0].1.path.is_none() && !self.tabs[0].1.dirty() {
                    self.tabs.clear();
                }
                self.add_tab(t);
            }
            Err(e) => self.status(&e, true),
        }
    }

    fn apply_style(&mut self, ctx: &egui::Context) {
        ctx.set_visuals(self.theme.visuals());
        let size = self.cfg.num("font.size") as f32;
        ctx.all_styles_mut(|s| {
            for (style, font) in s.text_styles.iter_mut() {
                font.size = match style {
                    egui::TextStyle::Heading => size * 1.3,
                    egui::TextStyle::Small => size * 0.8,
                    _ => size,
                };
            }
        });
        let name = self.cfg.text("font.name");
        if name != self.font_applied {
            self.font_applied = name.clone();
            let mut fonts = egui::FontDefinitions::default();
            // a name installed, or a file
            // a name that isn't a font keeps the built-in one (Settings says so)
            if let Some(path) = lineit_core::fonts::find(&name) {
                match std::fs::read(&path) {
                    Ok(bytes) => {
                        fonts.font_data.insert("user".into(), Arc::new(egui::FontData::from_owned(bytes)));
                        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                            fonts.families.entry(family).or_default().insert(0, "user".into());
                        }
                    }
                    Err(e) => self.status(&format!("font {}: {e}", path.display()), true),
                }
            }
            ctx.set_fonts(fonts);
        }
    }

    fn module_stamp(&self) -> Option<std::time::SystemTime> {
        let mut s = stamp(&lineit_core::paths::modules_dir());
        s = s.max(stamp(&lineit_core::paths::packages_dir()));
        for (_, t, _) in &self.tabs {
            s = s.max(stamp(&t.dir().join("modules")));
        }
        s
    }

    fn reload_modules(&mut self) {
        let dir = self.tabs.get(self.active).map(|(_, t, _)| t.dir());
        self.reg = Arc::new(lineit_core::registry(dir.as_deref()));
        let reg = self.reg.clone();
        for (_, t, _) in &mut self.tabs {
            t.update(&reg, false, true);
        }
        let errors = self.reg.errors.clone();
        for e in errors {
            self.tab().output.push(format!("lineit: {e}"));
        }
    }

    // ------------------------------------------------------------ files

    fn save(&mut self, ctx: &egui::Context) {
        if self.tab().settings {
            return;
        }
        match self.tab().path.clone() {
            Some(p) => match self.tab().save_to(&p) {
                Ok(()) => self.status(&format!("saved {}", p.display()), false),
                Err(e) => self.status(&e, true),
            },
            None => self.ask(PromptKind::SaveAs, ctx),
        }
    }

    fn ask(&mut self, kind: PromptKind, _ctx: &egui::Context) {
        let dir = self.tab().dir();
        let (title, text) = match kind {
            PromptKind::Open => ("Open", format!("{}/", dir.display())),
            PromptKind::SaveAs => (
                "Save as",
                self.tab()
                    .path
                    .clone()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| format!("{}/canvas.lua", dir.display())),
            ),
            PromptKind::SubgraphName => {
                ("Name of the subgraph (a module in your modules folder)", "my/subgraph".to_string())
            }
        };
        self.prompt = Some(Prompt { kind, title: title.into(), text, error: None, focus: true });
    }

    fn prompt_done(&mut self, ctx: &egui::Context) {
        let Some(p) = self.prompt.take() else { return };
        let path = PathBuf::from(p.text.trim());
        match p.kind {
            PromptKind::Open => {
                if path.is_dir() || p.text.trim().is_empty() {
                    self.prompt = Some(Prompt { error: Some("that's a folder: which file?".into()), ..p });
                    return;
                }
                self.open(&path);
            }
            PromptKind::SaveAs => {
                let path = if path.extension().is_none() { path.with_extension("lua") } else { path };
                match self.tab().save_to(&path) {
                    Ok(()) => self.status(&format!("saved {}", path.display()), false),
                    Err(e) => {
                        self.prompt = Some(Prompt { error: Some(e), ..p });
                        return;
                    }
                }
            }
            PromptKind::SubgraphName => {
                let name = p.text.trim().trim_matches('/').to_string();
                let sel = std::mem::take(&mut self.subgraph_of);
                match super::subgraph::make(self.tab(), &sel, &name) {
                    Ok(file) => {
                        self.reload_modules();
                        self.modules_stamp = self.module_stamp();
                        self.status(&format!("made {}: it's the module `{name}`", file.display()), false);
                    }
                    Err(e) => {
                        self.subgraph_of = sel;
                        self.prompt = Some(Prompt { error: Some(e), ..p });
                        return;
                    }
                }
            }
        }
        let _ = ctx;
    }

    fn close_tab(&mut self, i: usize) {
        let (_, t, _) = self.tabs.remove(i);
        if !t.settings {
            self.closed.push((t.canvas, t.path));
        }
        if self.tabs.is_empty() {
            let t = Tab::empty(&self.reg);
            self.add_tab(t);
        }
        self.active = self.active.min(self.tabs.len() - 1);
    }

    // ------------------------------------------------------------ run

    fn start(&mut self, mode: RunMode, ctx: &egui::Context) {
        if self.tab().settings {
            return;
        }
        self.mode = mode;
        if mode == RunMode::Compile {
            self.compile(ctx);
            return;
        }
        let clear = self.cfg.bool("output.clear_on_run");
        let tab = self.tab();
        if let Some(r) = &tab.run {
            if r.state() == State::Paused {
                r.send(if mode == RunMode::Step { Cmd::Step } else { Cmd::Run });
                return;
            }
            if r.is_live() {
                return;
            }
        }
        let Some(prog) = tab.lowered.program.clone() else {
            let n = tab.lowered.diags.len();
            tab.status = Some((format!("it can't run: {n} problem{} (in red)", if n == 1 { "" } else { "s" }), true));
            return;
        };
        if clear {
            tab.output.clear();
        }
        tab.status = None;
        let first = if mode == RunMode::Step { Cmd::Step } else { Cmd::Run };
        tab.run = Some(Runner::start(prog, tab.dir(), first, ctx.clone()));
        self.show_output = true;
    }

    fn stop(&mut self) {
        if let Some(r) = &self.tab().run {
            r.send(Cmd::Stop);
        }
    }

    fn compile(&mut self, ctx: &egui::Context) {
        if self.compiling.is_some() {
            return;
        }
        let tab_id = self.tabs[self.active].0;
        let tab = self.tab();
        let Some(path) = tab.path.clone() else {
            tab.status = Some(("save the canvas first: the program goes next to it".into(), true));
            return;
        };
        let Some(prog) = tab.lowered.program.clone() else {
            tab.status = Some(("it can't compile: fix the problems in red first".into(), true));
            return;
        };
        tab.output.push(format!("lineit: compiling {}…", path.display()));
        let out = path.with_file_name(lineit_core::paths::exe(&lineit_core::codegen::program_name(&path)));
        let log = Arc::new(Mutex::new(Vec::new()));
        let done = Arc::new(Mutex::new(None));
        let (l, d, c) = (log.clone(), done.clone(), ctx.clone());
        std::thread::spawn(move || {
            let opts = lineit_core::codegen::Options::for_canvas(&path, out);
            let r = lineit_core::codegen::compile(&prog, &opts, &mut |line| {
                l.lock().unwrap().push(line.to_string());
                c.request_repaint();
            });
            *d.lock().unwrap() = Some(r);
            c.request_repaint();
        });
        self.compiling = Some(Compiling { log, done, tab: tab_id });
        self.show_output = true;
    }

    /// Takes what the runs and the compile did since the last frame.
    fn poll(&mut self) {
        let max = self.cfg.num("output.max_lines") as usize;
        for (_, t, _) in &mut self.tabs {
            if let Some(r) = &t.run {
                let mut v = r.view.lock().unwrap();
                t.output.append(&mut v.output);
                if let Some(e) = v.error.take() {
                    t.output.push(format!("lineit: {e}"));
                    t.status = Some((e, true));
                }
            }
            if t.output.len() > max {
                let extra = t.output.len() - max;
                t.output.drain(..extra);
            }
        }
        if let Some(c) = &self.compiling {
            let lines: Vec<String> = std::mem::take(&mut *c.log.lock().unwrap());
            let done = c.done.lock().unwrap().take();
            let tab = self.tabs.iter_mut().find(|(id, _, _)| *id == c.tab).map(|(_, t, _)| t);
            if let Some(t) = tab {
                t.output.extend(lines);
                match &done {
                    Some(Ok(exe)) => {
                        t.output.push(format!("lineit: compiled to {}", exe.display()));
                        t.status = Some((format!("compiled to {}", exe.display()), false));
                    }
                    Some(Err(e)) => {
                        t.output.push(format!("lineit: {e}"));
                        t.status = Some((e.clone(), true));
                    }
                    None => {}
                }
            }
            if done.is_some() {
                self.compiling = None;
            }
        }
    }

    // ------------------------------------------------------------ editing

    fn selection_text(&self) -> Option<String> {
        let t = &self.tabs[self.active].1;
        if t.selected.is_empty() {
            return None;
        }
        let mut c = Canvas::default();
        for s in &t.canvas.squares {
            if t.selected.contains(&s.id) {
                c.squares.push(s.clone());
            }
        }
        for l in &t.canvas.links {
            if t.selected.contains(&l.from) && t.selected.contains(&l.to) {
                c.links.push(l.clone());
            }
        }
        Some(lineit_core::file::write_canvas(&c))
    }

    fn paste(&mut self, text: &str, at: Option<Pos2>) {
        let Ok(c) = lineit_core::file::read_canvas(text, "the clipboard") else { return };
        if c.squares.is_empty() {
            return;
        }
        let tab = self.tab();
        let mut next = tab.canvas.next_id();
        let mut ids = HashMap::new();
        let min_x = c.squares.iter().map(|s| s.x).fold(f32::INFINITY, f32::min);
        let min_y = c.squares.iter().map(|s| s.y).fold(f32::INFINITY, f32::min);
        let (dx, dy) = match at {
            Some(p) => (p.x - min_x, p.y - min_y),
            None => (32.0, 32.0),
        };
        tab.selected.clear();
        for mut s in c.squares {
            ids.insert(s.id, next);
            s.id = next;
            s.x += dx;
            s.y += dy;
            tab.selected.insert(next);
            tab.canvas.squares.push(s);
            next += 1;
        }
        for l in c.links {
            if let (Some(a), Some(b)) = (ids.get(&l.from), ids.get(&l.to)) {
                tab.canvas.links.push(Link { from: *a, to: *b, ..l });
            }
        }
    }

    fn delete_selection(&mut self) {
        let tab = self.tab();
        let sel = std::mem::take(&mut tab.selected);
        for id in sel {
            tab.canvas.remove(id);
        }
        let links = std::mem::take(&mut tab.selected_links);
        tab.canvas.links.retain(|l| !links.contains(l));
    }

    fn add_square(&mut self, path: &str, at: Pos2, pending: Option<Pending>) {
        let reg = self.reg.clone();
        let step = self.cfg.num("canvas.grid_step") as f32;
        let vim = self.vim_on();
        let tab = self.tab();
        let id = tab.canvas.next_id();
        let (x, y) = ((at.x / step).round() * step, (at.y / step).round() * step);
        let mut sq = Square::new(id, path, x, y);
        // a square added at the end of a wire goes on its side of it
        if let Some(Pending::To(..)) = &pending {
            sq.x -= 200.0;
        }
        tab.canvas.add(sq);
        tab.update(&reg, true, true);
        if let Some(p) = pending
            && let Some(info) = tab.lowered.squares.get(&id).cloned()
        {
            match p {
                Pending::From(from, output, ty) => {
                    if let Some(input) = canvas::first_fit(&info, &ty, false) {
                        tab.canvas.connect(Link { from, output, to: id, input }, ty.is_some());
                    }
                }
                Pending::To(to, input, ty) => {
                    if let Some(output) = canvas::first_fit(&info, &ty, true) {
                        tab.canvas.connect(Link { from: id, output, to, input }, ty.is_some());
                    }
                }
            }
        }
        tab.selected.clear();
        tab.selected.insert(id);
        // typing goes to its first field
        let reg = self.reg.clone();
        let (_, tab, state) = &mut self.tabs[self.active];
        tab.update(&reg, true, true);
        state.vim.reveal = Some(id);
        if vim && tab.canvas.camera.zoom < canvas::EDIT_ZOOM {
            // too small for text boxes: the new square couldn't be typed in
            let cam = &mut tab.canvas.camera;
            (cam.zoom, cam.x, cam.y) = (canvas::EDIT_ZOOM, x + 100.0, y + 40.0);
        }
        canvas::start_first(tab, &reg, state, id);
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        // a field open (typed in, or a checkbox chosen) has the keys
        let typing = ctx.memory(|m| m.focused().is_some()) || self.tabs[self.active].2.edit.is_some();
        let popup = self.search.is_some() || self.prompt.is_some();
        let cmd = Modifiers::COMMAND;
        let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
        let key = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_key(m, k));
        if key(cmd_shift, Key::S) {
            self.ask(PromptKind::SaveAs, ctx);
        } else if key(cmd, Key::S) {
            self.save(ctx);
        }
        if key(cmd, Key::O) {
            self.ask(PromptKind::Open, ctx);
        }
        if key(cmd_shift, Key::T) {
            if let Some((c, p)) = self.closed.pop() {
                let t = Tab::new(c, p, &self.reg);
                self.add_tab(t);
            }
        } else if key(cmd, Key::T) {
            let t = Tab::empty(&self.reg);
            self.add_tab(t);
        }
        if key(cmd, Key::W) {
            self.close_tab(self.active);
        }
        if key(cmd_shift, Key::Tab) || key(cmd, Key::PageUp) {
            self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
        } else if key(cmd, Key::Tab) || key(cmd, Key::PageDown) {
            self.active = (self.active + 1) % self.tabs.len();
        }
        if key(cmd, Key::Comma) {
            self.open_settings();
        }
        if key(Modifiers::SHIFT, Key::F5) {
            self.stop();
        } else if key(Modifiers::NONE, Key::F5) {
            self.start(RunMode::Run, ctx);
        }
        if key(Modifiers::NONE, Key::F10) {
            self.start(RunMode::Step, ctx);
        }
        // in the Settings tab, Esc empties its search first
        if self.tab().settings && !self.settings_query.is_empty() && key(Modifiers::NONE, Key::Escape) {
            self.settings_query.clear();
        }
        if key(Modifiers::NONE, Key::Escape) && self.search.take().is_none() && self.prompt.take().is_none() {
            self.tab().selected.clear();
            self.tab().selected_links.clear();
        }
        // the Settings tab has no canvas to act on
        if typing || popup || self.tab().settings {
            return;
        }
        if key(cmd_shift, Key::Z) || key(cmd, Key::Y) {
            self.tab().redo();
        } else if key(cmd, Key::Z) {
            self.tab().undo();
        }
        if key(cmd, Key::A) {
            let tab = self.tab();
            tab.selected = tab.canvas.squares.iter().map(|s| s.id).collect();
        }
        if key(cmd, Key::D)
            && let Some(t) = self.selection_text()
        {
            self.paste(&t, None);
        }
        if key(cmd, Key::G) && !self.tab().selected.is_empty() {
            self.subgraph_of = self.tab().selected.clone();
            self.ask(PromptKind::SubgraphName, ctx);
        }
        if key(Modifiers::NONE, Key::Delete) || key(Modifiers::NONE, Key::Backspace) {
            self.delete_selection();
        }
        let events = ctx.input(|i| i.events.clone());
        for e in events {
            match e {
                egui::Event::Copy => {
                    if let Some(t) = self.selection_text() {
                        ctx.copy_text(t);
                    }
                }
                egui::Event::Cut => {
                    if let Some(t) = self.selection_text() {
                        ctx.copy_text(t);
                        self.delete_selection();
                    }
                }
                egui::Event::Paste(text) => self.paste(&text, None),
                _ => {}
            }
        }
    }

    // ------------------------------------------------------------ popups

    fn search_ui(&mut self, ctx: &egui::Context) {
        let Some(s) = &mut self.search else { return };
        // words as typed match names with _ ("not equal" is not_equal)
        let q = s.query.trim().to_lowercase().replace(' ', "_");
        let mut matches: Vec<(usize, &String, &Arc<dyn lineit_core::registry::Module>)> =
            self.reg.modules.iter().filter_map(|(p, m)| search_score(p, &q).map(|score| (score, p, m))).collect();
        matches.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)));
        let matches: Vec<(&String, &Arc<dyn lineit_core::registry::Module>)> =
            matches.into_iter().map(|(_, p, m)| (p, m)).collect();
        s.selected = s.selected.min(matches.len().saturating_sub(1));
        // the arrows, or Ctrl+N and Ctrl+P (Ctrl, not Cmd, on macOS too)
        let (up, down, enter) = ctx.input_mut(|i| {
            let up = i.consume_key(Modifiers::NONE, Key::ArrowUp) | i.consume_key(Modifiers::CTRL, Key::P);
            let down = i.consume_key(Modifiers::NONE, Key::ArrowDown) | i.consume_key(Modifiers::CTRL, Key::N);
            (up, down, i.consume_key(Modifiers::NONE, Key::Enter))
        });
        // round: up from the first goes to the last, down from the last to the first
        let n = matches.len();
        if up && n > 0 {
            s.selected = (s.selected + n - 1) % n;
        }
        if down && n > 0 {
            s.selected = (s.selected + 1) % n;
        }
        let mut chosen: Option<String> = if enter { matches.get(s.selected).map(|(p, _)| (*p).clone()) } else { None };
        let mut close = false;
        let area = egui::Area::new(egui::Id::unique("lineit search"))
            .fixed_pos(s.screen)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(380.0);
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut s.query)
                            .hint_text("add a module: type to search")
                            .desired_width(f32::INFINITY),
                    );
                    r.request_focus();
                    // always 10 rows tall, with its scrollbar showing, so it's clear
                    // when there are more
                    let row = ui.spacing().interact_size.y + ui.spacing().item_spacing.y;
                    // a solid bar, not egui's faint floating one
                    ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
                    egui::ScrollArea::vertical()
                        .min_scrolled_height(10.0 * row)
                        .max_height(10.0 * row)
                        .auto_shrink([false, false])
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                        .show(ui, |ui| {
                            for (i, (p, m)) in matches.iter().enumerate() {
                                let resp = ui.selectable_label(i == s.selected, p.as_str()).on_hover_text(m.doc());
                                if i == s.selected && (up || down) {
                                    resp.scroll_to_me(None);
                                }
                                if resp.clicked() {
                                    chosen = Some((*p).clone());
                                }
                            }
                            if matches.is_empty() {
                                ui.weak("nothing matches");
                            }
                        });
                    // what the selected one does, and how many there are
                    ui.separator();
                    let doc = matches.get(s.selected).map(|(_, m)| m.doc()).unwrap_or("");
                    ui.add(egui::Label::new(egui::RichText::new(doc).small()).truncate());
                    ui.weak(
                        egui::RichText::new(format!("{} of {} modules", matches.len(), self.reg.modules.len())).small(),
                    );
                });
            });
        let clicked_outside = ctx.input(|i| i.pointer.any_pressed())
            && s.opened.elapsed() > Duration::from_millis(200)
            && ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| !area.response.rect.contains(p));
        if clicked_outside {
            close = true;
        }
        if let Some(path) = chosen {
            let (at, pending) = (s.at, s.pending.clone());
            self.search = None;
            self.add_square(&path, at, pending);
        } else if close {
            self.search = None;
        }
    }

    fn prompt_ui(&mut self, ctx: &egui::Context) {
        let Some(p) = &mut self.prompt else { return };
        let mut done = false;
        let mut cancel = false;
        let tab_pressed = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab));
        let entries = completions(&p.text);
        if tab_pressed && let Some(c) = complete(&p.text, &entries) {
            p.text = c;
        }
        egui::Modal::new(egui::Id::unique("lineit prompt")).show(ctx, |ui| {
            ui.set_width(520.0);
            ui.label(egui::RichText::new(&p.title).strong());
            let r = ui.add(
                egui::TextEdit::singleline(&mut p.text).desired_width(f32::INFINITY).font(egui::TextStyle::Monospace),
            );
            if p.focus || tab_pressed {
                r.request_focus();
                if let Some(mut state) = egui::TextEdit::load_state(ctx, r.id) {
                    let end = egui::text::CCursor::new(p.text.chars().count());
                    state.cursor.set_char_range(Some(egui::text::CCursorRange::one(end)));
                    state.store(ctx, r.id);
                }
                p.focus = false;
            }
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                done = true;
            }
            if let Some(e) = &p.error {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            if p.kind != PromptKind::SubgraphName && !entries.is_empty() {
                ui.weak(format!("Tab completes: {}", entries.iter().take(12).cloned().collect::<Vec<_>>().join("  ")));
            }
            ui.horizontal(|ui| {
                if ui.button("OK").clicked() {
                    done = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
        if cancel {
            self.prompt = None;
        } else if done {
            self.prompt_done(ctx);
        }
    }

    /// Shows the Settings tab, opened if it isn't.
    fn open_settings(&mut self) {
        // vim mode starts on the rows: / goes to the search
        self.settings_focus = !self.vim_on();
        match self.tabs.iter().position(|(_, t, _)| t.settings) {
            Some(i) => self.active = i,
            None => {
                let t = Tab::settings_tab(&self.reg);
                self.add_tab(t);
            }
        }
    }

    /// The Settings tab: every setting, searched, in sections (listed on the
    /// left, to go to one). A change is written to settings.lua and applies
    /// at once; Reset takes it out of there, back to config.lua's or the
    /// default.
    fn settings_page(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let theme = self.theme.clone();
        let mut themes: Vec<String> = theme::NAMES.iter().map(|s| s.to_string()).collect();
        themes.extend(self.cfg.themes.keys().filter(|k| !theme::NAMES.contains(&k.as_str())).cloned());
        let mut changed = false;
        // Ctrl+F goes to the search, Esc empties it
        if ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::F)) {
            self.settings_focus = true;
        }
        ui.heading("Settings");
        ui.add_space(4.0);
        let search = egui::Frame::new()
            .fill(theme.field)
            .stroke(egui::Stroke::new(1.0, theme.border))
            .inner_margin(egui::Margin::symmetric(8, 5))
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.settings_query)
                        .hint_text("Search settings: a name, a word of what it does...")
                        .frame(egui::Frame::NONE)
                        .desired_width(f32::INFINITY),
                )
            })
            .inner;
        if std::mem::take(&mut self.settings_focus) {
            search.request_focus();
        }
        if search.has_focus() && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.settings_query.clear();
        }
        // every word somewhere: in its name, its section, its key or what it does
        let words: Vec<String> = self.settings_query.to_lowercase().split_whitespace().map(String::from).collect();
        let shown: Vec<&lineit_core::config::SettingDef> = SETTINGS
            .iter()
            .filter(|d| {
                let hay = format!("{} {} {} {}", pretty(d.key), section(d.key), d.key, d.doc).to_lowercase();
                words.iter().all(|w| hay.contains(w.as_str()))
            })
            .collect();
        ui.horizontal(|ui| {
            let count = if words.is_empty() {
                format!("{} settings", SETTINGS.len())
            } else {
                format!("{} of {} settings", shown.len(), SETTINGS.len())
            };
            ui.label(egui::RichText::new(count).color(theme.dim).small());
            ui.label(
                egui::RichText::new(format!(" · changes are saved to {}", self.cfg.dir.join("settings.lua").display()))
                    .color(theme.dim)
                    .small(),
            );
        });
        ui.separator();
        let mut sections: Vec<&'static str> = Vec::new();
        for d in &shown {
            let sec = section(d.key);
            if !sections.contains(&sec) {
                sections.push(sec);
            }
        }
        // vim mode: the row the cursor is on, and what the keys asked of it
        let vim = self.vim_on();
        let (sel, op, moved) = if vim { self.settings_cursor(&shown, &sections) } else { (usize::MAX, None, false) };
        // as wide as its widest entry, which never wraps, in whatever font
        let entries: Vec<(&'static str, String)> = sections
            .iter()
            .map(|sec| (*sec, format!("{sec}  ({})", shown.iter().filter(|d| section(d.key) == *sec).count())))
            .collect();
        let font = egui::TextStyle::Button.resolve(ui.style());
        let widest = entries
            .iter()
            .map(|(_, e)| ui.painter().layout_no_wrap(e.clone(), font.clone(), theme.text).size().x)
            .fold(0.0, f32::max);
        egui::Panel::left("settings sections").resizable(false).exact_size(widest + 32.0).show(ui, |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            for (sec, entry) in &entries {
                if ui.selectable_label(false, entry).clicked() {
                    self.settings_jump = Some(sec);
                }
            }
        });
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for sec in &sections {
                    let h = ui.heading(*sec);
                    if self.settings_jump == Some(sec) {
                        h.scroll_to_me(Some(egui::Align::TOP));
                        self.settings_jump = None;
                    }
                    egui::Grid::new(("settings", *sec)).num_columns(3).spacing([16.0, 10.0]).show(ui, |ui| {
                        for d in shown.iter().filter(|d| section(d.key) == *sec) {
                            let active = vim && shown.iter().position(|x| x.key == d.key) == Some(sel);
                            let cell = ui.vertical(|ui| {
                                // wide enough for a description on a line or two
                                ui.set_width(360.0);
                                ui.label(pretty(d.key));
                                ui.label(egui::RichText::new(d.doc).color(theme.dim).small());
                            });
                            if active {
                                let r = cell.response.rect.expand(4.0);
                                ui.painter().rect_stroke(
                                    r,
                                    0.0,
                                    egui::Stroke::new(2.0, theme.accent),
                                    egui::StrokeKind::Outside,
                                );
                                if moved {
                                    ui.scroll_to_rect(r.expand(40.0), None);
                                }
                            }
                            let op = if active { op.clone() } else { None };
                            let v = self.cfg.get(d.key);
                            let new = match (&d.kind, v.clone()) {
                                (SKind::Bool, SValue::Bool(mut b)) => {
                                    if matches!(op, Some(SAct::Toggle | SAct::Activate | SAct::Change(_))) {
                                        b = !b;
                                    }
                                    ui.checkbox(&mut b, "");
                                    SValue::Bool(b)
                                }
                                (SKind::Num { min, max, step }, SValue::Num(mut n)) => {
                                    if let Some(SAct::Change(d)) = op {
                                        n = ((n + d as f64 * step).clamp(*min, *max) * 1e6).round() / 1e6;
                                    }
                                    ui.add(egui::DragValue::new(&mut n).range(*min..=*max).speed(*step));
                                    SValue::Num(n)
                                }
                                (SKind::Theme, SValue::Text(mut t)) => {
                                    if let Some(SAct::Change(d)) = op
                                        && let Some(i) = themes.iter().position(|x| *x == t)
                                    {
                                        t = themes[(i as i32 + d).rem_euclid(themes.len() as i32) as usize].clone();
                                    }
                                    egui::ComboBox::from_id_salt("theme").selected_text(&t).show_ui(ui, |ui| {
                                        for name in &themes {
                                            ui.selectable_value(&mut t, name.clone(), name);
                                        }
                                    });
                                    SValue::Text(t)
                                }
                                (SKind::Font, SValue::Text(mut t)) => {
                                    // typed, with the fonts installed that match below it, to click
                                    ui.vertical(|ui| {
                                        // a grid cell shrinks to what's in it: this one is wide
                                        ui.set_min_width(420.0);
                                        let r = ui.add(
                                            egui::TextEdit::singleline(&mut t)
                                                .hint_text("a font's name, or a .ttf/.otf file")
                                                .desired_width(420.0),
                                        );
                                        if matches!(op, Some(SAct::Activate | SAct::Toggle)) {
                                            r.request_focus();
                                        }
                                        if !t.trim().is_empty() && lineit_core::fonts::find(&t).is_none() {
                                            ui.colored_label(
                                                theme.error,
                                                "No font installed is called that, and it isn't a file",
                                            );
                                        }
                                        // until it's the name of a family: the closest ones, to click
                                        let families = lineit_core::fonts::families();
                                        if !t.trim().is_empty() && !families.contains(&t) {
                                            let q = t.to_lowercase();
                                            let mut hits: Vec<&String> =
                                                families.iter().filter(|f| f.to_lowercase().contains(&q)).collect();
                                            hits.sort_by_key(|f| (!f.to_lowercase().starts_with(&q), f.len()));
                                            for f in hits.into_iter().take(8) {
                                                if ui.selectable_label(false, f).clicked() {
                                                    t = f.clone();
                                                }
                                            }
                                        }
                                    });
                                    SValue::Text(t)
                                }
                                (_, SValue::Text(mut t)) => {
                                    let r = ui.add_sized([420.0, 22.0], egui::TextEdit::singleline(&mut t));
                                    if matches!(op, Some(SAct::Activate | SAct::Toggle)) {
                                        r.request_focus();
                                    }
                                    SValue::Text(t)
                                }
                                (_, v) => {
                                    ui.label("");
                                    v
                                }
                            };
                            let default = (d.default)();
                            let reset = ui.add_enabled(new != default, egui::Button::new("Reset"));
                            if reset.on_hover_text("Back to the default").clicked()
                                || (active && matches!(op, Some(SAct::Reset)) && new != default)
                            {
                                self.cfg.set(d.key, default);
                                changed = true;
                            } else if new != v {
                                self.cfg.set(d.key, new);
                                changed = true;
                            }
                            ui.end_row();
                        }
                    });
                    ui.add_space(14.0);
                }
                if sections.is_empty() {
                    ui.label(egui::RichText::new("No setting matches").color(theme.dim));
                }
            });
        });
        if changed {
            self.apply_cfg(&ctx);
        }
    }

    // ------------------------------------------------------------ bars

    fn tabs_ui(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme.clone();
        egui::Panel::top("tabs").show(ui, |ui| {
            // too many to fit: they share the width, their titles cut short;
            // so many that even that doesn't fit, the strip scrolls
            let font = egui::TextStyle::Button.resolve(ui.style());
            let x_w = 16.0;
            let chrome = 8.0 + 6.0 + x_w + 6.0 + 2.0;
            let natural: f32 = self
                .tabs
                .iter()
                .map(|(_, t, _)| ui.painter().layout_no_wrap(t.title(), font.clone(), theme.text).size().x + chrome)
                .sum();
            let room = ui.available_width() - 40.0;
            let max_title =
                if natural > room { Some(((room / self.tabs.len() as f32) - chrome).max(48.0)) } else { None };
            egui::ScrollArea::horizontal()
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        let mut close = None;
                        for (i, (_, t, _)) in self.tabs.iter().enumerate() {
                            // a tab is one box: its title and its ×, filled when it's the one
                            // shown. The box takes clicks first, so the × (made after) is on top.
                            let active = i == self.active;
                            let text = if active { theme.text } else { theme.dim };
                            let mut job = egui::text::LayoutJob::simple_singleline(t.title(), font.clone(), text);
                            if let Some(w) = max_title {
                                job.wrap = egui::text::TextWrapping::truncate_at_width(w);
                            }
                            let title = ui.painter().layout_job(job);
                            let size = vec2(8.0 + title.size().x + 6.0 + x_w + 6.0, title.size().y + 8.0);
                            let (rect, r) = ui.allocate_exact_size(size, egui::Sense::click());
                            // the tab shown comes into view when it changes
                            if active && self.tab_in_view != Some(self.tabs[i].0) {
                                r.scroll_to_me(None);
                            }
                            if active {
                                ui.painter().rect_filled(rect, 0.0, theme.header);
                            } else if r.hovered() {
                                ui.painter().rect_filled(rect, 0.0, theme.header.gamma_multiply(0.5));
                            }
                            ui.painter().galley(rect.left_center() + vec2(8.0, -title.size().y / 2.0), title, text);
                            let x_rect = egui::Rect::from_center_size(
                                rect.right_center() - vec2(6.0 + x_w / 2.0, 0.0),
                                vec2(x_w, x_w),
                            );
                            // drawn centered by hand: a button's text sits by its baseline
                            let x = ui.interact(x_rect, egui::Id::unique(("tab x", i)), egui::Sense::click());
                            let x_color = if x.hovered() { theme.text } else { text };
                            ui.painter().text(x_rect.center(), egui::Align2::CENTER_CENTER, "×", font.clone(), x_color);
                            if x.on_hover_text("Close (Ctrl+W)").clicked() {
                                close = Some(i);
                            }
                            let r = match &t.path {
                                Some(p) => r.on_hover_text(p.display().to_string()),
                                None => r,
                            };
                            if r.clicked() {
                                self.active = i;
                            }
                            if r.middle_clicked() {
                                close = Some(i);
                            }
                        }
                        ui.add_space(6.0);
                        let plus = ui.add(egui::Button::new(egui::RichText::new("+").color(theme.dim)).frame(false));
                        if plus.on_hover_text("New tab (Ctrl+T)").clicked() {
                            let t = Tab::empty(&self.reg);
                            self.add_tab(t);
                        }
                        self.tab_in_view = Some(self.tabs[self.active].0);
                        if let Some(i) = close {
                            self.close_tab(i);
                        }
                    });
                });
        });
    }

    fn bar_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        egui::Panel::bottom("bar").show(ui, |ui| {
            ui.horizontal(|ui| {
                let state = self.tab().run.as_ref().map(|r| r.state());
                let live = matches!(state, Some(State::Running | State::Paused));
                // Run, Step and Compile are for canvases
                let canvas = !self.tab().settings;
                ui.add_enabled_ui(canvas, |ui| {
                    if live {
                        if ui.button("Stop").on_hover_text("Shift+F5").clicked() {
                            self.stop();
                        }
                        if ui.button("Step").on_hover_text("Run the next square of each branch (F10)").clicked() {
                            self.start(RunMode::Step, &ctx);
                        }
                        if state == Some(State::Paused)
                            && ui.button("Run").on_hover_text("Go on running (F5)").clicked()
                        {
                            self.start(RunMode::Run, &ctx);
                        }
                    } else {
                        let label = match self.mode {
                            RunMode::Run => "Run",
                            RunMode::Step => "Step",
                            RunMode::Compile => "Compile",
                        };
                        // one button, its v choosing what it does: no gap between them
                        let gap = ui.spacing().item_spacing.x;
                        ui.spacing_mut().item_spacing.x = 0.0;
                        if ui.button(label).on_hover_text("F5 runs, F10 steps").clicked() {
                            let m = self.mode;
                            self.start(m, &ctx);
                        }
                        // its arrow drawn: the fonts have no ▴
                        let arrow = ui.menu_button("   ", |ui| {
                            if ui.button("Run (F5)").clicked() {
                                self.start(RunMode::Run, &ctx);
                                ui.close();
                            }
                            if ui.button("Step (F10)").clicked() {
                                self.start(RunMode::Step, &ctx);
                                ui.close();
                            }
                            if ui.button("Compile").clicked() {
                                self.start(RunMode::Compile, &ctx);
                                ui.close();
                            }
                        });
                        let c = arrow.response.rect.center();
                        let color = ui.style().interact(&arrow.response).text_color();
                        // pointing up: the menu opens above it
                        let tri = vec![c + vec2(-4.0, 2.0), c + vec2(0.0, -3.0), c + vec2(4.0, 2.0)];
                        ui.painter().add(egui::Shape::convex_polygon(tri, color, egui::Stroke::NONE));
                        ui.spacing_mut().item_spacing.x = gap;
                    }
                });
                ui.separator();
                self.vim_status_ui(ui);
                let compiling = self.compiling.is_some();
                let theme = self.theme.clone();
                let tab = self.tab();
                let n = tab.lowered.diags.len();
                if compiling {
                    ui.spinner();
                    ui.label("compiling");
                } else if let Some(r) = &tab.run {
                    let v = r.view.lock().unwrap();
                    let text = match v.state {
                        State::Running => format!("running · {:.1}s", v.started.elapsed().as_secs_f32()),
                        State::Paused => format!(
                            "stepping: before {} square{}",
                            v.current.len(),
                            if v.current.len() == 1 { "" } else { "s" }
                        ),
                        State::Done => "done".into(),
                        State::Stopped => "stopped".into(),
                        State::Failed => "failed".into(),
                    };
                    ui.label(text);
                }
                if n > 0 {
                    let all: Vec<String> = tab
                        .lowered
                        .diags
                        .iter()
                        .map(|d| {
                            let at = d
                                .square
                                .and_then(|s| tab.canvas.square(s))
                                .map(|s| format!("{} ({}): ", s.module, s.id))
                                .unwrap_or_default();
                            format!("{at}{}", d.message)
                        })
                        .collect();
                    ui.colored_label(theme.error, format!("{n} problem{}", if n == 1 { "" } else { "s" }))
                        .on_hover_text(all.join("\n"));
                }
                if let Some((msg, err)) = &tab.status {
                    let color = if *err { theme.error } else { theme.dim };
                    let first = msg.lines().next().unwrap_or("").to_string();
                    ui.add(egui::Label::new(egui::RichText::new(&first).color(color)).truncate())
                        .on_hover_text(msg.as_str());
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.menu_button("File", |ui| {
                        if ui.button("New tab (Ctrl+T)").clicked() {
                            let t = Tab::empty(&self.reg);
                            self.add_tab(t);
                            ui.close();
                        }
                        if ui.button("Open… (Ctrl+O)").clicked() {
                            self.ask(PromptKind::Open, &ctx);
                            ui.close();
                        }
                        if ui.button("Save (Ctrl+S)").clicked() {
                            self.save(&ctx);
                            ui.close();
                        }
                        if ui.button("Save as… (Ctrl+Shift+S)").clicked() {
                            self.ask(PromptKind::SaveAs, &ctx);
                            ui.close();
                        }
                        if ui.button("Close tab (Ctrl+W)").clicked() {
                            self.close_tab(self.active);
                            ui.close();
                        }
                        ui.separator();
                        let mut auto = self.cfg.bool("save.autosave");
                        if ui.checkbox(&mut auto, "Autosave").changed() {
                            self.cfg.set("save.autosave", SValue::Bool(auto));
                            let _ = self.cfg.save();
                        }
                        if ui.button("Settings (Ctrl+,)").clicked() {
                            self.open_settings();
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Quit").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.toggle_value(&mut self.show_output, "Output");
                });
            });
        });
    }

    /// The strip with what's typed in a field, all of it, wrapped, above
    /// the output: what it's for, and its keys.
    fn editor_ui(&mut self, ui: &mut egui::Ui) {
        let theme = self.theme.clone();
        let state = &mut self.tabs[self.active].2;
        let Some(e) = state.edit.as_mut().filter(|e| !e.choose && !e.multiline) else {
            state.strip = None;
            return;
        };
        // over the bottom of the canvas, so the canvas doesn't move
        let Some(canvas) = state.rect else { return };
        let ctx = ui.ctx().clone();
        let panel = egui::Area::new(egui::Id::unique("lineit field editor"))
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::LEFT_BOTTOM)
            .fixed_pos(canvas.left_bottom())
            .show(&ctx, |ui| {
                egui::Frame::new().fill(theme.panel).stroke(egui::Stroke::new(1.0, theme.border)).inner_margin(6).show(
                    ui,
                    |ui| {
                        ui.set_width(canvas.width() - 14.0);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(&e.title).color(theme.dim));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if e.error {
                                    ui.colored_label(theme.error, "not valid: fix it, or Esc");
                                } else {
                                    ui.colored_label(theme.dim, "Enter or Esc applies, Tab next");
                                }
                            });
                        });
                        let r = ui.add(
                            egui::TextEdit::multiline(&mut e.text)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY)
                                .desired_rows(1),
                        );
                        if r.changed() {
                            e.error = false;
                        }
                    },
                );
            });
        state.strip = Some(panel.response.rect);
    }

    fn output_ui(&mut self, ui: &mut egui::Ui) {
        if !self.show_output {
            return;
        }
        let theme = self.theme.clone();
        egui::Panel::bottom("output").resizable(true).default_size(160.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Output");
                if ui.small_button("Clear").clicked() {
                    self.tab().output.clear();
                }
                if ui.small_button("Copy").clicked() {
                    let text = self.tab().output.join("\n");
                    ui.ctx().copy_text(text);
                }
            });
            let lines = &self.tabs[self.active].1.output;
            let start = lines.len().saturating_sub(2000);
            egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                if start > 0 {
                    ui.weak(format!("({start} lines before)"));
                }
                for l in &lines[start..] {
                    let color = if l.starts_with("lineit: ") { theme.dim } else { theme.text };
                    ui.label(egui::RichText::new(l).monospace().color(color));
                }
            });
        });
    }

    /// Starts working out the previews of the active tab, if its canvas
    /// changed since, and takes them when they're ready.
    fn pure_previews(&mut self, ctx: &egui::Context) -> HashMap<(SquareId, String), Value> {
        let mut got = HashMap::new();
        let ready =
            self.preview_job.as_ref().and_then(|j| j.result.lock().unwrap().take().map(|r| (j.tab, j.generation, r)));
        if let Some((tab, generation, r)) = ready {
            self.previewed.insert(tab, generation);
            got.extend(r);
            self.preview_job = None;
        }
        let (id, tab, _) = &self.tabs[self.active];
        if self.preview_job.is_none() && self.previewed.get(id) != Some(&tab.generation) {
            match &tab.lowered.program {
                Some(prog) if !tab.lowered.previews.is_empty() => {
                    let (prog, exprs, dir) = (prog.clone(), tab.lowered.previews.clone(), tab.dir());
                    let result = Arc::new(Mutex::new(None));
                    let (r, c) = (result.clone(), ctx.clone());
                    std::thread::spawn(move || {
                        let mut out = Vec::new();
                        if let Ok(mut it) = lineit_core::interp::Interp::new(prog, dir) {
                            for (sq, pin, e) in exprs {
                                if let Ok(v) = it.eval(&e, &[]) {
                                    out.push(((sq, pin), v));
                                }
                            }
                        }
                        *r.lock().unwrap() = Some(out);
                        c.request_repaint();
                    });
                    self.preview_job = Some(PreviewJob { tab: *id, generation: tab.generation, result });
                }
                _ => {
                    self.previewed.insert(*id, tab.generation);
                }
            }
        }
        got
    }

    fn previews(&mut self, ctx: &egui::Context, values: &HashMap<(SquareId, String), Value>) {
        let (_, _, state) = &mut self.tabs[self.active];
        for (k, v) in values {
            let Value::Image(img) = v else { continue };
            let same = state.previews.get(k).is_some_and(|(_, old)| Arc::ptr_eq(&old.pixels, &img.pixels));
            if same || img.width == 0 || img.height == 0 {
                continue;
            }
            // small, for the square
            let scale = (256.0 / img.width.max(img.height) as f32).min(1.0);
            let small = if scale < 1.0 {
                lineit_rt::image::resize(
                    img,
                    (img.width as f32 * scale).max(1.0) as i64,
                    (img.height as f32 * scale).max(1.0) as i64,
                )
            } else {
                img.clone()
            };
            let color =
                egui::ColorImage::from_rgba_unmultiplied([small.width as usize, small.height as usize], &small.pixels);
            let tex = ctx.load_texture(format!("preview {k:?}"), color, egui::TextureOptions::LINEAR);
            state.previews.insert(k.clone(), (tex, img.clone()));
        }
    }
}

/// NEEDLE as a subsequence of HAY (its letters in order, maybe with others
/// between: "l/" is in "list/push" and "logic/and"): the length of the
/// shortest part of HAY that has it, None if none does. Both lower case.
fn fuzzy_span(hay: &str, needle: &str) -> Option<usize> {
    let hay: Vec<char> = hay.chars().collect();
    let needle: Vec<char> = needle.chars().collect();
    if needle.is_empty() {
        return Some(0);
    }
    let mut best: Option<usize> = None;
    for start in 0..hay.len() {
        if hay[start] != needle[0] {
            continue;
        }
        let (mut q, mut i) = (1, start + 1);
        while q < needle.len() && i < hay.len() {
            if hay[i] == needle[q] {
                q += 1;
            }
            i += 1;
        }
        if q < needle.len() {
            break; // later starts can't have it either
        }
        best = Some(best.map_or(i - start, |b| b.min(i - start)));
    }
    best
}

/// How well module PATH matches the search Q (lower case), lower is
/// better, None if it doesn't: its name exactly, then paths that start
/// with Q ("im" finds image/...), names that start with it, names with it,
/// paths with it, and last paths with its letters in that order, tighter
/// first.
fn search_score(path: &str, q: &str) -> Option<usize> {
    let path = path.to_lowercase();
    let name = path.rsplit('/').next().unwrap_or(&path);
    if name == q {
        Some(0)
    } else if path.starts_with(q) {
        Some(1)
    } else if name.starts_with(q) {
        Some(2)
    } else if name.contains(q) {
        Some(3)
    } else if path.contains(q) {
        Some(4)
    } else {
        fuzzy_span(&path, q).map(|span| 5 + span)
    }
}

/// The section of the Settings tab setting KEY goes in.
fn section(key: &str) -> &'static str {
    match key.split_once('.').map(|(t, _)| t) {
        Some("font") => "Font",
        Some("canvas") => "Canvas",
        Some("save") => "Saving",
        Some("output") => "Output",
        Some("window") => "Window",
        Some("editor") => "Editor",
        _ => "General",
    }
}

/// "canvas.grid_step" as "Grid step".
fn pretty(key: &str) -> String {
    let name = key.rsplit('.').next().unwrap_or(key).replace('_', " ");
    let mut c = name.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => name,
    }
}

/// What the editor starts with: files to open, or the session of the editor
/// it takes over from.
pub enum Start {
    Files(Vec<PathBuf>),
    Session(session::Session),
}

fn config_stamp() -> (Option<std::time::SystemTime>, Option<std::time::SystemTime>) {
    let dir = lineit_core::paths::config_dir();
    let m = |f: &str| std::fs::metadata(dir.join(f)).and_then(|m| m.modified()).ok();
    (m("config.lua"), m("settings.lua"))
}

/// The files and folders the path being typed could go on with.
fn completions(text: &str) -> Vec<String> {
    let (dir, prefix) = match text.rfind(['/', '\\']) {
        Some(i) => (&text[..=i], &text[i + 1..]),
        None => ("./", text),
    };
    let Ok(entries) = std::fs::read_dir(if dir.is_empty() { "." } else { dir }) else { return Vec::new() };
    let mut v: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.starts_with(prefix) || (name.starts_with('.') && !prefix.starts_with('.')) {
                return None;
            }
            Some(if e.path().is_dir() { format!("{name}/") } else { name })
        })
        .collect();
    v.sort();
    v
}

/// TEXT, completed as far as ENTRIES agree.
fn complete(text: &str, entries: &[String]) -> Option<String> {
    let first = entries.first()?;
    let mut common = first.clone();
    for e in &entries[1..] {
        while !e.starts_with(&common) {
            common.pop();
        }
    }
    let dir = match text.rfind(['/', '\\']) {
        Some(i) => &text[..=i],
        None => "",
    };
    Some(format!("{dir}{common}"))
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.vim_hook(ctx, raw);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll();
        {
            let reg = self.reg.clone();
            let (_, tab, state) = &mut self.tabs[self.active];
            canvas::edit_keys(&ctx, tab, &reg, state);
        }
        self.vim_keys(&ctx);
        self.shortcuts(&ctx);

        self.ready.frame();
        // files sent by `lineit FILE`: open, in tabs (none: a new tab)
        let sent: Vec<Option<PathBuf>> =
            self.single.as_ref().map(|s| std::mem::take(&mut *s.inbox.lock().unwrap())).unwrap_or_default();
        if !sent.is_empty() {
            for p in sent {
                match p {
                    Some(p) => self.open(&p),
                    None => {
                        let t = Tab::empty(&self.reg);
                        self.add_tab(t);
                    }
                }
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        // what changed on disk, once a second: the binary (handed over to),
        // the config, modules and packages, and the canvases open
        if self.last_stamp_check.elapsed() > Duration::from_secs(1) {
            self.last_stamp_check = Instant::now();
            if self.exe.as_mut().is_some_and(|e| e.rebuilt()) {
                self.hand_over(&ctx);
                if self.tabs.is_empty() {
                    return;
                }
            }
            let c = config_stamp();
            if c != self.config_stamp {
                self.config_stamp = c;
                self.reload_config(&ctx);
            }
            let s = self.module_stamp();
            if s != self.modules_stamp {
                self.modules_stamp = s;
                self.reload_modules();
            }
            let reg = self.reg.clone();
            for (_, t, _) in &mut self.tabs {
                t.reload_from_disk(&reg);
            }
        }
        ctx.request_repaint_after(Duration::from_secs(1));
        if self.cfg.bool("save.autosave") && self.last_autosave.elapsed().as_secs_f64() > self.cfg.num("save.interval")
        {
            self.last_autosave = Instant::now();
            for (_, t, _) in &mut self.tabs {
                if let (true, Some(p)) = (t.dirty(), t.path.clone()) {
                    let _ = t.save_to(&p);
                }
            }
        }
        // files dropped on the window: canvases open, images become image/open squares
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if !dropped.is_empty() {
            let at = ctx.input(|i| i.pointer.hover_pos()).unwrap_or_default();
            let mut n = 0.0;
            for f in dropped {
                let path = f.path().to_path_buf();
                if path.as_os_str().is_empty() {
                    continue;
                }
                let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                if ext == "lua" {
                    self.open(&path);
                } else if !self.tab().settings
                    && ["png", "jpg", "jpeg", "gif", "bmp", "qoi", "tga"].contains(&ext.as_str())
                {
                    let cam = self.tab().canvas.camera.clone();
                    let world = pos_world(&cam, at, ctx.content_rect().center()) + vec2(0.0, n);
                    let tab_dir = self.tab().dir();
                    let rel = path.strip_prefix(&tab_dir).map(Path::to_path_buf).unwrap_or(path.clone());
                    self.add_square("image/open", world, None);
                    let tab = self.tab();
                    if let Some(id) = tab.selected.iter().next().copied()
                        && let Some(sq) = tab.canvas.square_mut(id)
                    {
                        sq.fields.insert("path".into(), rel.display().to_string().replace('\\', "/"));
                    }
                    n += 120.0;
                }
            }
        }

        self.tabs_ui(ui);
        self.bar_ui(ui);
        self.output_ui(ui);
        self.editor_ui(ui);

        let (values, current): (HashMap<(SquareId, String), Value>, BTreeSet<SquareId>) = match &self.tab().run {
            Some(r) => {
                let v = r.view.lock().unwrap();
                (v.values.clone(), v.current.iter().copied().collect())
            }
            None => (HashMap::new(), BTreeSet::new()),
        };
        if self.cfg.bool("canvas.previews") {
            let pure = self.pure_previews(&ctx);
            self.previews(&ctx, &pure);
            self.previews(&ctx, &values);
        }
        let mut actions = Vec::new();
        if self.tab().settings {
            egui::CentralPanel::default().show(ui, |ui| self.settings_page(ui));
        } else {
            egui::CentralPanel::no_frame().show(ui, |ui| {
                let reg = self.reg.clone();
                let (tab_id, tab, state) = &mut self.tabs[self.active];
                let mut cx = canvas::Ctx {
                    tab,
                    tab_id: *tab_id,
                    reg: &reg,
                    theme: &self.theme,
                    cfg: &self.cfg,
                    state,
                    actions: Vec::new(),
                    values,
                    current,
                    pending: self.search.as_ref().and_then(|s| s.pending.clone().map(|p| (p, s.at))),
                };
                canvas::show(ui, &mut cx);
                actions = cx.actions;
            });
        }
        for a in actions {
            match a {
                Action::Search { at, screen, pending } => {
                    self.search =
                        Some(Search { at, screen, query: String::new(), pending, selected: 0, opened: Instant::now() });
                }
                Action::Delete => self.delete_selection(),
                Action::Duplicate => {
                    if let Some(t) = self.selection_text() {
                        self.paste(&t, None);
                    }
                }
                Action::Copy => {
                    if let Some(t) = self.selection_text() {
                        ctx.copy_text(t);
                    }
                }
                Action::MakeSubgraph => {
                    self.subgraph_of = self.tab().selected.clone();
                    self.ask(PromptKind::SubgraphName, &ctx);
                }
                Action::OpenModule(path) => {
                    let file = self.reg.get(&path).and_then(|m| m.subgraph()).and_then(|s| s.file.clone());
                    match file {
                        Some(f) => self.open(&f),
                        None => self.status(&format!("{path} has no file to open"), true),
                    }
                }
            }
        }
        self.search_ui(&ctx);
        self.prompt_ui(&ctx);
        self.vim_help_ui(&ctx);

        let settling = ctx.input(|i| i.pointer.any_down()) || ctx.memory(|m| m.focused().is_some());
        let reg = self.reg.clone();
        for (_, t, _) in &mut self.tabs {
            t.update(&reg, settling, false);
        }
        let title = format!("{} — lineit", self.tabs[self.active].1.title());
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        if self.tabs.iter().any(|(_, t, _)| t.is_running()) || self.compiling.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}

fn pos_world(cam: &lineit_core::model::Camera, screen: Pos2, origin: Pos2) -> Pos2 {
    let v = canvas::View { origin, cam: vec2(cam.x, cam.y), zoom: cam.zoom };
    v.to_world(screen)
}

#[cfg(test)]
mod tests {
    use super::search_score;

    fn order(q: &str, paths: &[&str]) -> Vec<String> {
        let mut v: Vec<(usize, &str)> = paths.iter().filter_map(|p| search_score(p, q).map(|s| (s, *p))).collect();
        v.sort();
        v.into_iter().map(|(_, p)| p.to_string()).collect()
    }

    #[test]
    fn search_order() {
        let paths = ["string/trim", "value/time", "image/open", "image/blur", "math/add", "map/delete"];
        assert_eq!(order("im", &paths), ["image/blur", "image/open", "string/trim", "value/time"]);
        assert_eq!(order("add", &paths), ["math/add"]);
        assert_eq!(order("mdl", &paths), ["map/delete"]);
    }
}

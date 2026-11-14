//! The tests of code squares in the editor: a line on the square that says
//! how they went, and the table to write them in (see
//! [`lineit_core::codetest`]). They run on a thread of their own (a Rust
//! or C square builds first), when the table opens and when it's told to.

use std::collections::{BTreeMap, HashMap};
use std::sync::mpsc;

use eframe::egui;
use lineit_core::codetest::{self, FIRES, Row, RowResult};
use lineit_core::model::SquareId;
use lineit_core::registry::code::shared_pin;

use super::tab::Tab;
use super::theme::Theme;

type Results = Result<Vec<RowResult>, String>;

/// What Run tests found: each `test/case` and each code square's tests.
pub struct Report {
    pub cases: Vec<codetest::CaseResult>,
    pub squares: Vec<codetest::SquareTests>,
    /// What the canvas was when they ran ([`content_key`]): another, and
    /// it may say what isn't so any more.
    pub key: u64,
    /// What each case gave, when they ran.
    pub given: HashMap<SquareId, String>,
}

/// What test/case CASE of CANVAS gives (as L lowered it): what's typed in
/// its fields (those it has: the canvas's variables), and what's wired.
fn given_of(canvas: &lineit_core::model::Canvas, l: &lineit_core::lower::Lowered, case: SquareId) -> String {
    let (Some(sq), Some(info)) = (canvas.square(case), l.squares.get(&case)) else { return String::new() };
    let mut v = Vec::new();
    for p in &info.sig.inputs {
        if canvas.links_into(case, &p.name).next().is_some() {
            v.push(format!("{} (wired)", p.name));
        } else if let Some(t) = sq.fields.get(&p.name).filter(|t| !t.trim().is_empty()) {
            v.push(format!("{} = {t}", p.name));
        }
    }
    v.join(", ")
}

/// What a canvas's tests depend on: its squares (but not where they are)
/// and its wires.
fn content_key(canvas: &lineit_core::model::Canvas) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for s in &canvas.squares {
        (s.id, &s.module, &s.params, &s.fields).hash(&mut h);
    }
    canvas.links.hash(&mut h);
    h.finish()
}

impl Report {
    /// How many tests passed, and failed.
    fn counts(&self) -> (usize, usize) {
        let mut ok = self.cases.iter().filter(|c| c.ok()).count();
        let mut bad = self.cases.len() - ok;
        for s in &self.squares {
            match &s.results {
                Ok(rows) => {
                    let p = rows.iter().filter(|r| r.ok()).count();
                    ok += p;
                    bad += rows.len() - p;
                }
                Err(_) => bad += s.rows.max(1),
            }
        }
        (ok, bad)
    }
}

#[derive(Default)]
pub struct TestsUi {
    /// What Run tests found, the last time; whether its window is open.
    report: Option<Report>,
    report_rx: Option<mpsc::Receiver<Report>>,
    /// The test being debugged, and its name: when its run ends, the status
    /// says how it went.
    pub debugging: Option<(SquareId, String)>,
    /// The canvas as the tab has it now: its generation, and its key.
    now: (u64, u64),
    /// The square whose table is open.
    pub open: Option<SquareId>,
    /// How each square's tests went, the last time they ran, and the text
    /// of the tests they ran.
    results: HashMap<SquareId, (String, Results)>,
    running: HashMap<SquareId, (String, mpsc::Receiver<Results>)>,
}

/// What the line on a square says, and in which color.
pub enum Badge {
    None,
    Running,
    /// Passed, out of.
    Ran(usize, usize),
    Failed(String),
    /// They changed since they ran.
    NotRun(usize),
}

impl TestsUi {
    /// Runs the tests of TAB's canvas, all of them, on a thread: the report
    /// opens with what they found.
    pub fn run_all(&mut self, tab: &Tab, reg: std::sync::Arc<lineit_core::registry::Registry>) {
        let canvas = tab.canvas.clone();
        let key = content_key(&canvas);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let l = lineit_core::lower::lower(&canvas, &reg);
            let squares = codetest::run_canvas(&canvas, &l);
            let cases = codetest::run_cases(&canvas, &reg);
            let given = cases.iter().map(|c| (c.square, given_of(&canvas, &l, c.square))).collect();
            let _ = tx.send(Report { cases, squares, key, given });
        });
        self.report_rx = Some(rx);
    }

    /// Takes what finished running.
    pub fn poll(&mut self, ctx: &egui::Context) {
        if let Some(r) = self.report_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.report = Some(r);
            self.report_rx = None;
        }
        if self.report_rx.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        let mut done = Vec::new();
        for (sq, (text, rx)) in &self.running {
            if let Ok(r) = rx.try_recv() {
                done.push((*sq, text.clone(), r));
            }
        }
        for (sq, text, r) in done {
            self.running.remove(&sq);
            self.results.insert(sq, (text, r));
        }
        if !self.running.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    pub fn badge(&self, tab: &Tab, sq: SquareId) -> Badge {
        let text = tests_text(tab, sq);
        let n = codetest::read_rows(&text).map(|r| r.len()).unwrap_or(0);
        if self.running.contains_key(&sq) {
            return Badge::Running;
        }
        match self.results.get(&sq) {
            _ if n == 0 => Badge::None,
            Some((ran, _)) if *ran != text => Badge::NotRun(n),
            Some((_, Ok(rows))) => Badge::Ran(rows.iter().filter(|r| r.ok()).count(), rows.len()),
            Some((_, Err(e))) => Badge::Failed(e.clone()),
            None => Badge::NotRun(n),
        }
    }

    /// Runs SQ's tests, on a thread.
    pub fn run(&mut self, tab: &Tab, sq: SquareId) {
        let text = tests_text(tab, sq);
        let (tx, rx) = mpsc::channel();
        let rows = codetest::read_rows(&text);
        let info = tab.lowered.squares.get(&sq).cloned();
        let square = tab.canvas.square(sq).cloned();
        std::thread::spawn(move || {
            let r = (|| {
                let rows = rows?;
                let info = info.ok_or("its code has a problem")?;
                let f = info.sig.code.clone().ok_or("its code has a problem")?;
                let square = square.ok_or("it isn't there")?;
                let mut defaults = BTreeMap::new();
                for p in &info.sig.inputs {
                    if let Some(d) = &p.field {
                        defaults.insert(p.name.clone(), square.fields.get(&p.name).unwrap_or(d).clone());
                    }
                }
                let lang = square
                    .params
                    .get("language")
                    .cloned()
                    .unwrap_or_else(lineit_core::registry::code::default_language);
                let kind = lineit_core::registry::code::ext_of(&lang, &f)?;
                codetest::run(&f, &kind, &rows, &defaults)
            })();
            let _ = tx.send(r);
        });
        self.running.insert(sq, (text, rx));
    }
}

fn tests_text(tab: &Tab, sq: SquareId) -> String {
    tab.canvas.square(sq).and_then(|s| s.params.get("tests").cloned()).unwrap_or_default()
}

/// The columns of the table: what a row gives, and what it expects.
fn columns(tab: &Tab, sq: SquareId) -> (Vec<String>, Vec<String>) {
    let Some(f) = tab.lowered.squares.get(&sq).and_then(|i| i.sig.code.clone()) else {
        return (Vec::new(), Vec::new());
    };
    let mut given: Vec<String> = f.inputs().map(|(n, _)| n.to_string()).collect();
    given.extend(f.state.iter().cloned());
    given.extend(f.shared.iter().map(|(g, _)| shared_pin(g)));
    // what it fires, for one that runs on ticks
    let mut expect = if f.ticked() { vec![FIRES.to_string()] } else { Vec::new() };
    if f.exits().next().is_some() {
        for (_, values) in f.exits() {
            expect.extend(values.iter().map(|(p, _)| p.clone()));
        }
    } else {
        expect.extend(f.returns.iter().map(|(p, _)| p.clone()));
    }
    expect.extend(f.state.iter().cloned());
    expect.extend(f.shared.iter().filter(|(_, s)| *s).map(|(g, _)| shared_pin(g)));
    (given, expect)
}

/// The table of the square that has it open, if any.
pub fn window(ctx: &egui::Context, tab: &mut Tab, ui_state: &mut TestsUi, theme: &Theme) {
    let Some(sq) = ui_state.open else { return };
    let Some(square) = tab.canvas.square(sq) else {
        ui_state.open = None;
        return;
    };
    let text = square.params.get("tests").cloned().unwrap_or_default();
    // tests that can't be read stay as they are: the table would lose them
    let mut rows = match codetest::read_rows(&text) {
        Ok(r) => r,
        Err(e) => {
            let mut open = true;
            egui::Window::new("Tests").id(egui::Id::unique(("tests", sq))).open(&mut open).collapsible(false).show(
                ctx,
                |ui| {
                    ui.colored_label(theme.error, format!("Its tests can't be read: {e}"));
                    ui.label("Fix them in the canvas's file (the square's `tests`).");
                },
            );
            if !open {
                ui_state.open = None;
            }
            return;
        }
    };
    let name = tab.lowered.squares.get(&sq).and_then(|i| i.sig.code.as_ref().map(|f| f.name.clone()));
    let (given, expect) = columns(tab, sq);
    let results = ui_state.results.get(&sq).filter(|(ran, _)| *ran == text).map(|(_, r)| r.clone());
    let mut open = true;
    let mut changed = false;
    let mut run = false;
    egui::Window::new(format!("Tests of {}", name.as_deref().unwrap_or("the square")))
        .id(egui::Id::unique(("tests", sq)))
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(640.0)
        .show(ctx, |ui| {
            if given.is_empty() && expect.len() <= 1 {
                ui.label("Its code has a problem: fix it to write its tests.");
                return;
            }
            ui.weak(
                "Each row is a call. Given: its inputs, its state, the globals (a row with no state goes on from the one before). Expected: what it fires (`even`, `item, item`), its outputs, its state after. Empty: not checked. Lists, maps and records are Lua tables: {1, 2}, {a = 1}.",
            );
            ui.add_space(6.0);
            egui::ScrollArea::both().max_height(360.0).show(ui, |ui| {
                egui::Grid::new(("tests grid", sq)).striped(true).spacing([6.0, 4.0]).show(ui, |ui| {
                    // which half each column is in, over the first of each
                    ui.label("");
                    for k in 0..given.len() {
                        if k == 0 { ui.weak("given") } else { ui.label("") };
                    }
                    for k in 0..expect.len() {
                        if k == 0 { ui.weak("expected") } else { ui.label("") };
                    }
                    ui.label("");
                    ui.end_row();
                    ui.label("");
                    for g in given.iter().chain(expect.iter()) {
                        ui.label(egui::RichText::new(g).strong());
                    }
                    ui.label("");
                    ui.end_row();
                    let mut remove = None;
                    for (k, row) in rows.iter_mut().enumerate() {
                        // how it went
                        match results.as_ref().and_then(|r| r.as_ref().ok()).and_then(|r| r.get(k)) {
                            Some(r) if r.ok() => {
                                let l = ui.colored_label(egui::Color32::from_rgb(110, 190, 110), "ok");
                                if !r.printed.is_empty() {
                                    l.on_hover_text(format!("printed:\n{}", r.printed.join("\n")));
                                }
                            }
                            Some(r) => {
                                let mut tip = r.problems.join("\n");
                                if !r.printed.is_empty() {
                                    tip.push_str(&format!("\n\nprinted:\n{}", r.printed.join("\n")));
                                }
                                ui.colored_label(theme.error, "failed").on_hover_text(tip);
                            }
                            None => {
                                ui.weak(format!("{}", k + 1));
                            }
                        }
                        for (cols, half) in [(&given, &mut row.given), (&expect, &mut row.expect)] {
                            for c in cols.iter() {
                                let mut v = half.get(c).cloned().unwrap_or_default();
                                let r = ui.add(egui::TextEdit::singleline(&mut v).desired_width(64.0).font(egui::TextStyle::Monospace));
                                if r.changed() {
                                    if v.is_empty() {
                                        half.remove(c);
                                    } else {
                                        half.insert(c.clone(), v);
                                    }
                                    changed = true;
                                }
                            }
                        }
                        if ui.small_button("×").on_hover_text("Remove this test").clicked() {
                            remove = Some(k);
                        }
                        ui.end_row();
                    }
                    if let Some(k) = remove {
                        rows.remove(k);
                        changed = true;
                    }
                });
            });
            // what went wrong, in words, under the table
            if let Some(r) = &results {
                match r {
                    Err(e) => {
                        ui.colored_label(theme.error, e);
                    }
                    Ok(rs) => {
                        for (k, r) in rs.iter().enumerate().filter(|(_, r)| !r.ok()) {
                            for p in &r.problems {
                                ui.colored_label(theme.error, format!("test {}: {p}", k + 1));
                            }
                        }
                    }
                }
            }
            ui.horizontal(|ui| {
                if ui.button("Add a test").clicked() {
                    rows.push(Row::default());
                    changed = true;
                }
                let running = ui_state.running.contains_key(&sq);
                if ui.add_enabled(!running, egui::Button::new(if running { "Running..." } else { "Run the tests" })).clicked() {
                    run = true;
                }
            });
        });
    if changed && let Some(s) = tab.canvas.square_mut(sq) {
        s.params.insert("tests".into(), codetest::write_rows(&rows));
    }
    if run {
        ui_state.run(tab, sq);
    }
    if !open {
        ui_state.open = None;
    }
}

/// What the report asks the editor to do.
pub enum ReportAction {
    RunTests,
    /// Run this `test/case` by steps.
    Debug(SquareId),
    /// Select this square, and show it.
    Show(SquareId),
}

/// The report of Run tests, in the bottom panel: what failed first, and
/// why; what passed below. A test of the canvas runs by steps from it
/// (Debug); a click on a test or a check shows its square.
pub fn report_ui(ui: &mut egui::Ui, ui_state: &mut TestsUi, theme: &Theme) -> Vec<ReportAction> {
    let mut actions = Vec::new();
    let green = egui::Color32::from_rgb(110, 190, 110);
    let running = ui_state.report_rx.is_some();
    // a link-like label: shows its square when clicked
    let link = |ui: &mut egui::Ui, text: egui::RichText, sq: SquareId, actions: &mut Vec<ReportAction>| {
        if ui.add(egui::Label::new(text).sense(egui::Sense::click())).on_hover_text("Show it on the canvas").clicked() {
            actions.push(ReportAction::Show(sq));
        }
    };
    ui.horizontal(|ui| {
        if ui.add_enabled(!running, egui::Button::new("Run tests")).on_hover_text("F6").clicked() {
            actions.push(ReportAction::RunTests);
        }
        match (&ui_state.report, running) {
            (_, true) => {
                ui.spinner();
                ui.label("running the tests...");
            }
            (Some(r), false) => {
                let (ok, bad) = r.counts();
                if !ui_state.fresh() {
                    ui.colored_label(theme.dim, "(the canvas changed since: F6 runs them again)");
                }
                if ok + bad == 0 {
                    ui.label(
                        "No tests yet: add a test/case square (and test/expect squares), or tests to a code square.",
                    );
                } else if bad == 0 {
                    ui.colored_label(green, format!("all {ok} passed"));
                } else {
                    ui.colored_label(theme.error, format!("{bad} failed"));
                    ui.label(format!("{ok} passed"));
                }
            }
            (None, false) => {
                ui.weak("Run tests runs each test/case of the canvas, and the tests of its code squares.");
            }
        }
    });
    let Some(r) = &ui_state.report else { return actions };
    ui.separator();
    // what a case gave, when it ran
    let given = |case: SquareId| -> String { r.given.get(&case).cloned().unwrap_or_default() };
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        // failed first
        let mut cases: Vec<&codetest::CaseResult> = r.cases.iter().collect();
        cases.sort_by_key(|c| c.ok());
        for c in cases {
            let ok = c.ok();
            let body = |ui: &mut egui::Ui, actions: &mut Vec<ReportAction>| {
                let g = given(c.square);
                ui.weak(if g.is_empty() {
                    "it gives nothing: the variables as they start".to_string()
                } else {
                    format!("given {g}")
                });
                for k in &c.checks {
                    let what = if k.label.is_empty() { "a check".to_string() } else { k.label.clone() };
                    let text = if k.ok {
                        egui::RichText::new(format!("{what}: {} as expected", k.got.text())).color(theme.dim)
                    } else {
                        egui::RichText::new(format!("{what}: expected {}, got {}", k.expected.text(), k.got.text()))
                            .color(theme.error)
                    };
                    link(ui, text, k.square, actions);
                }
                if c.checks.is_empty() && c.error.is_none() {
                    ui.weak("it ran, and got to no test/expect");
                }
                if let Some(e) = &c.error {
                    ui.colored_label(theme.error, e);
                }
                if !c.output.is_empty() {
                    ui.collapsing(
                        format!("it printed {} line{}", c.output.len(), if c.output.len() == 1 { "" } else { "s" }),
                        |ui| {
                            for l in &c.output {
                                ui.monospace(l);
                            }
                        },
                    );
                }
            };
            let head = |ui: &mut egui::Ui, actions: &mut Vec<ReportAction>| {
                if ok {
                    ui.colored_label(green, "passed");
                } else {
                    ui.colored_label(theme.error, "failed");
                }
                link(ui, egui::RichText::new(&c.name).strong(), c.square, actions);
                let n = c.checks.len();
                ui.weak(format!("{n} check{}", if n == 1 { "" } else { "s" }));
                if ui
                    .button("Debug")
                    .on_hover_text("Run this test by steps (F10), from what it gives: its checks show on the canvas")
                    .clicked()
                {
                    actions.push(ReportAction::Debug(c.square));
                }
            };
            if ok {
                // passed: one line, open to see
                let id = ui.make_persistent_id(("case", c.square));
                egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false)
                    .show_header(ui, |ui| head(ui, &mut actions))
                    .body(|ui| body(ui, &mut actions));
            } else {
                ui.horizontal(|ui| head(ui, &mut actions));
                ui.indent(("case", c.square), |ui| body(ui, &mut actions));
            }
        }
        // the tests of code squares
        let mut squares: Vec<&codetest::SquareTests> = r.squares.iter().collect();
        squares.sort_by_key(|s| s.results.as_ref().is_ok_and(|rows| rows.iter().all(|r| r.ok())));
        for s in squares {
            ui.horizontal(|ui| {
                let name = if s.name.is_empty() { format!("square {}", s.square) } else { s.name.clone() };
                match &s.results {
                    Ok(rows) => {
                        let p = rows.iter().filter(|r| r.ok()).count();
                        if p == rows.len() {
                            ui.colored_label(green, "passed");
                        } else {
                            ui.colored_label(theme.error, "failed");
                        }
                        link(ui, egui::RichText::new(&name).strong(), s.square, &mut actions);
                        ui.weak(format!("its tests: {p} of {} rows", rows.len()));
                    }
                    Err(_) => {
                        ui.colored_label(theme.error, "failed");
                        link(ui, egui::RichText::new(&name).strong(), s.square, &mut actions);
                    }
                }
                if ui.button("Its tests").on_hover_text("Open the table of its tests").clicked() {
                    ui_state.open = Some(s.square);
                }
            });
            ui.indent(("square", s.square), |ui| match &s.results {
                Ok(rows) => {
                    for (k, row) in rows.iter().enumerate() {
                        for p in &row.problems {
                            ui.colored_label(theme.error, format!("row {}: {p}", k + 1));
                        }
                    }
                }
                Err(e) => {
                    ui.colored_label(theme.error, e);
                }
            });
        }
    });
    actions
}

impl TestsUi {
    /// Keeps up with TAB's canvas: whether the report is of it as it is.
    pub fn sync(&mut self, tab: &Tab) {
        if self.now.0 != tab.generation || self.now == (0, 0) {
            self.now = (tab.generation, content_key(&tab.canvas));
        }
    }

    /// Whether the report is of the canvas as it is now (moving squares
    /// doesn't change that).
    fn fresh(&self) -> bool {
        self.report.as_ref().is_some_and(|r| r.key == self.now.1)
    }

    /// How the last Run tests found test/case SQ, if it ran on the canvas
    /// as it is.
    pub fn case_result(&self, sq: SquareId) -> Option<bool> {
        let r = self.report.as_ref().filter(|_| self.fresh())?;
        r.cases.iter().find(|c| c.square == sq).map(|c| c.ok())
    }
}

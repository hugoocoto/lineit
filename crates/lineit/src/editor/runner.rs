//! Run and Step: the interpreter on a thread of its own, so a long run
//! doesn't freeze the editor. The editor sends it commands and reads what it
//! does from a [`View`], a few times a second.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;
use lineit_core::interp::{Interp, Status, World};
use lineit_core::ir::Program;
use lineit_core::model::{Canvas, SquareId};
use lineit_rt::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmd {
    Run,
    Step,
    Stop,
    /// A program waits in [`Runner::offer`]: look for it.
    Swap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Running,
    /// Stepping, stopped before the squares in `current`.
    Paused,
    /// The program ended.
    Done,
    Stopped,
    Failed,
}

pub struct View {
    pub state: State,
    /// Lines printed since the editor last took them.
    pub output: Vec<String>,
    /// The squares the branches stopped before, while stepping.
    pub current: Vec<SquareId>,
    /// The last value of each output that keeps one, by square and output.
    pub values: HashMap<(SquareId, String), Value>,
    /// What each code square keeps, when the program last paused or ended.
    pub states: HashMap<SquareId, Vec<(String, Value)>>,
    /// The variables of the canvas, and their values.
    pub vars: Vec<(String, Value)>,
    /// How the `test/expect`s it got to went (in a test's run).
    pub checks: Vec<lineit_core::interp::CheckResult>,
    pub error: Option<String>,
    /// How the last edit went, and whether it's an error: applied to the
    /// running program, or why it wasn't.
    pub edit: Option<(String, bool)>,
    /// When an edit was applied: its note fades after a few seconds.
    pub edit_at: Option<Instant>,
    /// An edit waits for the branches that run to finish.
    pub waiting: bool,
    /// The canvas and generation of the last edit that went on to the run:
    /// the editor takes it (see [`Runner::canvas`]).
    pub applied: Option<(Canvas, u64)>,
    pub started: Instant,
    pub ops: u64,
}

pub struct Runner {
    tx: Sender<Cmd>,
    pub view: Arc<Mutex<View>>,
    /// Stops the program right away, even in the middle of a fan-out, when
    /// the thread that runs it isn't reading commands.
    stop: Arc<AtomicBool>,
    /// The edited program that waits for the run to be between handlers.
    pending: Arc<Mutex<Option<Pending>>>,
    /// Makes the branches stop, for an edit that restarts them.
    interrupt: Arc<AtomicBool>,
    /// The canvas the run goes by now (the last edit that was applied), and
    /// the generation of the tab it was.
    pub canvas: Canvas,
    pub generation: u64,
    /// The generation last offered, applied or not.
    pub offered: u64,
}

/// An edit that waits.
struct Pending {
    prog: Program,
    canvas: Canvas,
    generation: u64,
    /// Files the modules are made of changed too.
    files: bool,
    /// The branches that run are dropped, and the entries run again.
    restart: bool,
}

impl Runner {
    pub fn start(
        prog: Program,
        canvas: Canvas,
        generation: u64,
        dir: PathBuf,
        first: Cmd,
        ctx: egui::Context,
    ) -> Runner {
        let (tx, rx) = channel();
        let view = Arc::new(Mutex::new(View {
            state: if first == Cmd::Step { State::Paused } else { State::Running },
            output: Vec::new(),
            current: Vec::new(),
            values: HashMap::new(),
            states: HashMap::new(),
            checks: Vec::new(),
            vars: Vec::new(),
            error: None,
            edit: None,
            edit_at: None,
            waiting: false,
            applied: None,
            started: Instant::now(),
            ops: 0,
        }));
        let v = view.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let stopped = stop.clone();
        let pending = Arc::new(Mutex::new(None));
        let waiting = pending.clone();
        let interrupt = Arc::new(AtomicBool::new(false));
        let interrupting = interrupt.clone();
        std::thread::Builder::new()
            .name("lineit run".into())
            .spawn(move || {
                let r = work(prog, dir, first, flag, interrupting, &rx, &v, &ctx, &waiting);
                let mut view = v.lock().unwrap();
                match r {
                    Ok(state) => view.state = state,
                    // Stop ended a square that was running: that's a stop
                    Err(_) if stopped.load(Ordering::Relaxed) => view.state = State::Stopped,
                    Err(e) => {
                        view.state = State::Failed;
                        view.error = Some(e);
                    }
                }
                view.current.clear();
                ctx.request_repaint();
            })
            .expect("a thread for the run");
        Runner { tx, view, stop, pending, interrupt, canvas, offered: generation, generation }
    }

    /// Gives the run an edited program, to go on with when no branch is in
    /// the middle of a handler (at once, if none is). A newer one replaces
    /// one that still waits. FILES: what the modules are made of changed on
    /// disk too.
    pub fn offer(&mut self, prog: Program, canvas: Canvas, generation: u64, files: bool) {
        let mut p = self.pending.lock().unwrap();
        let restart = p.as_ref().is_some_and(|p| p.restart);
        *p = Some(Pending { prog, canvas, generation, files, restart });
        drop(p);
        self.offered = generation;
        let _ = self.tx.send(Cmd::Swap);
    }

    /// For an edit that waits for branches that don't end (a loop of wires):
    /// they stop where they are, the program changes, and its entries run
    /// again with the state it has.
    pub fn restart(&self) {
        if let Some(p) = self.pending.lock().unwrap().as_mut() {
            p.restart = true;
            self.interrupt.store(true, Ordering::Relaxed);
            let _ = self.tx.send(Cmd::Swap);
        }
    }

    pub fn send(&self, c: Cmd) {
        if c == Cmd::Stop {
            self.stop.store(true, Ordering::Relaxed);
        }
        let _ = self.tx.send(c);
    }

    pub fn state(&self) -> State {
        self.view.lock().unwrap().state
    }

    pub fn is_live(&self) -> bool {
        matches!(self.state(), State::Running | State::Paused)
    }
}

impl Drop for Runner {
    fn drop(&mut self) {
        self.send(Cmd::Stop);
    }
}

/// Gives the view what the program printed and nobody took yet, when
/// dropped.
struct OutputOnDrop(Arc<Mutex<Arc<World>>>, Arc<Mutex<View>>);

impl Drop for OutputOnDrop {
    fn drop(&mut self) {
        let lines = self.0.lock().unwrap().take_output();
        if let Ok(mut v) = self.1.lock() {
            v.output.extend(lines);
        }
    }
}

fn publish(it: &mut Interp, view: &Mutex<View>, state: State, ctx: &egui::Context) {
    let squares: Vec<SquareId> = it.current().into_iter().map(|n| it.program().nodes[n as usize].square).collect();
    let values = it.w.shown();
    // here nothing runs: the code squares can say what they keep
    let states = it.w.code_states();
    let mut v = view.lock().unwrap();
    v.output.extend(it.w.take_output());
    v.current = squares;
    v.values = values;
    v.states = states;
    v.checks = it.w.checks();
    v.vars = it.w.variables();
    v.state = state;
    v.ops = it.ops_run;
    drop(v);
    ctx.request_repaint();
}

fn work(
    prog: Program,
    dir: PathBuf,
    first: Cmd,
    stop: Arc<AtomicBool>,
    interrupt: Arc<AtomicBool>,
    rx: &Receiver<Cmd>,
    view: &Arc<Mutex<View>>,
    ctx: &egui::Context,
    pending: &Mutex<Option<Pending>>,
) -> Result<State, String> {
    let mut it = Interp::new(prog, dir)?;
    let stopping = stop.clone();
    it.set_stop(stop);
    it.set_interrupt(interrupt);
    // the world changes when an edit is applied: the thread that shows what
    // the program does (and stops it) asks here for the one that runs
    let world = it.live();
    // what it printed reaches the view however this ends, an error too
    let _rest = OutputOnDrop(world.clone(), view.clone());
    // while branches run on threads this one waits for them: another one
    // shows what they print and set meanwhile
    let done = Arc::new(AtomicBool::new(false));
    let _done = SetOnDrop(done.clone());
    {
        let (world, view, ctx, done) = (world.clone(), view.clone(), ctx.clone(), done.clone());
        std::thread::spawn(move || {
            let mut killed: Option<Arc<World>> = None;
            while !done.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(100));
                let w = world.lock().unwrap().clone();
                // a square stuck in its code (Python, C...) can't see Stop: its process ends
                // (and the processes of a world an edit made after Stop too)
                if stopping.load(Ordering::Relaxed) && !killed.as_ref().is_some_and(|k| Arc::ptr_eq(k, &w)) {
                    w.kill_externals();
                    killed = Some(w.clone());
                }
                let out = w.take_output();
                let values = w.shown();
                let checks = w.checks();
                let vars = w.variables();
                let mut v = view.lock().unwrap();
                v.output.extend(out);
                v.values = values;
                v.checks = checks;
                v.vars = vars;
                drop(v);
                ctx.request_repaint();
            }
        });
    }
    it.start()?;
    let mut stepping = first == Cmd::Step;
    let mut last = Instant::now();
    if stepping {
        publish(&mut it, view, State::Paused, ctx);
    }
    loop {
        if stepping {
            // wait for the next press
            match rx.recv() {
                Ok(Cmd::Step) => {
                    try_swap(&mut it, pending, view);
                    it.step()?;
                    // the branches may be done now: the edit that waited goes on
                    try_swap(&mut it, pending, view);
                    let st = if it.is_idle() { Status::Idle } else { Status::Paused };
                    if st == Status::Idle && it.next_clock().is_none() {
                        ended(pending, view);
                        publish(&mut it, view, State::Done, ctx);
                        return Ok(State::Done);
                    }
                    publish(&mut it, view, State::Paused, ctx);
                }
                Ok(Cmd::Run) => stepping = false,
                Ok(Cmd::Swap) => {
                    try_swap(&mut it, pending, view);
                    publish(&mut it, view, State::Paused, ctx);
                }
                Ok(Cmd::Stop) | Err(_) => {
                    publish(&mut it, view, State::Stopped, ctx);
                    return Ok(State::Stopped);
                }
            }
            continue;
        }
        match rx.try_recv() {
            Ok(Cmd::Stop) => {
                publish(&mut it, view, State::Stopped, ctx);
                return Ok(State::Stopped);
            }
            Ok(Cmd::Step) => {
                stepping = true;
                publish(&mut it, view, State::Paused, ctx);
                continue;
            }
            _ => {}
        }
        it.run(50_000)?;
        // an edit may have dropped the branches or started new ones
        try_swap(&mut it, pending, view);
        let st = if it.is_idle() { Status::Idle } else { Status::Running };
        if last.elapsed() > Duration::from_millis(50) || st == Status::Idle {
            publish(&mut it, view, State::Running, ctx);
            last = Instant::now();
        }
        if st == Status::Idle {
            let Some(wait) = it.next_clock() else {
                ended(pending, view);
                publish(&mut it, view, State::Done, ctx);
                return Ok(State::Done);
            };
            match rx.recv_timeout(Duration::from_secs_f64(wait)) {
                Ok(Cmd::Stop) | Err(RecvTimeoutError::Disconnected) => {
                    publish(&mut it, view, State::Stopped, ctx);
                    return Ok(State::Stopped);
                }
                Ok(Cmd::Step) => {
                    stepping = true;
                    publish(&mut it, view, State::Paused, ctx);
                }
                Ok(Cmd::Swap) => {
                    try_swap(&mut it, pending, view);
                    publish(&mut it, view, State::Running, ctx);
                }
                Ok(Cmd::Run) | Err(RecvTimeoutError::Timeout) => {}
            }
            it.tick_clocks(false)?;
        }
    }
}

/// Goes on with the program an edit left in PENDING, if no branch is in the
/// middle of a handler; if one is, the view says the edit waits (and the
/// editor may ask to restart them). The world the program has is in
/// [`Interp::live`] after.
fn try_swap(it: &mut Interp, pending: &Mutex<Option<Pending>>, view: &Mutex<View>) {
    let mut p = pending.lock().unwrap();
    let Some(restart) = p.as_ref().map(|p| p.restart) else { return };
    if restart {
        // no branch is on a thread now: this one drives them
        it.abandon();
    } else if !it.can_swap() {
        view.lock().unwrap().waiting = true;
        return;
    }
    let Pending { prog, canvas, generation, files, .. } = p.take().expect("a program waits");
    drop(p);
    let (edit, applied) = match it.swap(prog, files) {
        Ok(r) => ((format!("edit applied: {r}"), false), Some((canvas, generation))),
        Err(e) => ((format!("edit not applied: {e}"), true), None),
    };
    if restart && let Err(e) = it.restart() {
        view.lock().unwrap().error = Some(e);
    }
    let mut v = view.lock().unwrap();
    v.edit_at = applied.is_some().then(Instant::now);
    v.edit = Some(edit);
    v.applied = applied;
    v.waiting = false;
}

/// The program ended with an edit still waiting: it says so.
fn ended(pending: &Mutex<Option<Pending>>, view: &Mutex<View>) {
    if pending.lock().unwrap().take().is_some() {
        let mut v = view.lock().unwrap();
        v.waiting = false;
        v.edit = Some(("edit not applied: the program ended first".into(), true));
    }
}

/// Sets its flag when it goes, however the run ends.
struct SetOnDrop(Arc<AtomicBool>);

impl Drop for SetOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lineit_core::registry::Registry;

    /// An entry that prints A and then B.
    fn prints(b: &str) -> (Canvas, Program) {
        let src = format!(
            r#"return {{ lineit = 1, squares = {{
            {{ id = 1, module = "flow/entry" }},
            {{ id = 2, module = "io/print", fields = {{ value = "a" }} }},
            {{ id = 3, module = "io/print", fields = {{ value = "{b}" }} }},
            }}, links = {{ {{ 1, "then", 2, "tick" }}, {{ 2, "then", 3, "tick" }} }} }}"#
        );
        let c = lineit_core::file::read_canvas(&src, "prints").unwrap();
        let p = lineit_core::lower::lower(&c, &Registry::builtin()).program.unwrap();
        (c, p)
    }

    fn wait(what: &str, mut f: impl FnMut() -> bool) {
        for _ in 0..200 {
            if f() {
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        panic!("{what}");
    }

    /// An edit offered while stepping, with the branches stopped before a
    /// square, waits, and goes on by itself when the branches are done: no
    /// other command is needed, and the run doesn't end with it waiting.
    #[test]
    fn a_step_waits_for_the_branches_and_applies_the_edit_by_itself() {
        let (c, p) = prints("b");
        let mut r = Runner::start(p, c, 0, std::env::temp_dir(), Cmd::Step, egui::Context::default());
        wait("paused before the first square", || r.view.lock().unwrap().current.len() == 1);
        let (c2, p2) = prints("c");
        r.offer(p2, c2.clone(), 1, false);
        wait("it says the edit waits", || r.view.lock().unwrap().waiting);
        assert!(r.view.lock().unwrap().applied.is_none());
        for _ in 0..3 {
            r.send(Cmd::Step);
            std::thread::sleep(Duration::from_millis(50));
        }
        wait("the edit applied", || {
            let v = r.view.lock().unwrap();
            !v.waiting && v.edit.as_ref().is_some_and(|(m, err)| !err && m.starts_with("edit applied"))
        });
        let applied = r.view.lock().unwrap().applied.clone();
        assert_eq!(applied, Some((c2, 1)));
    }

    /// A loop of wires never goes idle: the edit waits, and "apply now"
    /// drops the branches and goes on with it.
    #[test]
    fn restart_gives_a_loop_the_edit() {
        let src = |inc: &str| {
            let src = format!(
                r#"return {{ lineit = 1, squares = {{
                {{ id = 1, module = "flow/entry" }},
                {{ id = 2, module = "var/var", params = {{ name = "n", type = "int", initial = "0" }} }},
                {{ id = 3, module = "var/get", params = {{ name = "n" }} }},
                {{ id = 4, module = "math/add", fields = {{ b = "{inc}" }} }},
                {{ id = 5, module = "var/set", params = {{ name = "n" }} }},
                }}, links = {{ {{ 1, "then", 5, "tick" }}, {{ 5, "then", 5, "tick" }},
                {{ 3, "value", 4, "a" }}, {{ 4, "result", 5, "value" }} }} }}"#
            );
            let c = lineit_core::file::read_canvas(&src, "loop").unwrap();
            let p = lineit_core::lower::lower(&c, &Registry::builtin()).program.unwrap();
            (c, p)
        };
        let (c, p) = src("1");
        let mut r = Runner::start(p, c, 0, std::env::temp_dir(), Cmd::Run, egui::Context::default());
        let n = |r: &Runner| r.view.lock().unwrap().vars.iter().find(|(k, _)| k == "n").map(|(_, v)| v.int());
        wait("it counts", || n(&r).is_some_and(|n| n > 0));
        let (c2, p2) = src("1000");
        r.offer(p2, c2, 1, false);
        wait("it waits", || r.view.lock().unwrap().waiting);
        r.restart();
        wait("the edit applied", || {
            r.view.lock().unwrap().edit.as_ref().is_some_and(|(m, _)| m.starts_with("edit applied"))
        });
        let at = n(&r).unwrap();
        wait("it counts by 1000", || n(&r).is_some_and(|n| n > at + 1000));
        assert_eq!((n(&r).unwrap() - at) % 1000, 0);
        r.send(Cmd::Stop);
    }
}

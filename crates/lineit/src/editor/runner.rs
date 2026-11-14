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
use lineit_core::interp::{Interp, Status};
use lineit_core::ir::Program;
use lineit_core::model::SquareId;
use lineit_rt::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmd {
    Run,
    Step,
    Stop,
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
    pub started: Instant,
    pub ops: u64,
}

pub struct Runner {
    tx: Sender<Cmd>,
    pub view: Arc<Mutex<View>>,
    /// Stops the program right away, even in the middle of a fan-out, when
    /// the thread that runs it isn't reading commands.
    stop: Arc<AtomicBool>,
}

impl Runner {
    pub fn start(prog: Program, dir: PathBuf, first: Cmd, ctx: egui::Context) -> Runner {
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
            started: Instant::now(),
            ops: 0,
        }));
        let v = view.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let stopped = stop.clone();
        std::thread::Builder::new()
            .name("lineit run".into())
            .spawn(move || {
                let r = work(prog, dir, first, flag, &rx, &v, &ctx);
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
        Runner { tx, view, stop }
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
struct OutputOnDrop(Arc<lineit_core::interp::World>, Arc<Mutex<View>>);

impl Drop for OutputOnDrop {
    fn drop(&mut self) {
        let lines = self.0.take_output();
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
    rx: &Receiver<Cmd>,
    view: &Arc<Mutex<View>>,
    ctx: &egui::Context,
) -> Result<State, String> {
    let mut it = Interp::new(prog, dir)?;
    // what it printed reaches the view however this ends, an error too
    let _rest = OutputOnDrop(it.w.clone(), view.clone());
    let stopping = stop.clone();
    it.set_stop(stop);
    // while branches run on threads this one waits for them: another one
    // shows what they print and set meanwhile
    let done = Arc::new(AtomicBool::new(false));
    let _done = SetOnDrop(done.clone());
    {
        let (w, view, ctx, done) = (it.w.clone(), view.clone(), ctx.clone(), done.clone());
        std::thread::spawn(move || {
            let mut killed = false;
            while !done.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(100));
                // a square stuck in its code (Python, C...) can't see Stop: its process ends
                if !killed && stopping.load(Ordering::Relaxed) {
                    w.kill_externals();
                    killed = true;
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
                    let st = it.step()?;
                    if st == Status::Idle && it.next_clock().is_none() {
                        publish(&mut it, view, State::Done, ctx);
                        return Ok(State::Done);
                    }
                    publish(&mut it, view, State::Paused, ctx);
                }
                Ok(Cmd::Run) => stepping = false,
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
        let st = it.run(50_000)?;
        if last.elapsed() > Duration::from_millis(50) || st == Status::Idle {
            publish(&mut it, view, State::Running, ctx);
            last = Instant::now();
        }
        if st == Status::Idle {
            let Some(wait) = it.next_clock() else {
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
                Ok(Cmd::Run) | Err(RecvTimeoutError::Timeout) => {}
            }
            it.tick_clocks(false)?;
        }
    }
}

/// Sets its flag when it goes, however the run ends.
struct SetOnDrop(Arc<AtomicBool>);

impl Drop for SetOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

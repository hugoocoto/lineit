//! Run and Step: the interpreter on a thread of its own, so a long run
//! doesn't freeze the editor. The editor sends it commands and reads what it
//! does from a [`View`], a few times a second.

use std::collections::HashMap;
use std::path::PathBuf;
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
    pub error: Option<String>,
    pub started: Instant,
    pub ops: u64,
}

pub struct Runner {
    tx: Sender<Cmd>,
    pub view: Arc<Mutex<View>>,
}

impl Runner {
    pub fn start(prog: Program, dir: PathBuf, first: Cmd, ctx: egui::Context) -> Runner {
        let (tx, rx) = channel();
        let view = Arc::new(Mutex::new(View {
            state: if first == Cmd::Step { State::Paused } else { State::Running },
            output: Vec::new(),
            current: Vec::new(),
            values: HashMap::new(),
            error: None,
            started: Instant::now(),
            ops: 0,
        }));
        let v = view.clone();
        std::thread::Builder::new()
            .name("lineit run".into())
            .spawn(move || {
                let r = work(prog, dir, first, &rx, &v, &ctx);
                let mut view = v.lock().unwrap();
                match r {
                    Ok(state) => view.state = state,
                    Err(e) => {
                        view.state = State::Failed;
                        view.error = Some(e);
                    }
                }
                view.current.clear();
                ctx.request_repaint();
            })
            .expect("a thread for the run");
        Runner { tx, view }
    }

    pub fn send(&self, c: Cmd) {
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
        let _ = self.tx.send(Cmd::Stop);
    }
}

fn publish(it: &mut Interp, view: &Mutex<View>, state: State, ctx: &egui::Context) {
    let squares: Vec<SquareId> = it.current().into_iter().map(|n| it.program().nodes[n as usize].square).collect();
    let mut values = HashMap::new();
    for (i, s) in it.program().slots.iter().enumerate() {
        if let Some((sq, pin)) = &s.shows {
            values.insert((*sq, pin.clone()), it.w.slots[i].clone());
        }
    }
    let mut v = view.lock().unwrap();
    v.output.append(&mut it.w.output);
    v.current = squares;
    v.values = values;
    v.state = state;
    v.ops = it.ops_run;
    drop(v);
    ctx.request_repaint();
}

fn work(
    prog: Program,
    dir: PathBuf,
    first: Cmd,
    rx: &Receiver<Cmd>,
    view: &Mutex<View>,
    ctx: &egui::Context,
) -> Result<State, String> {
    let mut it = Interp::new(prog, dir)?;
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

//! Runs the IR: Run and Step in the editor, and `lineit run`.
//!
//! Each handler is first flattened into ops with jumps, so a branch is just
//! a stack of frames, each an op index and its locals.
//!
//! Run runs branches on threads, in parallel, as compiled programs do: the
//! program's state ([`World`]) has a lock for each slot and each Lua or Rust
//! module, so two branches only wait for each other where they use the same
//! one. Step runs them as tasks that take turns on one thread, so it can
//! stop every one of them before its next square, all together.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use lineit_rt::Ext;
use lineit_rt::ext::{Handles, Ticked};
use lineit_rt::lua::LuaModule;
use lineit_rt::{Record, Type, Value, image as img, ops, rand, text};

use crate::ir::*;
use crate::model::SquareId;

enum Op {
    Enter(NodeId),
    Set(SlotId, Expr),
    Let(LocalId, Expr),
    Print(Expr),
    Do(Effect, Vec<Expr>),
    Jump(usize),
    JumpIfNot(Expr, usize),
    /// local = from; end = to
    ForInit {
        local: LocalId,
        end: LocalId,
        from: Expr,
        to: Expr,
    },
    /// if !(local < end) jump exit
    ForCheck {
        local: LocalId,
        end: LocalId,
        exit: usize,
    },
    /// local += 1; jump back
    ForNext {
        local: LocalId,
        back: usize,
    },
    Fire(Fire),
    Stop,
    Ext {
        instance: usize,
        input: Option<String>,
        inputs: Vec<(String, Expr)>,
        outputs: Vec<(String, SlotId)>,
        fires: Vec<(String, Fire)>,
    },
    Check {
        square: SquareId,
        label: String,
        value: Expr,
        expected: Expr,
    },
    Return,
}

/// How a `test/expect` went, when the run got to it.
#[derive(Clone, Debug, PartialEq)]
pub struct CheckResult {
    pub square: SquareId,
    pub label: String,
    pub got: Value,
    pub expected: Value,
    pub ok: bool,
}

struct Code {
    ops: Vec<Op>,
    locals: Vec<Type>,
}

fn compile(h: &Handler) -> Code {
    let mut c = Code { ops: Vec::new(), locals: h.locals.clone() };
    block(&mut c, &h.body);
    c.ops.push(Op::Return);
    c
}

fn block(c: &mut Code, b: &Block) {
    for s in b {
        match s {
            Stmt::Enter(n) => c.ops.push(Op::Enter(*n)),
            Stmt::Set(s, e) => c.ops.push(Op::Set(*s, e.clone())),
            Stmt::Let(l, e) => c.ops.push(Op::Let(*l, e.clone())),
            Stmt::Print(e) => c.ops.push(Op::Print(e.clone())),
            Stmt::Do(f, a) => c.ops.push(Op::Do(f.clone(), a.clone())),
            Stmt::Fire(f) => c.ops.push(Op::Fire(f.clone())),
            Stmt::Stop => c.ops.push(Op::Stop),
            Stmt::Check { square, label, value, expected } => c.ops.push(Op::Check {
                square: *square,
                label: label.clone(),
                value: value.clone(),
                expected: expected.clone(),
            }),
            Stmt::Ext { instance, input, inputs, outputs, fires } => c.ops.push(Op::Ext {
                instance: *instance,
                input: input.clone(),
                inputs: inputs.clone(),
                outputs: outputs.clone(),
                fires: fires.clone(),
            }),
            Stmt::If(cond, then, els) => {
                let j = c.ops.len();
                c.ops.push(Op::JumpIfNot(cond.clone(), 0));
                block(c, then);
                let k = c.ops.len();
                c.ops.push(Op::Jump(0));
                let else_at = c.ops.len();
                block(c, els);
                let end = c.ops.len();
                c.ops[j] = Op::JumpIfNot(cond.clone(), else_at);
                c.ops[k] = Op::Jump(end);
            }
            Stmt::While(cond, body) => {
                let top = c.ops.len();
                c.ops.push(Op::JumpIfNot(cond.clone(), 0));
                block(c, body);
                c.ops.push(Op::Jump(top));
                let end = c.ops.len();
                c.ops[top] = Op::JumpIfNot(cond.clone(), end);
            }
            Stmt::For { local, from, to, body } => {
                c.locals.push(Type::Int);
                let end = (c.locals.len() - 1) as LocalId;
                c.ops.push(Op::ForInit { local: *local, end, from: from.clone(), to: to.clone() });
                let check = c.ops.len();
                c.ops.push(Op::ForCheck { local: *local, end, exit: 0 });
                block(c, body);
                c.ops.push(Op::ForNext { local: *local, back: check });
                let exit = c.ops.len();
                c.ops[check] = Op::ForCheck { local: *local, end, exit };
            }
        }
    }
}

struct Frame {
    handler: HandlerId,
    pc: usize,
    locals: Vec<Value>,
    /// Fires still to do, the next one last, with the slots that get their
    /// values just before: a Lua module fires several outputs, one after
    /// the other.
    pending: Vec<(Vec<(SlotId, Value)>, Fire)>,
}

/// The meets of one fan-out, and which ones were reached (by branches on
/// any thread).
struct Meets {
    ids: Vec<HandlerId>,
    arrived: Vec<AtomicBool>,
}

impl Meets {
    fn new(ids: &[HandlerId]) -> Arc<Meets> {
        Arc::new(Meets { ids: ids.to_vec(), arrived: ids.iter().map(|_| AtomicBool::new(false)).collect() })
    }

    /// The meets reached, last first: pushed as frames in this order, the
    /// first runs first.
    fn reached_last_first(&self) -> Vec<HandlerId> {
        self.ids.iter().zip(&self.arrived).rev().filter(|(_, a)| a.load(Ordering::Relaxed)).map(|(h, _)| *h).collect()
    }
}

/// A cache line of its own: slots that different branches change, next to
/// each other, would make their threads wait for each other's caches.
#[repr(align(128))]
pub struct Padded<T>(pub T);

impl<T> std::ops::Deref for Padded<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

/// A lock that a branch that panicked while holding it doesn't poison: the
/// program goes on with what's there.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

struct Task {
    frames: Vec<Frame>,
    /// The fan-outs it's inside of, innermost last.
    inside: Vec<Arc<Meets>>,
    /// Waiting for this many branches, of the fan-out with these meets.
    waiting: Option<(usize, Arc<Meets>)>,
    parent: Option<usize>,
    done: bool,
    /// It ran a square in this step.
    entered: bool,
    /// The square it ran in this step (Step only: the trace asks).
    ran: Option<NodeId>,
}

/// A branch, as the trace sees it (Step only).
pub(crate) struct TaskView {
    /// The branch that fanned it out.
    pub parent: Option<usize>,
    /// Waiting for the branches it fanned out.
    pub waiting: bool,
    /// The square it ran in this step, if it did.
    pub ran: Option<NodeId>,
    /// The squares whose handlers it's in, the outermost first: a loop's,
    /// under the squares of its body.
    pub inside: Vec<NodeId>,
}

/// When [`Interp::step_with`] calls: before a branch's turn, or after.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Turn {
    Before,
    After,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Status {
    /// Some branch can still run.
    Running,
    /// Every branch is done; clocks may start more.
    Idle,
    /// Stopped before the next square of each branch (Step).
    Paused,
}

/// The program's state: what its ops read and change, shared by the
/// branches running on threads. A lock each, so they only wait for each
/// other on the same slot or module; two branches that read and change one
/// at once can lose a change, as the language says.
pub struct World {
    prog: Arc<Program>,
    pub slots: Vec<Padded<Mutex<Value>>>,
    exts: Vec<Mutex<Box<dyn Ext>>>,
    /// Per instance in the program: its module and its instance in it.
    ext_instances: Vec<(usize, usize)>,
    /// What it printed, for the caller to take.
    pub output: Mutex<Vec<String>>,
    /// How the `test/expect`s it got to went, in order.
    checks: Mutex<Vec<CheckResult>>,
    started: Mutex<Instant>,
    /// Relative paths are from here.
    pub base_dir: PathBuf,
    /// Set to end the program: every branch, on every thread, stops at its
    /// next op. Stop sets it from another thread; `flow/stop` from inside.
    pub stop: Arc<AtomicBool>,
    /// Set when the program itself ends (`flow/stop`), which `stop` also
    /// says when something else sets it: the trace tells them apart.
    pub(crate) ended: AtomicBool,
    /// Set to make every branch stop at its next op without ending the
    /// program: the edit that restarts the branches ([`Interp::abandon`]).
    pub interrupt: Arc<AtomicBool>,
    /// Per slot: whether a statement reads and changes it, so those that
    /// change it hold its lock ([`Program::read_and_changed`]).
    locked: Vec<bool>,
    any_locked: bool,
    slot_locks: Vec<Mutex<()>>,
    /// What ends the processes of extensions now, and what they write to
    /// stderr (see [`lineit_rt::ext::Handles`]).
    handles: Vec<Mutex<Handles>>,
}

/// What stands where a module that moved to another [`World`] was.
struct Gone;

impl Ext for Gone {
    fn instance(&mut self, _: &[(&str, Value)]) -> Result<usize, String> {
        Err("the module is in another program".into())
    }
    fn eval(&mut self, _: usize, _: &[(&str, Value)], _: &[(&str, &Type)]) -> Result<Vec<Value>, String> {
        Err("the module is in another program".into())
    }
    fn tick(&mut self, _: usize, _: Option<&str>, _: &[(&str, Value)], _: &[(&str, &Type)]) -> Result<Ticked, String> {
        Err("the module is in another program".into())
    }
}

impl World {
    /// A world for PROG, with its slots at their initial values and the
    /// modules EXTS (opened, with their HANDLES) and INSTANCES of them.
    fn build(
        prog: Arc<Program>,
        exts: Vec<Box<dyn Ext>>,
        handles: Vec<Handles>,
        ext_instances: Vec<(usize, usize)>,
        base_dir: PathBuf,
        stop: Arc<AtomicBool>,
        interrupt: Arc<AtomicBool>,
    ) -> World {
        let locked = prog.read_and_changed();
        World {
            handles: handles.into_iter().map(Mutex::new).collect(),
            any_locked: locked.contains(&true),
            locked,
            slot_locks: prog.slots.iter().map(|_| Mutex::new(())).collect(),
            slots: prog.slots.iter().map(|s| Padded(Mutex::new(s.init.clone()))).collect(),
            prog,
            exts: exts.into_iter().map(Mutex::new).collect(),
            ext_instances,
            output: Mutex::new(Vec::new()),
            checks: Mutex::new(Vec::new()),
            started: Mutex::new(Instant::now()),
            base_dir,
            ended: AtomicBool::new(false),
            stop,
            interrupt,
        }
    }

    /// Ends the processes of extensions (code in Python, C...): one stuck in
    /// a call ends it, with an error. For Stop.
    pub fn kill_externals(&self) {
        for h in &self.handles {
            if let Some(k) = &lock(h).kill {
                k();
            }
        }
    }

    /// The variables of the canvas, by name, and their values now.
    pub fn variables(&self) -> Vec<(String, Value)> {
        self.prog.vars.iter().map(|(n, s)| (n.clone(), self.slot(*s as usize))).collect()
    }

    /// How the `test/expect`s the run got to went, so far.
    pub fn checks(&self) -> Vec<CheckResult> {
        lock(&self.checks).clone()
    }

    /// What the extensions printed, to the output.
    fn printed(&self) {
        let lines = lineit_rt::ext::take_printed();
        if !lines.is_empty() {
            lock(&self.output).extend(lines);
        }
    }

    /// Holds the locks of the SLOTS (that an op changes) that need one, in
    /// order, until what it returns is dropped: so the op is one step for
    /// the other branches.
    fn hold(&self, slots: &[SlotId]) -> Vec<std::sync::MutexGuard<'_, ()>> {
        if !self.any_locked {
            return Vec::new();
        }
        self.prog.locks_of(slots, &self.locked).into_iter().map(|s| lock(&self.slot_locks[s as usize])).collect()
    }

    pub fn slot(&self, s: usize) -> Value {
        lock(&self.slots[s]).clone()
    }

    /// What it printed since the last time.
    /// What it printed since this was last called, and what the processes
    /// of its extensions wrote to stderr (if that's kept).
    pub fn take_output(&self) -> Vec<String> {
        let mut out = std::mem::take(&mut *lock(&self.output));
        for h in &self.handles {
            if let Some(e) = &lock(h).stderr {
                out.append(&mut lock(e));
            }
        }
        out
    }

    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    fn interrupted(&self) -> bool {
        self.interrupt.load(Ordering::Relaxed)
    }

    /// The value of each output that shows on a square of the canvas, by
    /// square and output.
    /// What the code squares keep (their state), by square: ask it when
    /// nothing runs (it waits for a square that's running).
    pub fn code_states(&self) -> std::collections::HashMap<crate::model::SquareId, Vec<(String, Value)>> {
        let mut m = std::collections::HashMap::new();
        for (k, i) in self.prog.ext_instances.iter().enumerate() {
            let (module, inst) = self.ext_instances[k];
            if let Ok(st) = lock(&self.exts[module]).state(inst)
                && !st.is_empty()
            {
                m.insert(i.square, st);
            }
        }
        m
    }

    pub fn shown(&self) -> std::collections::HashMap<(crate::model::SquareId, String), Value> {
        let mut m = std::collections::HashMap::new();
        for (i, s) in self.prog.slots.iter().enumerate() {
            if let Some((sq, pin)) = &s.shows {
                m.insert((*sq, pin.clone()), self.slot(i));
            }
        }
        m
    }
}

pub struct Interp {
    prog: Arc<Program>,
    code: Arc<Vec<Code>>,
    tasks: Vec<Task>,
    pub w: Arc<World>,
    /// When each clock ticks next, in seconds since the start.
    clock_next: Vec<f64>,
    /// How many ops ran, all in all.
    pub ops_run: u64,
    /// The world that runs now, for another thread to look at (see
    /// [`Interp::live`]).
    live: Option<Arc<Mutex<Arc<World>>>>,
}

/// What [`Interp::swap`] did with the state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Swapped {
    /// Values that carried over.
    pub kept: usize,
    /// Those whose type changed and were converted (as `value/convert` does), by name.
    pub converted: Vec<String>,
    /// Those whose type changed with no conversion, which start again, by name.
    pub reset: Vec<String>,
    /// Those of variables and squares that are gone.
    pub dropped: usize,
    /// Entries of the edit that weren't there, which run now.
    pub started: usize,
}

impl std::fmt::Display for Swapped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "kept {} value{}", self.kept, if self.kept == 1 { "" } else { "s" })?;
        if !self.converted.is_empty() {
            write!(f, ", converted (the type changed): {}", self.converted.join(", "))?;
        }
        if !self.reset.is_empty() {
            write!(f, ", started again (the type changed): {}", self.reset.join(", "))?;
        }
        if self.dropped > 0 {
            write!(f, ", dropped {}", self.dropped)?;
        }
        if self.started > 0 {
            write!(f, ", ran {} new entr{}", self.started, if self.started == 1 { "y" } else { "ies" })?;
        }
        Ok(())
    }
}

/// What tells a slot of one program from another in the next: where it
/// shows on a square of the canvas, the variable's name, or where it is.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum SlotKey {
    Shown(SquareId, String),
    Var(String),
    /// Where it is (see [`crate::ir::SlotDef::home`]): what tells those
    /// inside subgraphs and those private to a square.
    Home(Vec<SquareId>, String),
}

type Keyed = (SlotKey, usize);

fn slot_name(k: &Keyed) -> String {
    match &k.0 {
        SlotKey::Shown(sq, pin) => format!("{pin} of square {sq}"),
        SlotKey::Var(n) => n.clone(),
        SlotKey::Home(path, what) => {
            let what = what.split_once(' ').map_or(what.as_str(), |(_, w)| w);
            let path: Vec<String> = path.iter().map(|sq| sq.to_string()).collect();
            format!("{what} of square {}", path.join("/"))
        }
    }
}

/// The keys of each slot (it can be a variable and show on a square, and
/// every one has a home), each with how many before it have the same.
fn slot_keys(p: &Program) -> Vec<Vec<Keyed>> {
    let mut keys: Vec<Vec<SlotKey>> = p
        .slots
        .iter()
        .map(|s| s.shows.as_ref().map(|(sq, pin)| SlotKey::Shown(*sq, pin.clone())).into_iter().collect())
        .collect();
    for (name, s) in &p.vars {
        keys[*s as usize].push(SlotKey::Var(name.clone()));
    }
    for (k, s) in keys.iter_mut().zip(&p.slots) {
        k.push(SlotKey::Home(s.home.0.clone(), s.home.1.clone()));
    }
    let mut seen: HashMap<SlotKey, usize> = HashMap::new();
    keys.into_iter()
        .map(|ks| {
            ks.into_iter()
                .map(|k| {
                    let n = seen.entry(k.clone()).or_default();
                    *n += 1;
                    (k, *n - 1)
                })
                .collect()
        })
        .collect()
}

/// Whether a module has to end before another is opened, as it can hold
/// what the new one needs (the lock of a C# build, the library a Rust
/// module loaded), and has to open again if the files changed.
fn closes_first(e: &ExtDef) -> bool {
    matches!(e.kind, ExtKind::Native { .. } | ExtKind::Process { lock: Some(_), .. })
}

/// The handlers of the entries NEW has that OLD hadn't (by square and
/// module).
fn new_entries(old: &Program, new: &Program) -> Vec<HandlerId> {
    let key = |p: &Program, h: HandlerId| {
        let n = &p.nodes[p.handlers[h as usize].node as usize];
        (n.square, n.module.clone())
    };
    let mut had: Vec<_> = old.starts.targets.iter().map(|h| key(old, *h)).collect();
    let mut fresh = Vec::new();
    for h in &new.starts.targets {
        match had.iter().position(|k| *k == key(new, *h)) {
            Some(i) => {
                had.swap_remove(i);
            }
            None => fresh.push(*h),
        }
    }
    fresh
}

const MAX_FRAMES: usize = 1 << 20;

/// Why a task stopped running ops.
enum Next {
    /// Out of ops (or stopped before a square, stepping).
    Budget,
    /// No frames left.
    Done,
    /// It fans out: the slow path.
    FanOut(Fire),
    Stop,
    /// The edit that restarts the branches wants them to stop.
    Interrupt,
}

fn new_frame(code: &[Code], h: HandlerId) -> Frame {
    let locals = code[h as usize].locals.iter().map(Value::default_of).collect();
    Frame { handler: h, pc: 0, locals, pending: Vec::new() }
}

/// A call from TASK to H: a frame for it, unless it's a meet the task
/// stops at. TAIL: the caller has nothing left, so its frame goes.
fn call(task: &mut Task, code: &[Code], h: HandlerId, tail: bool) -> Result<(), String> {
    for m in task.inside.iter().rev() {
        if let Ok(i) = m.ids.binary_search(&h) {
            m.arrived[i].store(true, Ordering::Relaxed);
            return Ok(());
        }
    }
    if tail {
        task.frames.pop();
    }
    if task.frames.len() >= MAX_FRAMES {
        return Err("the ticks go too deep: a loop of squares that never ends?".into());
    }
    task.frames.push(new_frame(code, h));
    Ok(())
}

/// The ops of one task, until it's out of LEFT, done, or fans out.
fn run_ops(task: &mut Task, w: &World, code: &[Code], left: &mut u64, stepping: bool) -> Result<Next, String> {
    while *left > 0 {
        *left -= 1;
        if (*left).is_multiple_of(1024) {
            if w.stopped() {
                return Ok(Next::Stop);
            }
            if w.interrupted() {
                return Ok(Next::Interrupt);
            }
        }
        let Some(frame) = task.frames.last_mut() else { return Ok(Next::Done) };
        if let Some((writes, f)) = frame.pending.pop() {
            {
                let slots: Vec<SlotId> = writes.iter().map(|(s, _)| *s).collect();
                let _held = w.hold(&slots);
                for (s, v) in writes {
                    *lock(&w.slots[s as usize]) = v;
                }
            }
            match f.targets.len() {
                0 => {}
                1 => call(task, code, f.targets[0], false)?,
                _ => return Ok(Next::FanOut(f)),
            }
            continue;
        }
        let h = frame.handler as usize;
        let pc = frame.pc;
        frame.pc += 1;
        match &code[h].ops[pc] {
            Op::Enter(n) => {
                if stepping {
                    if task.entered {
                        frame.pc -= 1;
                        return Ok(Next::Budget);
                    }
                    task.entered = true;
                    task.ran = Some(*n);
                }
            }
            Op::Set(s, e) => {
                let _held = w.hold(&[*s]);
                let v = match e {
                    Expr::Local(l) => frame.locals[*l as usize].clone(),
                    _ => w.eval(e, &frame.locals)?,
                };
                *lock(&w.slots[*s as usize]) = v;
            }
            Op::Let(l, e) => {
                let v = w.eval(e, &frame.locals)?;
                frame.locals[*l as usize] = v;
            }
            Op::Print(e) => {
                let v = w.eval(e, &frame.locals)?;
                lock(&w.output).push(v.str().to_string());
            }
            Op::Do(effect, args) => {
                let _held = w.hold(&effect.writes());
                let args = args.iter().map(|a| w.eval(a, &frame.locals)).collect::<Result<Vec<_>, _>>()?;
                w.effect(effect, args);
            }
            Op::Jump(to) => frame.pc = *to,
            Op::JumpIfNot(e, to) => {
                if !w.eval(e, &frame.locals)?.bool() {
                    frame.pc = *to;
                }
            }
            Op::ForInit { local, end, from, to } => {
                let (a, b) = (w.eval(from, &frame.locals)?, w.eval(to, &frame.locals)?);
                frame.locals[*local as usize] = a;
                frame.locals[*end as usize] = b;
            }
            Op::ForCheck { local, end, exit } => {
                if frame.locals[*local as usize].int() >= frame.locals[*end as usize].int() {
                    frame.pc = *exit;
                }
            }
            Op::ForNext { local, back } => {
                let i = frame.locals[*local as usize].int();
                frame.locals[*local as usize] = Value::Int(i + 1);
                frame.pc = *back;
            }
            Op::Fire(fire) => match fire.targets.len() {
                0 => {}
                1 => {
                    let tail = is_tail(&code[h].ops, pc + 1);
                    call(task, code, fire.targets[0], tail)?;
                }
                _ => return Ok(Next::FanOut(fire.clone())),
            },
            Op::Stop => {
                w.ended.store(true, Ordering::Relaxed);
                return Ok(Next::Stop);
            }
            Op::Check { square, label, value, expected } => {
                let got = w.eval(value, &frame.locals)?;
                let expected = w.eval(expected, &frame.locals)?;
                let ok = lineit_rt::value::same(&got, &expected);
                lock(&w.checks).push(CheckResult { square: *square, label: label.clone(), got, expected, ok });
            }
            Op::Ext { instance, input, inputs, outputs, fires } => {
                let slots: Vec<SlotId> = outputs.iter().map(|(_, s)| *s).collect();
                let _held = w.hold(&slots);
                let args = inputs.iter().map(|(n, e)| Ok((n.as_str(), w.eval(e, &frame.locals)?))).collect::<Result<
                    Vec<_>,
                    String,
                >>(
                )?;
                let (m, inst) = w.ext_instances[*instance];
                let tys: Vec<Type> = outputs.iter().map(|(_, s)| w.prog.slots[*s as usize].ty.clone()).collect();
                let outs: Vec<(&str, &Type)> = outputs.iter().zip(&tys).map(|((n, _), t)| (n.as_str(), t)).collect();
                let run = lock(&w.exts[m]).run(inst, input.as_deref(), &args, &outs);
                w.printed();
                let run = run?;
                for ((_, s), v) in outputs.iter().zip(run.set) {
                    if let Some(v) = v {
                        *lock(&w.slots[*s as usize]) = v;
                    }
                }
                let mut list = Vec::new();
                for (name, values) in run.fire {
                    let f = fires.iter().find(|(n, _)| *n == name).ok_or_else(|| {
                        format!("{}: it fires `{name}`, which isn't one of its tick outputs", w.prog.exts[m].path)
                    })?;
                    let writes = values.into_iter().filter_map(|(o, v)| Some((outputs.get(o)?.1, v))).collect();
                    list.push((writes, f.1.clone()));
                }
                // the first to fire is the last, and goes first
                list.reverse();
                frame.pending = list;
            }
            Op::Return => {
                task.frames.pop();
                if task.frames.is_empty() {
                    return Ok(Next::Done);
                }
            }
        }
    }
    Ok(Next::Budget)
}

/// Runs TASK on this thread until it's done, its fan-outs on threads of
/// their own. Stops early if the program is stopped.
fn run_branch(task: &mut Task, w: &World, code: &[Code]) -> Result<(), String> {
    loop {
        let mut left = 1 << 20;
        match run_ops(task, w, code, &mut left, false)? {
            Next::Budget => {}
            Next::Done => return Ok(()),
            Next::FanOut(f) => {
                fan_out(task, &f, w, code)?;
                if w.interrupted() {
                    task.frames.clear();
                    return Ok(());
                }
            }
            Next::Interrupt => {
                task.frames.clear();
                return Ok(());
            }
            Next::Stop => {
                w.stop.store(true, Ordering::Relaxed);
                task.frames.clear();
                return Ok(());
            }
        }
    }
}

/// The branches of FIRE, from TASK: each on a thread (the first on this
/// one), all to the end. Then the meets they reached go on TASK, to run one
/// after the other.
fn fan_out(task: &mut Task, fire: &Fire, w: &World, code: &[Code]) -> Result<(), String> {
    let meets = Meets::new(&fire.meets);
    let mut inside = task.inside.clone();
    inside.push(meets.clone());
    let branch = |h: HandlerId, inside: Vec<Arc<Meets>>| -> Result<(), String> {
        let mut t =
            Task { frames: Vec::new(), inside, waiting: None, parent: None, done: false, entered: false, ran: None };
        call(&mut t, code, h, false)?;
        run_branch(&mut t, w, code)
    };
    let results: Vec<Result<(), String>> = std::thread::scope(|scope| {
        let others: Vec<_> = fire.targets[1..]
            .iter()
            .map(|&h| {
                let inside = inside.clone();
                std::thread::Builder::new()
                    .name("lineit branch".into())
                    .stack_size(64 << 20)
                    .spawn_scoped(scope, move || branch(h, inside))
                    .expect("a thread for a branch")
            })
            .collect();
        let mut results = vec![branch(fire.targets[0], inside.clone())];
        for t in others {
            results.push(t.join().unwrap_or_else(|_| Err("a branch panicked".into())));
        }
        results
    });
    for r in results {
        r?;
    }
    for h in meets.reached_last_first() {
        call(task, code, h, false)?;
    }
    Ok(())
}

/// The module of KIND (a Lua module, a code square's function, a Rust
/// module...), ready to make squares of; PATH names it in errors.
pub fn open_ext(path: &str, kind: &ExtKind) -> Result<Box<dyn Ext>, String> {
    Ok(match kind {
        ExtKind::Lua { source } => Box::new(LuaModule::load(path, source)?),
        ExtKind::LuaCode { name, glue } => Box::new(lineit_rt::lua::LuaCode::load(name, glue)?),
        ExtKind::Process { name, commands, args, env, lock, host, load } => {
            let commands: Vec<&str> = commands.iter().map(String::as_str).collect();
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            Box::new(lineit_rt::process::Process::start(name, &commands, &args, &env, lock.as_deref(), host, load)?)
        }
        ExtKind::Native { crate_dir, crate_name, ty } => {
            Box::new(crate::native::Dylib::open(crate_dir, crate_name, ty).map_err(|err| format!("{path}: {err}"))?)
        }
    })
}

impl Interp {
    pub fn new(prog: Program, base_dir: PathBuf) -> Result<Interp, String> {
        let code = prog.handlers.iter().map(compile).collect();
        let mut exts: Vec<Box<dyn Ext>> = Vec::new();
        for e in &prog.exts {
            exts.push(open_ext(&e.path, &e.kind)?);
        }
        let mut ext_instances = Vec::new();
        for i in &prog.ext_instances {
            let params: Vec<(&str, Value)> = i.params.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
            ext_instances.push((i.module, exts[i.module].instance(&params)?));
        }
        let handles = exts.iter_mut().map(|e| e.handles()).collect();
        let prog = Arc::new(prog);
        Ok(Interp {
            clock_next: vec![0.0; prog.clocks.len()],
            w: Arc::new(World::build(
                prog.clone(),
                exts,
                handles,
                ext_instances,
                base_dir,
                Arc::new(AtomicBool::new(false)),
                Arc::new(AtomicBool::new(false)),
            )),
            prog,
            code: Arc::new(code),
            tasks: Vec::new(),
            ops_run: 0,
            live: None,
        })
    }

    pub fn program(&self) -> &Program {
        &self.prog
    }

    /// Whether [`Interp::swap`] can run now: no branch is in the middle of
    /// anything, and the program hasn't ended.
    pub fn can_swap(&self) -> bool {
        self.is_idle() && !self.w.stopped()
    }

    /// The world that runs, as it is after each [`Interp::swap`]: for the
    /// thread that shows what the program does, and stops it. Ask for it
    /// after [`Interp::set_stop`].
    pub fn live(&mut self) -> Arc<Mutex<Arc<World>>> {
        self.live.get_or_insert_with(|| Arc::new(Mutex::new(self.w.clone()))).clone()
    }

    /// Makes INTERRUPT the flag that makes the branches stop without ending
    /// the program (see [`World::interrupt`]). Only before it starts.
    pub fn set_interrupt(&mut self, interrupt: Arc<AtomicBool>) {
        if let Some(w) = Arc::get_mut(&mut self.w) {
            w.interrupt = interrupt;
        }
    }

    /// Drops every branch, wherever it is: for an edit a program that never
    /// goes idle can't be given otherwise. Only when none runs on a thread
    /// (they've stopped, after [`World::interrupt`], or the program steps).
    /// The state stays; [`Interp::restart`] runs the entries again.
    pub fn abandon(&mut self) {
        self.tasks.clear();
        self.w.interrupt.store(false, Ordering::Relaxed);
    }

    /// Runs the entries of the program again, with the state it has.
    pub fn restart(&mut self) -> Result<(), String> {
        let starts = self.prog.starts.clone();
        self.root(&starts)
    }

    /// Goes on with PROG (the canvas, edited) in place of the program that
    /// runs, keeping its state: the variables by name, the outputs that show
    /// on a square by square and pin (a value of another type is converted
    /// as `value/convert` does, or starts again), the clocks' schedule by
    /// square, and the modules and code squares that didn't change, with
    /// their state. What's gone is dropped. Entries that are new run now;
    /// the others don't run again. FILES: files the modules are made of
    /// changed on disk (a module that builds one, such as C# or Rust,
    /// opens again).
    ///
    /// It needs [`Interp::can_swap`]: this thread is the only one that
    /// touches the world, so no branch sees half of each program. If it
    /// fails the old program goes on as it was, except that a module that
    /// had to end before another opened (see `closes_first`) opens again
    /// without its state.
    pub fn swap(&mut self, prog: Program, files: bool) -> Result<Swapped, String> {
        if self.w.stopped() {
            return Err("the program is stopping".into());
        }
        if !self.is_idle() {
            return Err("a branch is in the middle of a handler".into());
        }
        let old = self.w.clone();
        let oldp = self.prog.clone();
        let reusable = |e: &ExtDef, o: &ExtDef| o.path == e.path && o.kind == e.kind && !(files && closes_first(e));
        let mut taken = vec![false; oldp.exts.len()];
        let mut origin: Vec<Option<usize>> = Vec::new();
        for e in &prog.exts {
            let same = (0..oldp.exts.len()).find(|&i| !taken[i] && reusable(e, &oldp.exts[i]));
            if let Some(i) = same {
                taken[i] = true;
            }
            origin.push(same);
        }
        // a module that holds what a new one needs ends first
        let opening = |j: usize| origin[j].is_none();
        let mut closed = Vec::new();
        if (0..prog.exts.len()).any(|j| opening(j) && closes_first(&prog.exts[j])) {
            for i in (0..oldp.exts.len()).filter(|&i| !taken[i] && closes_first(&oldp.exts[i])) {
                *lock(&old.exts[i]) = Box::new(Gone);
                closed.push(i);
            }
        }
        // what a failure undoes: the modules that ended open again
        let reopen = |why: String| -> String {
            let mut failed = Vec::new();
            for &i in &closed {
                let again = open_ext(&oldp.exts[i].path, &oldp.exts[i].kind).and_then(|mut m| {
                    for (j, oi) in oldp.ext_instances.iter().enumerate().filter(|(_, oi)| oi.module == i) {
                        let params: Vec<(&str, Value)> =
                            oi.params.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
                        if m.instance(&params)? != old.ext_instances[j].1 {
                            return Err("its squares changed places".to_string());
                        }
                    }
                    *lock(&old.handles[i]) = m.handles();
                    *lock(&old.exts[i]) = m;
                    Ok(())
                });
                if let Err(e) = again {
                    failed.push(format!("{}: {e}", oldp.exts[i].path));
                }
            }
            match (closed.is_empty(), failed.is_empty()) {
                (true, _) => why,
                (false, true) => format!("{why} (the modules that ended opened again, without their state)"),
                (false, false) => {
                    format!("{why} (and these couldn't open again, the program can't go on: {})", failed.join("; "))
                }
            }
        };
        let opened = (|| -> Result<(Vec<Option<Box<dyn Ext>>>, Vec<(usize, usize)>), String> {
            let mut fresh: Vec<Option<Box<dyn Ext>>> = Vec::new();
            for (j, e) in prog.exts.iter().enumerate() {
                fresh.push(if opening(j) { Some(open_ext(&e.path, &e.kind)?) } else { None });
            }
            // the squares of code: the same square with the same params keeps its state
            let mut used = vec![false; oldp.ext_instances.len()];
            let mut instances = Vec::new();
            for ni in &prog.ext_instances {
                let params: Vec<(&str, Value)> = ni.params.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
                let kept = origin[ni.module].and_then(|om| {
                    (0..oldp.ext_instances.len()).find(|&j| {
                        let oi = &oldp.ext_instances[j];
                        !used[j]
                            && old.ext_instances[j].0 == om
                            && oi.square == ni.square
                            && oi.params.len() == ni.params.len()
                            && oi
                                .params
                                .iter()
                                .zip(&ni.params)
                                .all(|(a, b)| a.0 == b.0 && lineit_rt::value::same(&a.1, &b.1))
                    })
                });
                let inst = match kept {
                    Some(j) => {
                        used[j] = true;
                        old.ext_instances[j].1
                    }
                    None => match origin[ni.module] {
                        Some(om) => lock(&old.exts[om]).instance(&params)?,
                        None => fresh[ni.module].as_mut().expect("a new module").instance(&params)?,
                    },
                };
                instances.push((ni.module, inst));
            }
            Ok((fresh, instances))
        })();
        let (fresh, instances) = opened.map_err(reopen)?;
        // the modules move to the new world; if what's left to work out
        // fails they move back
        let (mut exts, mut handles) = (Vec::new(), Vec::new());
        for (j, f) in fresh.into_iter().enumerate() {
            match (f, origin[j]) {
                (Some(mut e), _) => {
                    handles.push(e.handles());
                    exts.push(e);
                }
                (None, Some(i)) => {
                    exts.push(std::mem::replace(&mut *lock(&old.exts[i]), Box::new(Gone)));
                    handles.push(std::mem::take(&mut *lock(&old.handles[i])));
                }
                (None, None) => unreachable!("a module is opened or kept"),
            }
        }
        let prog = Arc::new(prog);
        let w = World::build(
            prog.clone(),
            exts,
            handles,
            instances,
            old.base_dir.clone(),
            old.stop.clone(),
            old.interrupt.clone(),
        );
        *lock(&w.started) = *lock(&old.started);
        *lock(&w.checks) = lock(&old.checks).clone();
        // the slots that carry over
        let mut report = Swapped::default();
        let mut olds: HashMap<Keyed, usize> = HashMap::new();
        let old_keys = slot_keys(&oldp);
        for (i, ks) in old_keys.iter().enumerate() {
            for k in ks {
                olds.insert(k.clone(), i);
            }
        }
        let mut matched = vec![false; oldp.slots.len()];
        for (i, ks) in slot_keys(&prog).iter().enumerate() {
            let Some((k, o)) = ks.iter().find_map(|k| olds.get(k).map(|o| (k, *o))).filter(|(_, o)| !matched[*o])
            else {
                continue;
            };
            matched[o] = true;
            if oldp.slots[o].ty == prog.slots[i].ty {
                *lock(&w.slots[i]) = old.slot(o);
                report.kept += 1;
            } else if let Ok(e) =
                crate::registry::builtin::convert(Expr::Const(old.slot(o)), &oldp.slots[o].ty, &prog.slots[i].ty)
                && let Ok(v) = w.eval(&e, &[])
            {
                *lock(&w.slots[i]) = v;
                report.converted.push(slot_name(k));
            } else {
                report.reset.push(slot_name(k));
            }
        }
        report.dropped = (0..oldp.slots.len()).filter(|i| !matched[*i]).count();
        // the clocks: one that's still there keeps its time, or the new
        // interval if that comes first
        let now = lock(&w.started).elapsed().as_secs_f64();
        let mut before: HashMap<(SquareId, usize), f64> = HashMap::new();
        let mut seen: HashMap<SquareId, usize> = HashMap::new();
        for (c, t) in oldp.clocks.iter().zip(&self.clock_next) {
            let k = seen.entry(c.square).or_default();
            before.insert((c.square, *k), *t);
            *k += 1;
        }
        let mut clock_next = Vec::new();
        let mut seen: HashMap<SquareId, usize> = HashMap::new();
        for c in &prog.clocks {
            let k = seen.entry(c.square).or_default();
            let b = before.get(&(c.square, *k)).copied();
            *k += 1;
            match w.eval(&c.interval, &[]) {
                Ok(i) => {
                    let at = now + i.float().max(0.001);
                    clock_next.push(b.map_or(at, |b| b.min(at)));
                }
                Err(e) => {
                    // the modules go back, and the old program goes on
                    for (j, o) in origin.iter().enumerate() {
                        if let Some(i) = o {
                            *lock(&old.exts[*i]) = std::mem::replace(&mut *lock(&w.exts[j]), Box::new(Gone));
                            *lock(&old.handles[*i]) = std::mem::take(&mut *lock(&w.handles[j]));
                        }
                    }
                    return Err(reopen(format!("a clock's interval fails: {e}")));
                }
            }
        }
        // done: what's left of the modules that went away is shown, and they end
        for i in (0..oldp.exts.len()).filter(|&i| !taken[i]) {
            *lock(&old.exts[i]) = Box::new(Gone);
            if let Some(e) = &lock(&old.handles[i]).stderr {
                lock(&w.output).append(&mut lock(e));
            }
        }
        let mut out = std::mem::take(&mut *lock(&old.output));
        out.append(&mut lock(&w.output));
        *lock(&w.output) = out;
        report.started = 0;
        let fresh_entries = new_entries(&oldp, &prog);
        self.prog = prog.clone();
        self.code = Arc::new(prog.handlers.iter().map(compile).collect());
        self.tasks.clear();
        self.w = Arc::new(w);
        self.clock_next = clock_next;
        if let Some(l) = &self.live {
            *lock(l) = self.w.clone();
        }
        if !fresh_entries.is_empty() {
            report.started = fresh_entries.len();
            // can't fail: one frame each
            let _ = self.root(&Fire { targets: fresh_entries, meets: Vec::new() });
        }
        Ok(report)
    }

    /// Makes STOP the flag that stops the program (see [`World::stop`]), to
    /// set from another thread. Only before it starts.
    pub fn set_stop(&mut self, stop: Arc<AtomicBool>) {
        if let Some(w) = Arc::get_mut(&mut self.w) {
            w.stop = stop;
        }
    }

    pub fn eval(&mut self, e: &Expr, locals: &[Value]) -> Result<Value, String> {
        self.w.eval(e, locals)
    }

    /// Starts the program: its entries, as branches if there are several.
    pub fn start(&mut self) -> Result<(), String> {
        *lock(&self.w.started) = Instant::now();
        let starts = self.prog.starts.clone();
        self.root(&starts)?;
        for i in 0..self.prog.clocks.len() {
            self.clock_next[i] = self.interval(i)?;
        }
        Ok(())
    }

    fn root(&mut self, fire: &Fire) -> Result<(), String> {
        self.tasks.push(Task {
            frames: Vec::new(),
            inside: Vec::new(),
            waiting: None,
            parent: None,
            done: false,
            entered: false,
            ran: None,
        });
        let t = self.tasks.len() - 1;
        self.fire(t, fire)?;
        self.settle(t);
        Ok(())
    }

    fn interval(&mut self, clock: usize) -> Result<f64, String> {
        let e = self.prog.clone().clocks[clock].interval.clone();
        Ok(self.w.eval(&e, &[])?.float().max(0.001))
    }

    /// Seconds until the next clock ticks, if there's a clock.
    pub fn next_clock(&self) -> Option<f64> {
        let now = lock(&self.w.started).elapsed().as_secs_f64();
        self.clock_next.iter().map(|t| (t - now).max(0.0)).min_by(f64::total_cmp)
    }

    /// Ticks the clocks that are due (or the next one, with FORCE).
    pub fn tick_clocks(&mut self, force: bool) -> Result<(), String> {
        let now = lock(&self.w.started).elapsed().as_secs_f64();
        let next = self.clock_next.iter().cloned().fold(f64::INFINITY, f64::min);
        for i in 0..self.clock_next.len() {
            if self.clock_next[i] <= now || (force && self.clock_next[i] <= next) {
                let interval = self.interval(i)?;
                self.clock_next[i] =
                    if force { self.clock_next[i] + interval } else { (self.clock_next[i] + interval).max(now) };
                let h = self.prog.clocks[i].handler;
                self.root(&Fire { targets: vec![h], meets: vec![] })?;
            }
        }
        Ok(())
    }

    pub fn is_idle(&self) -> bool {
        self.tasks.iter().all(|t| t.done)
    }

    /// The squares the branches are about to run (while stepping).
    pub fn current(&self) -> Vec<NodeId> {
        let mut v = Vec::new();
        for t in self.tasks.iter().filter(|t| !t.done && t.waiting.is_none()) {
            if let Some(f) = t.frames.last()
                && let Op::Enter(n) = self.code[f.handler as usize].ops[f.pc]
            {
                v.push(n);
            }
        }
        v
    }

    /// Runs every branch for about BUDGET ops each.
    pub fn run(&mut self, budget: u64) -> Result<Status, String> {
        let mut i = 0;
        while i < self.tasks.len() {
            if !self.tasks[i].done && self.tasks[i].waiting.is_none() {
                self.run_task(i, budget, false)?;
            }
            i += 1;
        }
        self.gc();
        Ok(if self.is_idle() { Status::Idle } else { Status::Running })
    }

    /// Lets every branch run its next square, and stops each before the one
    /// after.
    pub fn step(&mut self) -> Result<Status, String> {
        self.step_with(|_, _, _| {})
    }

    /// [`Interp::step`], calling TURN with the branch (its [`TaskView`] is
    /// at that index) right before its turn and right after it, even if the
    /// turn failed: what the trace needs to tell what each branch did.
    pub(crate) fn step_with(&mut self, mut turn: impl FnMut(&Interp, usize, Turn)) -> Result<Status, String> {
        if self.is_idle() {
            if self.clock_next.is_empty() {
                return Ok(Status::Idle);
            }
            self.tick_clocks(true)?;
        }
        for t in &mut self.tasks {
            t.entered = false;
            t.ran = None;
        }
        let mut i = 0;
        while i < self.tasks.len() {
            if !self.tasks[i].done && self.tasks[i].waiting.is_none() {
                turn(self, i, Turn::Before);
                let r = self.run_task(i, u64::MAX, true);
                turn(self, i, Turn::After);
                r?;
            }
            i += 1;
        }
        self.gc();
        Ok(if self.is_idle() { Status::Idle } else { Status::Paused })
    }

    /// How many branches there are (tasks, done ones too, until all are).
    pub(crate) fn task_count(&self) -> usize {
        self.tasks.len()
    }

    pub(crate) fn task(&self, i: usize) -> TaskView {
        let t = &self.tasks[i];
        let inside = t.frames.iter().map(|f| self.prog.handlers[f.handler as usize].node).collect();
        TaskView { parent: t.parent, waiting: t.waiting.is_some(), ran: t.ran, inside }
    }

    /// Whether clocks may start more branches when these end.
    pub(crate) fn has_clocks(&self) -> bool {
        !self.clock_next.is_empty()
    }

    /// Forgets tasks that are done, if all are.
    fn gc(&mut self) {
        if self.is_idle() {
            self.tasks.clear();
        }
    }

    /// A task with no frames and nothing to wait for is done: its parent
    /// may go on.
    fn settle(&mut self, t: usize) {
        let mut t = t;
        loop {
            let task = &self.tasks[t];
            if task.done || !task.frames.is_empty() || task.waiting.is_some() {
                return;
            }
            self.tasks[t].done = true;
            let Some(p) = self.tasks[t].parent else { return };
            let (left, m) = self.tasks[p].waiting.clone().expect("a parent waits");
            if left > 1 {
                self.tasks[p].waiting = Some((left - 1, m));
                return;
            }
            self.tasks[p].waiting = None;
            // the meets that were reached run now, one after the other
            for h in m.reached_last_first() {
                // can't fail: it's not a tail call, and the frames are few
                let _ = call(&mut self.tasks[p], &self.code, h, false);
            }
            t = p;
        }
    }

    /// Fires FIRE from task T: a call, or branches, as tasks that take turns
    /// (for Step).
    fn fire(&mut self, t: usize, fire: &Fire) -> Result<(), String> {
        match fire.targets.len() {
            0 => {}
            1 => call(&mut self.tasks[t], &self.code, fire.targets[0], false)?,
            n => {
                let m = Meets::new(&fire.meets);
                let mut inside = self.tasks[t].inside.clone();
                inside.push(m.clone());
                self.tasks[t].waiting = Some((n, m));
                let entered = self.tasks[t].entered;
                for &h in &fire.targets {
                    let mut task = Task {
                        frames: Vec::new(),
                        inside: inside.clone(),
                        waiting: None,
                        parent: Some(t),
                        done: false,
                        entered,
                        ran: None,
                    };
                    call(&mut task, &self.code, h, false)?;
                    self.tasks.push(task);
                }
                for c in self.tasks.len() - n..self.tasks.len() {
                    self.settle(c);
                }
            }
        }
        Ok(())
    }

    fn run_task(&mut self, t: usize, budget: u64, stepping: bool) -> Result<(), String> {
        let code = self.code.clone();
        let mut left = budget;
        loop {
            let task = &mut self.tasks[t];
            if task.waiting.is_some() || task.done {
                return Ok(());
            }
            let before = left;
            let next = run_ops(task, &self.w, &code, &mut left, stepping);
            self.ops_run += before - left;
            match next? {
                Next::Budget => return Ok(()),
                Next::Done => {
                    self.settle(t);
                    return Ok(());
                }
                // stepping, branches take turns; running, they're threads
                Next::FanOut(f) if stepping => self.fire(t, &f)?,
                Next::FanOut(f) => {
                    let w = self.w.clone();
                    fan_out(&mut self.tasks[t], &f, &w, &code)?;
                    if w.stopped() {
                        self.stop_all();
                        return Ok(());
                    }
                    if w.interrupted() {
                        self.abandon();
                        return Ok(());
                    }
                }
                Next::Interrupt => {
                    self.abandon();
                    return Ok(());
                }
                Next::Stop => {
                    self.w.stop.store(true, Ordering::Relaxed);
                    self.stop_all();
                    return Ok(());
                }
            }
        }
    }

    /// The program ends: every branch, and the clocks.
    fn stop_all(&mut self) {
        for task in &mut self.tasks {
            task.done = true;
            task.frames.clear();
        }
        self.clock_next.clear();
    }
}

impl World {
    #[inline(never)]
    fn effect(&self, effect: &Effect, mut a: Vec<Value>) {
        let elem = |s: SlotId, prog: &Program| match &prog.slots[s as usize].ty {
            Type::List(t) | Type::Map(t) => (**t).clone(),
            _ => Type::Int,
        };
        match effect {
            Effect::ListPush(s) => lock(&self.slots[*s as usize]).list_mut().push(a.remove(0)),
            Effect::ListPop { list, into } => {
                let ty = elem(*list, &self.prog);
                let v = lock(&self.slots[*list as usize]).list_mut().pop().unwrap_or_else(|| Value::default_of(&ty));
                *lock(&self.slots[*into as usize]) = v;
            }
            Effect::ListSet(s) => {
                let v = a.remove(1);
                lineit_rt::list::set(lock(&self.slots[*s as usize]).list_mut(), a[0].int(), v);
            }
            Effect::ListInsert(s) => {
                let v = a.remove(1);
                lineit_rt::list::insert(lock(&self.slots[*s as usize]).list_mut(), a[0].int(), v);
            }
            Effect::ListRemove(s) => lineit_rt::list::remove(lock(&self.slots[*s as usize]).list_mut(), a[0].int()),
            Effect::ListClear(s) => lock(&self.slots[*s as usize]).list_mut().clear(),
            Effect::MapSet(s) => {
                let v = a.remove(1);
                lock(&self.slots[*s as usize]).map_mut().insert(a[0].str().to_string(), v);
            }
            Effect::MapDelete(s) => {
                lock(&self.slots[*s as usize]).map_mut().remove(a[0].str());
            }
            Effect::MapClear(s) => lock(&self.slots[*s as usize]).map_mut().clear(),
            Effect::ImageSave { ok } => {
                let saved = img::save(a[0].image(), a[1].str());
                *lock(&self.slots[*ok as usize]) = Value::Bool(saved);
            }
        }
    }

    /// The value of E. The common cases are here, and small, so the
    /// recursion is cheap; the rest are in [`World::eval_rest`].
    pub fn eval(&self, e: &Expr, locals: &[Value]) -> Result<Value, String> {
        Ok(match e {
            Expr::Const(v) => v.clone(),
            Expr::Slot(s) => self.slot(*s as usize),
            Expr::Local(l) => locals[*l as usize].clone(),
            Expr::Un(op, a) => un(*op, self.eval(a, locals)?),
            Expr::Bin(op, a, b) => {
                // and/or don't read b if they don't need it, as in the compiled code
                let a = self.eval(a, locals)?;
                match op {
                    Bin::And if !a.bool() => return Ok(Value::Bool(false)),
                    Bin::Or if a.bool() => return Ok(Value::Bool(true)),
                    _ => {}
                }
                bin(op, a, self.eval(b, locals)?)
            }
            _ => return self.eval_rest(e, locals),
        })
    }

    #[inline(never)]
    fn eval_rest(&self, e: &Expr, locals: &[Value]) -> Result<Value, String> {
        Ok(match e {
            Expr::Const(_) | Expr::Slot(_) | Expr::Local(_) | Expr::Un(..) | Expr::Bin(..) => unreachable!(),
            Expr::Default(t) => Value::default_of(t),
            Expr::If(c, a, b) => {
                if self.eval(c, locals)?.bool() {
                    self.eval(a, locals)?
                } else {
                    self.eval(b, locals)?
                }
            }
            Expr::Call(f, args) => {
                let a = args.iter().map(|x| self.eval(x, locals)).collect::<Result<Vec<_>, _>>()?;
                self.call(f, a)
            }
            Expr::ListGet(s, i) => {
                let i = self.eval(i, locals)?.int();
                let slot = lock(&self.slots[*s as usize]);
                let l = slot.list();
                match index(l.len(), i) {
                    Some(i) => l[i].clone(),
                    None => Value::default_of(&elem_type(&self.prog.slots[*s as usize].ty)),
                }
            }
            Expr::ListLen(s) => Value::Int(lock(&self.slots[*s as usize]).list().len() as i64),
            Expr::MapGet(s, k) => {
                let k = self.eval(k, locals)?;
                match &*lock(&self.slots[*s as usize]) {
                    Value::Map(m) if m.contains_key(k.str()) => m[k.str()].clone(),
                    _ => Value::default_of(&elem_type(&self.prog.slots[*s as usize].ty)),
                }
            }
            Expr::MapHas(s, k) => {
                let k = self.eval(k, locals)?;
                Value::Bool(matches!(&*lock(&self.slots[*s as usize]), Value::Map(m) if m.contains_key(k.str())))
            }
            Expr::MapKeys(s) => match &*lock(&self.slots[*s as usize]) {
                Value::Map(m) => Value::List(Arc::new(m.keys().map(|k| Value::Str(k.as_str().into())).collect())),
                _ => Value::List(Arc::default()),
            },
            Expr::MapLen(s) => Value::Int(match &*lock(&self.slots[*s as usize]) {
                Value::Map(m) => m.len() as i64,
                _ => 0,
            }),
            Expr::Record(r, fields) => {
                let fields = fields.iter().map(|x| self.eval(x, locals)).collect::<Result<Vec<_>, _>>()?;
                Value::Record(Arc::new(Record { ty: r.clone(), fields }))
            }
            Expr::Field(e, r, i) => match self.eval(e, locals)? {
                Value::Record(rec) => rec.fields[*i].clone(),
                _ => Value::default_of(&r.fields[*i].1),
            },
            Expr::Ext { instance, output, ty, inputs } => {
                let args = inputs
                    .iter()
                    .map(|(n, e)| Ok((n.as_str(), self.eval(e, locals)?)))
                    .collect::<Result<Vec<_>, String>>()?;
                let (m, inst) = self.ext_instances[*instance];
                let v = lock(&self.exts[m]).eval(inst, &args, &[(output.as_str(), ty)]);
                self.printed();
                v?.remove(0)
            }
        })
    }

    #[inline(never)]
    fn call(&self, f: &Func, a: Vec<Value>) -> Value {
        let s = |i: usize| a[i].str();
        let i = |n: usize| a[n].int();
        let fl = |n: usize| a[n].float();
        let im = |n: usize| a[n].image();
        let st = |x: String| Value::Str(x.into());
        match f {
            Func::ToStr(_) => st(a[0].text()),
            Func::StrToInt => Value::Int(ops::str_to_int(s(0))),
            Func::StrToFloat => Value::Float(ops::str_to_float(s(0))),
            Func::StrToBool => Value::Bool(ops::str_to_bool(s(0))),
            Func::Concat => st(text::concat(s(0), s(1))),
            Func::StrLen => Value::Int(text::length(s(0))),
            Func::Upper => st(text::upper(s(0))),
            Func::Lower => st(text::lower(s(0))),
            Func::Trim => st(text::trim(s(0))),
            Func::Find => Value::Int(text::find(s(0), s(1))),
            Func::Substring => st(text::substring(s(0), i(1), i(2))),
            Func::Split => {
                Value::List(Arc::new(text::split(s(0), s(1)).into_iter().map(|x| Value::Str(x.into())).collect()))
            }
            Func::Join => {
                let parts: Vec<String> = a[0].list().iter().map(|v| v.str().to_string()).collect();
                st(text::join(&parts, s(1)))
            }
            Func::Replace => st(text::replace(s(0), s(1), s(2))),
            Func::Repeat => st(text::repeat(s(0), i(1))),
            Func::Format => {
                let args: Vec<&dyn lineit_rt::Show> = a[1..].iter().map(|v| v as &dyn lineit_rt::Show).collect();
                st(text::format(s(0), &args))
            }
            Func::Clamp(Num::Int) => Value::Int(ops::int_clamp(i(0), i(1), i(2))),
            Func::Clamp(Num::Float) => Value::Float(ops::float_clamp(fl(0), fl(1), fl(2))),
            Func::Random(Num::Int) => Value::Int(rand::int(i(0), i(1))),
            Func::Random(Num::Float) => Value::Float(rand::float(fl(0), fl(1))),
            Func::Time => Value::Float(lock(&self.started).elapsed().as_secs_f64()),
            Func::ListAt(t) => {
                let l = a[0].list();
                match index(l.len(), i(1)) {
                    Some(x) => l[x].clone(),
                    None => Value::default_of(t),
                }
            }
            Func::ListSize => Value::Int(a[0].list().len() as i64),
            Func::Path => {
                let p = std::path::Path::new(s(0));
                st(if p.is_relative() && !s(0).is_empty() {
                    self.base_dir.join(p).to_string_lossy().into()
                } else {
                    s(0).to_string()
                })
            }
            Func::ImgOpen => Value::Image(img::open(s(0))),
            Func::ImgNew => Value::Image(img::Image::new(i(0), i(1), img::color(s(2)))),
            Func::ImgWidth => Value::Int(img::width(im(0))),
            Func::ImgHeight => Value::Int(img::height(im(0))),
            Func::ImgPixel => st(img::pixel(im(0), i(1), i(2))),
            Func::ImgGray => Value::Image(img::grayscale(im(0))),
            Func::ImgInvert => Value::Image(img::invert(im(0))),
            Func::ImgFlip => Value::Image(img::flip(im(0), a[1].bool())),
            Func::ImgRotate => Value::Image(img::rotate(im(0), i(1))),
            Func::ImgResize => Value::Image(img::resize(im(0), i(1), i(2))),
            Func::ImgCrop => Value::Image(img::crop(im(0), i(1), i(2), i(3), i(4))),
            Func::ImgBlur => Value::Image(img::blur(im(0), i(1))),
            Func::ImgBrightness => Value::Image(img::brightness(im(0), fl(1))),
            Func::ImgContrast => Value::Image(img::contrast(im(0), fl(1))),
            Func::ImgTint => Value::Image(img::tint(im(0), s(1), fl(2))),
            Func::ImgShader => Value::Image(lineit_rt::shader::apply_file(im(0), s(1), fl(2), &a[3..])),
        }
    }
}

fn is_tail(ops: &[Op], mut pc: usize) -> bool {
    for _ in 0..64 {
        match ops.get(pc) {
            Some(Op::Jump(to)) => pc = *to,
            Some(Op::Return) => return true,
            _ => return false,
        }
    }
    false
}

fn index(len: usize, i: i64) -> Option<usize> {
    let i = if i < 0 { len as i64 + i } else { i };
    (i >= 0 && (i as usize) < len).then_some(i as usize)
}

fn elem_type(t: &Type) -> Type {
    match t {
        Type::List(t) | Type::Map(t) => (**t).clone(),
        _ => Type::Int,
    }
}

fn un(op: Un, a: Value) -> Value {
    match op {
        Un::Neg(Num::Int) => Value::Int(a.int().wrapping_neg()),
        Un::Neg(Num::Float) => Value::Float(-a.float()),
        Un::Abs(Num::Int) => Value::Int(a.int().wrapping_abs()),
        Un::Abs(Num::Float) => Value::Float(a.float().abs()),
        Un::Not => Value::Bool(!a.bool()),
        Un::Widen | Un::BoolToFloat => Value::Float(a.float()),
        Un::Trunc => Value::Int(ops::float_to_int(a.float())),
        Un::Floor => Value::Int(ops::float_to_int(a.float().floor())),
        Un::Ceil => Value::Int(ops::float_to_int(a.float().ceil())),
        Un::Round => Value::Int(ops::float_to_int(a.float().round())),
        Un::Math(f) => Value::Float(f.apply(a.float())),
        Un::BoolToInt => Value::Int(a.bool() as i64),
        Un::IntToBool => Value::Bool(a.int() != 0),
        Un::FloatToBool => Value::Bool(a.float() != 0.0),
    }
}

fn ord(a: &Value, b: &Value) -> std::cmp::Ordering {
    use std::cmp::Ordering::Equal;
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x.cmp(y),
        (Value::Float(x), Value::Float(y)) => x.partial_cmp(y).unwrap_or(Equal),
        (Value::Str(x), Value::Str(y)) => x.cmp(y),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        _ => Equal,
    }
}

fn bin(op: &Bin, a: Value, b: Value) -> Value {
    use std::cmp::Ordering::*;
    let num = |n: &Num, fi: fn(i64, i64) -> i64, ff: fn(f64, f64) -> f64| match n {
        Num::Int => Value::Int(fi(a.int(), b.int())),
        Num::Float => Value::Float(ff(a.float(), b.float())),
    };
    // NaN is neither less nor more than anything, as in the compiled code
    let nan = matches!((&a, &b), (Value::Float(x), Value::Float(y)) if x.is_nan() || y.is_nan());
    match op {
        Bin::Add(n) => num(n, i64::wrapping_add, |x, y| x + y),
        Bin::Sub(n) => num(n, i64::wrapping_sub, |x, y| x - y),
        Bin::Mul(n) => num(n, i64::wrapping_mul, |x, y| x * y),
        Bin::Div(n) => num(n, ops::int_div, |x, y| x / y),
        Bin::Mod(n) => num(n, ops::int_mod, ops::float_mod),
        Bin::Pow(n) => num(n, ops::int_pow, f64::powf),
        Bin::Min(n) => num(n, i64::min, f64::min),
        Bin::Max(n) => num(n, i64::max, f64::max),
        Bin::Eq(_) => Value::Bool(a == b),
        Bin::Ne(_) => Value::Bool(a != b),
        Bin::Lt(_) => Value::Bool(!nan && ord(&a, &b) == Less),
        Bin::Le(_) => Value::Bool(!nan && ord(&a, &b) != Greater),
        Bin::Gt(_) => Value::Bool(!nan && ord(&a, &b) == Greater),
        Bin::Ge(_) => Value::Bool(!nan && ord(&a, &b) != Less),
        Bin::And => Value::Bool(a.bool() && b.bool()),
        Bin::Or => Value::Bool(a.bool() || b.bool()),
        Bin::Xor => Value::Bool(a.bool() ^ b.bool()),
    }
}

/// Runs PROG to the end, or while its clocks tick for SECONDS (None: no
/// limit), printing what it prints with PRINT.
pub fn run_to_end(
    prog: Program,
    base_dir: PathBuf,
    seconds: Option<f64>,
    mut print: impl FnMut(&str),
) -> Result<(), String> {
    let mut it = Interp::new(prog, base_dir)?;
    // what it printed is printed before what went wrong, if anything did
    let r = it.start();
    for line in it.w.take_output() {
        print(&line);
    }
    r?;
    let started = Instant::now();
    loop {
        let st = it.run(100_000);
        for line in it.w.take_output() {
            print(&line);
        }
        let st = st?;
        if st == Status::Idle {
            let Some(wait) = it.next_clock() else { return Ok(()) };
            if let Some(limit) = seconds
                && started.elapsed().as_secs_f64() + wait > limit
            {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_secs_f64(wait));
            let r = it.tick_clocks(false);
            for line in it.w.take_output() {
                print(&line);
            }
            r?;
        }
    }
}

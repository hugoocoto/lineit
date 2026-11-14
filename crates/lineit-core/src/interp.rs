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

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use lineit_rt::Ext;
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
    /// Per slot: whether a statement reads and changes it, so those that
    /// change it hold its lock ([`Program::read_and_changed`]).
    locked: Vec<bool>,
    any_locked: bool,
    slot_locks: Vec<Mutex<()>>,
    /// What ends the processes of extensions now, and what they write to
    /// stderr (see [`lineit_rt::ext::Handles`]).
    kills: Vec<Box<dyn Fn() + Send + Sync>>,
    stderrs: Vec<Arc<Mutex<Vec<String>>>>,
}

impl World {
    /// Ends the processes of extensions (code in Python, C...): one stuck in
    /// a call ends it, with an error. For Stop.
    pub fn kill_externals(&self) {
        for k in &self.kills {
            k();
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
        for e in &self.stderrs {
            out.append(&mut lock(e));
        }
        out
    }

    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
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
        if (*left).is_multiple_of(1024) && w.stopped() {
            return Ok(Next::Stop);
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
            Op::Enter(_) => {
                if stepping {
                    if task.entered {
                        frame.pc -= 1;
                        return Ok(Next::Budget);
                    }
                    task.entered = true;
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
            Op::Stop => return Ok(Next::Stop),
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
            Next::FanOut(f) => fan_out(task, &f, w, code)?,
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
        let mut t = Task { frames: Vec::new(), inside, waiting: None, parent: None, done: false, entered: false };
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
        ExtKind::Native { crate_dir, crate_name, ty } => Box::new(
            crate::native::Dylib::open(crate_dir, crate_name, ty).map_err(|err| format!("{path}: {err}"))?,
        ),
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
        let (mut kills, mut stderrs) = (Vec::new(), Vec::new());
        for e in &mut exts {
            let h = e.handles();
            kills.extend(h.kill);
            stderrs.extend(h.stderr);
        }
        let prog = Arc::new(prog);
        let locked = prog.read_and_changed();
        Ok(Interp {
            clock_next: vec![0.0; prog.clocks.len()],
            w: Arc::new(World {
                kills,
                stderrs,
                any_locked: locked.contains(&true),
                locked,
                slot_locks: prog.slots.iter().map(|_| Mutex::new(())).collect(),
                slots: prog.slots.iter().map(|s| Padded(Mutex::new(s.init.clone()))).collect(),
                prog: prog.clone(),
                exts: exts.into_iter().map(Mutex::new).collect(),
                ext_instances,
                output: Mutex::new(Vec::new()),
                checks: Mutex::new(Vec::new()),
                started: Mutex::new(Instant::now()),
                base_dir,
                stop: Arc::new(AtomicBool::new(false)),
            }),
            prog,
            code: Arc::new(code),
            tasks: Vec::new(),
            ops_run: 0,
        })
    }

    pub fn program(&self) -> &Program {
        &self.prog
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
        if self.is_idle() {
            if self.clock_next.is_empty() {
                return Ok(Status::Idle);
            }
            self.tick_clocks(true)?;
        }
        for t in &mut self.tasks {
            t.entered = false;
        }
        let mut i = 0;
        while i < self.tasks.len() {
            if !self.tasks[i].done && self.tasks[i].waiting.is_none() {
                self.run_task(i, u64::MAX, true)?;
            }
            i += 1;
        }
        self.gc();
        Ok(if self.is_idle() { Status::Idle } else { Status::Paused })
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

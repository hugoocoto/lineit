//! Modules that aren't IR: Lua modules ([`crate::lua`]) and Rust modules
//! from packages ([`crate::native`]). The interpreter and compiled programs
//! run both through this, by the names of their pins.

use crate::types::Type;
use crate::value::Value;

/// What a tick returns: the outputs it set (None for those it left), and
/// the tick outputs to fire, in order.
pub type Ticked = (Vec<Option<Value>>, Vec<String>);

/// What a tick did, as [`Ext::run`] says it: the outputs it set (None for
/// those it left), then the tick outputs it fired, in order, each with the
/// outputs (by index) that get their values just before it fires. So a
/// square that fires the same output for each item of a list gives each
/// its own values.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Run {
    pub set: Vec<Option<Value>>,
    pub fire: Vec<(String, Vec<(usize, Value)>)>,
}

pub trait Ext: Send {
    /// Makes the state of a square, with its PARAMS. Returns its number.
    fn instance(&mut self, params: &[(&str, Value)]) -> Result<usize, String>;

    /// The OUTPUTS (by name and type) of a pure square, from its INPUTS.
    fn eval(&mut self, inst: usize, inputs: &[(&str, Value)], outputs: &[(&str, &Type)]) -> Result<Vec<Value>, String>;

    /// Tick input INPUT fired (None: the program started).
    fn tick(
        &mut self,
        inst: usize,
        input: Option<&str>,
        inputs: &[(&str, Value)],
        outputs: &[(&str, &Type)],
    ) -> Result<Ticked, String>;

    /// What [`Ext::tick`] does, with each fire's own values. The
    /// interpreter and compiled programs call this one.
    fn run(
        &mut self,
        inst: usize,
        input: Option<&str>,
        inputs: &[(&str, Value)],
        outputs: &[(&str, &Type)],
    ) -> Result<Run, String> {
        let (set, fire) = self.tick(inst, input, inputs, outputs)?;
        Ok(Run { set, fire: fire.into_iter().map(|f| (f, Vec::new())).collect() })
    }

    /// What square INST keeps between ticks, by name, to show it.
    fn state(&mut self, inst: usize) -> Result<Vec<(String, Value)>, String> {
        let _ = inst;
        Ok(Vec::new())
    }

    /// What lets the program stop it while it runs, and see what it writes
    /// to stderr: for those that run in a process of their own. Asked
    /// once, before it's used.
    fn handles(&mut self) -> Handles {
        Handles::default()
    }

    /// Gives square INST these values to keep, as a test does.
    fn set_state(&mut self, inst: usize, state: &[(&str, Value)]) -> Result<(), String> {
        let _ = (inst, state);
        Err("it keeps nothing that can be set".into())
    }
}

#[cfg(feature = "lua")]
impl Ext for crate::lua::LuaModule {
    fn handles(&mut self) -> Handles {
        crate::lua::LuaModule::handles(self)
    }
    fn instance(&mut self, params: &[(&str, Value)]) -> Result<usize, String> {
        crate::lua::LuaModule::instance(self, params)
    }
    fn eval(&mut self, inst: usize, inputs: &[(&str, Value)], outputs: &[(&str, &Type)]) -> Result<Vec<Value>, String> {
        crate::lua::LuaModule::eval(self, inst, inputs, outputs)
    }
    fn tick(
        &mut self,
        inst: usize,
        input: Option<&str>,
        inputs: &[(&str, Value)],
        outputs: &[(&str, &Type)],
    ) -> Result<Ticked, String> {
        crate::lua::LuaModule::tick(self, inst, input, inputs, outputs)
    }
}

/// What [`Ext::handles`] gives.
#[derive(Default)]
pub struct Handles {
    /// Ends it now (kills its process): a call it's in fails.
    pub kill: Option<Box<dyn Fn() + Send + Sync>>,
    /// The lines it wrote to stderr, if they're kept ([`capture_stderr`]).
    pub stderr: Option<std::sync::Arc<std::sync::Mutex<Vec<String>>>>,
}

static CAPTURE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// What the processes of extensions write to stderr is kept for the
/// program to show (the editor's Output), not written to its stderr.
pub fn capture_stderr() {
    CAPTURE.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Whether [`capture_stderr`] was called.
pub fn captures_stderr() -> bool {
    CAPTURE.load(std::sync::atomic::Ordering::Relaxed)
}

/// Where a child process's stderr goes: kept in a buffer, read by a
/// thread, if it's captured; the program's own stderr if not.
pub fn stderr_of(child: &mut std::process::Child) -> Option<std::sync::Arc<std::sync::Mutex<Vec<String>>>> {
    let err = child.stderr.take()?;
    let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    keep_lines(err, lines.clone());
    Some(lines)
}

/// The lines of FROM, as they come, into LINES (by a thread of its own).
pub fn keep_lines(from: impl std::io::Read + Send + 'static, lines: std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
    std::thread::spawn(move || {
        use std::io::BufRead;
        for line in std::io::BufReader::new(from).lines().map_while(Result::ok) {
            lines.lock().unwrap_or_else(|e| e.into_inner()).push(line);
        }
    });
}

/// How a child process's stderr is set up: piped if it's captured.
pub fn stderr_stdio() -> std::process::Stdio {
    if captures_stderr() { std::process::Stdio::piped() } else { std::process::Stdio::inherit() }
}

thread_local! {
    static PRINTED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// A line an extension prints (Lua's `print`, Python's, a code square's
/// `println!` or `printf`...): it goes where the program's own lines do,
/// in order, once the call that printed it returns ([`take_printed`]).
pub fn print(line: &str) {
    PRINTED.with(|p| p.borrow_mut().push(line.to_string()));
}

/// The lines extensions printed on this thread since this was last called.
pub fn take_printed() -> Vec<String> {
    PRINTED.with(|p| std::mem::take(&mut *p.borrow_mut()))
}

/// An extension failed in a compiled program: says why and ends it.
pub fn fail(e: &str) -> ! {
    crate::io::flush();
    eprintln!("lineit: {e}");
    std::process::exit(1)
}

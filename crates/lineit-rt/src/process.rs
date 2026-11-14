//! Code squares in languages that run in their own interpreter (Python, C#...):
//! one process for each text of code, which lineit talks to in JSON, a
//! message a line.
//!
//! The process runs HOST, a program in its language that lineit carries,
//! with LOAD (the code and how to call it) as its first line in. Then each
//! request is a line, and so is its answer:
//!
//! ```text
//! {"op": "new"}                               -> {"sq": 0}
//! {"op": "call", "sq": 0, "i": {"n": 4}}      -> {"set": {"pin": v}, "fire": [["even", {"half": 2}]]}
//! {"op": "state", "sq": 0}                    -> {"state": {"count": 3}}
//! {"op": "set_state", "sq": 0, "state": {...}} -> {}
//! ```
//!
//! Any answer can be `{"error": "..."}` instead, and any can have
//! `"printed": [lines]`: what the code printed, for the program's output.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};

use serde_json::{Map, Value as Json, json};

use crate::ext::{Ext, Run, Ticked};
use crate::types::Type;
use crate::value::{Record, Value};

pub struct Process {
    name: String,
    /// Shared with what stops it ([`Ext::handles`]), which can kill it
    /// while a call waits for it.
    child: Arc<Mutex<Child>>,
    stderr: Option<Arc<Mutex<Vec<String>>>>,
    to: ChildStdin,
    from: BufReader<ChildStdout>,
    /// The code's folder's run lock, held while it runs ([`busy`]).
    _run: Option<std::fs::File>,
}

/// The files of a code folder (see [`Process::start`]) that lock it: one
/// while a process of it starts, one while it runs.
const BUILD_LOCK: &str = "build.lock";
const RUN_LOCK: &str = "run.lock";

fn lock_file(dir: &str, name: &str) -> Option<std::fs::File> {
    std::fs::File::options().create(true).append(true).open(std::path::Path::new(dir).join(name)).ok()
}

/// Whether a process runs the code in DIR (a folder a code square's process
/// was started with): what stops it from being deleted.
pub fn busy(dir: &std::path::Path) -> bool {
    let Ok(f) = std::fs::File::open(dir.join(RUN_LOCK)) else { return false };
    fs4::FileExt::try_lock(&f).is_err()
}

impl Process {
    /// Starts the first of COMMANDS that runs (`python3`, `python`...),
    /// with ARGS and HOST after them, ENV set, and gives it LOAD.
    ///
    /// LOCK is the folder of code that is built when it starts (`dotnet
    /// run`): starts of it, by any process, go one at a time, and it can
    /// tell it's in use while this one runs ([`busy`]).
    pub fn start(
        name: &str,
        commands: &[&str],
        args: &[&str],
        env: &[(&str, &str)],
        lock: Option<&str>,
        host: &str,
        load: &str,
    ) -> Result<Process, String> {
        let build = lock.and_then(|d| lock_file(d, BUILD_LOCK));
        if let Some(f) = &build {
            let _ = fs4::FileExt::lock(f);
        }
        let mut tried = Vec::new();
        for cmd in commands {
            let spawned = Command::new(cmd)
                .args(args)
                .arg(host)
                .envs(env.iter().copied())
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(crate::ext::stderr_stdio())
                .spawn();
            match spawned {
                Ok(mut child) => {
                    let to = child.stdin.take().ok_or("no stdin")?;
                    let from = BufReader::new(child.stdout.take().ok_or("no stdout")?);
                    let stderr = crate::ext::stderr_of(&mut child);
                    let mut p =
                        Process { name: name.into(), child: Arc::new(Mutex::new(child)), stderr, to, from, _run: None };
                    // one that ends without answering isn't the language
                    // (Windows has a `python3` that only offers to install it)
                    match p.send_line(load).and_then(|_| p.first_answer()) {
                        Ok((Some(answer), said)) => {
                            let got = p.parse_map(answer);
                            // it's running: its folder is in use now
                            p._run = lock.and_then(|d| lock_file(d, RUN_LOCK)).inspect(|f| {
                                let _ = fs4::FileExt::lock_shared(f);
                            });
                            return got
                                .map(|_| p)
                                .map_err(|e| if said.is_empty() { e } else { format!("{e}\n{}", said.join("\n")) });
                        }
                        Ok((None, said)) => tried.push(match said.last() {
                            None => format!("{cmd}: it ended without answering"),
                            Some(_) => format!("{cmd}: it ended without answering:\n{}", said.join("\n")),
                        }),
                        Err(e) => tried.push(format!("{cmd}: {e}")),
                    }
                }
                Err(e) => tried.push(format!("{cmd}: {e}")),
            }
        }
        Err(format!("{name}: couldn't run its language ({})", tried.join("; ")))
    }

    /// Its first answer (a JSON object on a line), and the lines it said
    /// before it; or, if it ends without one, all it said (the last
    /// lines): what the tool that runs the code says first (a banner, build
    /// errors).
    fn first_answer(&mut self) -> Result<(Option<Map<String, Json>>, Vec<String>), String> {
        let mut said = Vec::new();
        while let Some(line) = self.answer_line()? {
            if let Ok(Json::Object(m)) = serde_json::from_str::<Json>(&line) {
                return Ok((Some(m), said));
            }
            if !line.trim().is_empty() {
                said.push(line.trim_end().to_string());
            }
        }
        // the last lines are the ones that tell
        said.drain(..said.len().saturating_sub(10));
        Ok((None, said))
    }

    fn send_line(&mut self, line: &str) -> Result<(), String> {
        let gone = |e: std::io::Error| format!("{}: the process isn't there ({e})", self.name);
        self.to.write_all(line.as_bytes()).map_err(gone)?;
        self.to.write_all(b"\n").map_err(gone)?;
        self.to.flush().map_err(gone)
    }

    /// A line it says, or None if it ended.
    fn answer_line(&mut self) -> Result<Option<String>, String> {
        let mut line = String::new();
        let n = self.from.read_line(&mut line).map_err(|e| format!("{}: {e}", self.name))?;
        Ok((n > 0).then_some(line))
    }

    fn answer(&mut self) -> Result<Map<String, Json>, String> {
        let Some(line) = self.answer_line()? else {
            let mut child = self.child.lock().unwrap_or_else(|e| e.into_inner());
            let status = child.try_wait().ok().flatten().map(|s| format!(" ({s})")).unwrap_or_default();
            return Err(format!("{}: the process ended{status}", self.name));
        };
        self.parse(&line)
    }

    fn parse(&self, line: &str) -> Result<Map<String, Json>, String> {
        let got: Json = serde_json::from_str(line).map_err(|e| format!("{}: a bad answer ({e}): {line}", self.name))?;
        let Json::Object(m) = got else { return Err(format!("{}: a bad answer: {line}", self.name)) };
        self.parse_map(m)
    }

    /// M, an answer: what it printed goes out, an error is the Err.
    fn parse_map(&self, m: Map<String, Json>) -> Result<Map<String, Json>, String> {
        for p in m.get("printed").and_then(Json::as_array).into_iter().flatten() {
            crate::ext::print(p.as_str().unwrap_or(""));
        }
        if let Some(e) = m.get("error") {
            return Err(e.as_str().map(String::from).unwrap_or_else(|| e.to_string()));
        }
        Ok(m)
    }

    fn ask(&mut self, msg: Json) -> Result<Map<String, Json>, String> {
        self.send_line(&msg.to_string())?;
        self.answer()
    }

    fn call(&mut self, sq: usize, inputs: &[(&str, Value)]) -> Result<Map<String, Json>, String> {
        let mut i = Map::new();
        for (k, v) in inputs {
            i.insert(k.to_string(), to_json(v)?);
        }
        self.ask(json!({ "op": "call", "sq": sq, "i": i }))
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        let mut child = self.child.lock().unwrap_or_else(|e| e.into_inner());
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// OUTPUTS' values in M, by name: None for those it doesn't have.
fn by_name(m: Option<&Json>, outputs: &[(&str, &Type)]) -> Result<Vec<Option<Value>>, String> {
    outputs
        .iter()
        .map(|(name, ty)| match m.and_then(|m| m.get(*name)) {
            None | Some(Json::Null) => Ok(None),
            Some(v) => from_json(v, ty).map(Some).map_err(|e| format!("`{name}`: {e}")),
        })
        .collect()
}

impl Ext for Process {
    fn handles(&mut self) -> crate::ext::Handles {
        let child = self.child.clone();
        crate::ext::Handles {
            kill: Some(Box::new(move || {
                let _ = child.lock().unwrap_or_else(|e| e.into_inner()).kill();
            })),
            stderr: self.stderr.clone(),
        }
    }

    fn instance(&mut self, _params: &[(&str, Value)]) -> Result<usize, String> {
        let got = self.ask(json!({ "op": "new" }))?;
        got.get("sq").and_then(Json::as_u64).map(|n| n as usize).ok_or_else(|| format!("{}: no square", self.name))
    }

    fn eval(&mut self, sq: usize, inputs: &[(&str, Value)], outputs: &[(&str, &Type)]) -> Result<Vec<Value>, String> {
        let got = self.call(sq, inputs)?;
        Ok(by_name(got.get("set"), outputs)?
            .into_iter()
            .zip(outputs)
            .map(|(v, (_, t))| v.unwrap_or_else(|| Value::default_of(t)))
            .collect())
    }

    fn tick(
        &mut self,
        sq: usize,
        input: Option<&str>,
        inputs: &[(&str, Value)],
        outputs: &[(&str, &Type)],
    ) -> Result<Ticked, String> {
        let run = self.run(sq, input, inputs, outputs)?;
        let mut set = run.set;
        let mut fire = Vec::new();
        for (name, values) in run.fire {
            for (o, v) in values {
                set[o] = Some(v);
            }
            fire.push(name);
        }
        Ok((set, fire))
    }

    fn run(
        &mut self,
        sq: usize,
        _input: Option<&str>,
        inputs: &[(&str, Value)],
        outputs: &[(&str, &Type)],
    ) -> Result<Run, String> {
        let got = self.call(sq, inputs)?;
        let mut fire = Vec::new();
        for f in got.get("fire").and_then(Json::as_array).into_iter().flatten() {
            let name = f.get(0).and_then(Json::as_str).ok_or_else(|| format!("{}: a bad fire: {f}", self.name))?;
            let values = by_name(f.get(1), outputs)?;
            fire.push((name.to_string(), values.into_iter().enumerate().filter_map(|(o, v)| Some((o, v?))).collect()));
        }
        Ok(Run { set: by_name(got.get("set"), outputs)?, fire })
    }

    fn state(&mut self, sq: usize) -> Result<Vec<(String, Value)>, String> {
        let got = self.ask(json!({ "op": "state", "sq": sq }))?;
        let mut v: Vec<(String, Value)> = got
            .get("state")
            .and_then(Json::as_object)
            .into_iter()
            .flatten()
            .filter_map(|(k, x)| Some((k.clone(), from_json_any(x)?)))
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(v)
    }

    fn set_state(&mut self, sq: usize, state: &[(&str, Value)]) -> Result<(), String> {
        let mut m = Map::new();
        for (k, v) in state {
            m.insert(k.to_string(), to_json(v)?);
        }
        self.ask(json!({ "op": "set_state", "sq": sq, "state": m })).map(|_| ())
    }
}

pub fn to_json(v: &Value) -> Result<Json, String> {
    Ok(match v {
        Value::Int(i) => json!(i),
        Value::Float(f) => json!(f),
        Value::Bool(b) => json!(b),
        Value::Str(s) => json!(&**s),
        Value::List(l) => Json::Array(l.iter().map(to_json).collect::<Result<_, _>>()?),
        Value::Map(m) => {
            Json::Object(m.iter().map(|(k, v)| Ok((k.clone(), to_json(v)?))).collect::<Result<_, String>>()?)
        }
        Value::Record(r) => Json::Object(
            r.ty.fields
                .iter()
                .zip(&r.fields)
                .map(|((n, _), v)| Ok((n.clone(), to_json(v)?)))
                .collect::<Result<_, String>>()?,
        ),
        Value::Image(_) => return Err("images can't go to another process".into()),
    })
}

pub fn from_json(v: &Json, ty: &Type) -> Result<Value, String> {
    let bad = || format!("expected {ty}, got {v}");
    Ok(match (ty, v) {
        (Type::Int, Json::Number(n)) => {
            Value::Int(n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)).ok_or_else(bad)?)
        }
        (Type::Float, Json::Number(n)) => Value::Float(n.as_f64().ok_or_else(bad)?),
        (Type::Bool, Json::Bool(b)) => Value::Bool(*b),
        (Type::Str, Json::String(s)) => Value::Str(s.as_str().into()),
        (Type::Str, Json::Number(n)) => Value::Str(n.to_string().into()),
        (Type::List(t), Json::Array(a)) => {
            Value::List(Arc::new(a.iter().map(|x| from_json(x, t)).collect::<Result<_, _>>()?))
        }
        (Type::Map(t), Json::Object(m)) => Value::Map(Arc::new(
            m.iter().map(|(k, x)| Ok((k.clone(), from_json(x, t)?))).collect::<Result<BTreeMap<_, _>, String>>()?,
        )),
        (Type::Record(r), Json::Object(m)) => {
            let mut fields = Vec::new();
            for (name, t) in &r.fields {
                fields.push(match m.get(name) {
                    None | Some(Json::Null) => Value::default_of(t),
                    Some(x) => from_json(x, t)?,
                });
            }
            Value::Record(Arc::new(Record { ty: r.clone(), fields }))
        }
        _ => return Err(bad()),
    })
}

/// V as a lineit value of whatever type it looks like, to show it.
pub fn from_json_any(v: &Json) -> Option<Value> {
    Some(match v {
        Json::Number(n) => match n.as_i64() {
            Some(i) => Value::Int(i),
            None => Value::Float(n.as_f64()?),
        },
        Json::Bool(b) => Value::Bool(*b),
        Json::String(s) => Value::Str(s.as_str().into()),
        Json::Array(a) => Value::List(Arc::new(a.iter().filter_map(from_json_any).collect())),
        Json::Object(m) => {
            Value::Map(Arc::new(m.iter().filter_map(|(k, x)| Some((k.clone(), from_json_any(x)?))).collect()))
        }
        Json::Null => return None,
    })
}

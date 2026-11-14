//! Languages added with a file, without building lineit: `NAME.lua` in a
//! `languages` folder (lineit's, in its data folder, or one next to the
//! canvas), a table that says how to run the language:
//!
//! ```lua
//! return {
//!     name = "javascript",
//!     title = "JavaScript",
//!     extensions = { "js" },
//!     commands = { "node" },      -- the first of them that runs
//!     args = { "-e" },            -- and these, then HOST
//!     template = "...",           -- what a new square has in it
//!     host = [[ ... ]],           -- a program in the language
//!     -- if it's there, to color its code: a compiled tree-sitter grammar
//!     -- and its highlight queries (files, or the query)
//!     grammar = "~/.local/share/nvim/site/parser/javascript.so",
//!     highlights = { "queries/ecma.scm", "queries/javascript.scm" },
//! }
//! ```
//!
//! Paths are from the file's folder (`~/` is the home folder). A grammar
//! that isn't there only means no colors.
//!
//! The language itself reads its code's signature, so lineit parses
//! nothing. HOST's first line in is a request:
//!
//! - `{"op": "signature", "source": CODE}`: it answers one line and ends:
//!   `{"name": ..., "doc": ..., "params": [{"name": "n", "type": "int"},
//!   {"name": "x", "type": null, "default": "2"}, {"name": "even", "exit":
//!   [["half", "int"]]}], "returns": [[null, "int"]], "state": ["count"],
//!   "shared": [["total", true]]}` (types are lineit's: `int`, `list<str>`,
//!   a record's name; a name of null: lineit names it), or `{"error":
//!   "line 3: ..."}`.
//! - `{"op": "load", ...}`: what to run (as [`Language::ext`] gives it,
//!   with `"signature"`, its own answer): it answers `{}` and then serves
//!   the requests of [`lineit_rt::process`].
//!
//! `examples/languages/javascript.lua` is one, for Node.js.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

use serde_json::{Value as Json, json};

use super::{Function, Language, Param, Role, exit_values, return_values};
use crate::ir::ExtKind;
use crate::registry::SigCx;

#[derive(Clone)]
pub struct FileLanguage {
    name: String,
    title: String,
    extensions: Vec<String>,
    commands: Vec<String>,
    args: Vec<String>,
    host: String,
    template: String,
    /// The grammar's library, the function in it, and the highlight
    /// queries (files or text), if it says them.
    grammar: Option<(std::path::PathBuf, String, Vec<String>)>,
    /// What loading them gave, once.
    colors: std::sync::Arc<OnceLock<Option<(tree_sitter::Language, String)>>>,
}

impl std::fmt::Debug for FileLanguage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileLanguage").field("name", &self.name).finish_non_exhaustive()
    }
}

/// PATH, with `~/` the home folder, from DIR if it's relative.
fn path_from(dir: &Path, path: &str) -> std::path::PathBuf {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    match (path.strip_prefix("~/"), home) {
        (Some(rest), Some(h)) => std::path::PathBuf::from(h).join(rest),
        _ => dir.join(path),
    }
}

impl FileLanguage {
    /// Its grammar and highlight query, loaded the first time they're
    /// asked for: None if it has none, or they can't be loaded.
    fn colors(&self) -> Option<&(tree_sitter::Language, String)> {
        self.colors
            .get_or_init(|| {
                let (lib, symbol, queries) = self.grammar.as_ref()?;
                let language = unsafe {
                    let l = libloading::Library::new(lib).ok()?;
                    let f: libloading::Symbol<unsafe extern "C" fn() -> *const ()> = l.get(symbol.as_bytes()).ok()?;
                    let f = *f;
                    // the grammar's code stays loaded while lineit runs
                    std::mem::forget(l);
                    tree_sitter::Language::from(tree_sitter_language::LanguageFn::from_raw(f))
                };
                let mut query = String::new();
                for q in queries {
                    let p = Path::new(q);
                    match std::fs::read_to_string(p) {
                        Ok(text) if q.ends_with(".scm") => query.push_str(&text),
                        _ => query.push_str(q),
                    }
                    query.push('\n');
                }
                Some((language, query))
            })
            .as_ref()
    }
}

/// The languages of the files lineit found.
static FILES: Mutex<Vec<FileLanguage>> = Mutex::new(Vec::new());

/// The languages from files, after the built-in ones.
pub fn file_languages() -> Vec<FileLanguage> {
    FILES.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// The languages in the files of DIRS: these are the ones from files from
/// now on. Says what couldn't be read.
pub fn load_dirs(dirs: &[&Path]) -> Vec<String> {
    let mut found = Vec::new();
    let mut errors = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "lua")).collect();
        paths.sort();
        for p in paths {
            let text = match std::fs::read_to_string(&p) {
                Ok(t) => t,
                Err(e) => {
                    errors.push(format!("{}: {e}", p.display()));
                    continue;
                }
            };
            match read_file(&text, &p.display().to_string()) {
                Ok(l) => found.push(l),
                Err(e) => errors.push(format!("{}: {e}", p.display())),
            }
        }
    }
    *FILES.lock().unwrap_or_else(|e| e.into_inner()) = found;
    errors
}

/// The language a file's TEXT says; NAME is its path.
pub fn read_file(text: &str, name: &str) -> Result<FileLanguage, String> {
    let dir = Path::new(name).parent().map(Path::to_path_buf).unwrap_or_default();
    crate::file::with_data(text, name, |t| {
        let s = |k: &str| t.get::<String>(k).map_err(|_| format!("it needs `{k}`, a string"));
        let list = |k: &str| -> Result<Vec<String>, String> {
            let l: mlua::Table = t.get(k).map_err(|_| format!("it needs `{k}`, a list of strings"))?;
            l.sequence_values::<String>().collect::<Result<_, _>>().map_err(|_| format!("`{k}` is a list of strings"))
        };
        let name = s("name")?;
        let grammar = t.get::<String>("grammar").ok().map(|g| {
            let symbol = t.get::<String>("grammar_function").unwrap_or_else(|_| format!("tree_sitter_{}", name.replace('-', "_")));
            let queries: Vec<String> = match t.get::<mlua::Value>("highlights") {
                Ok(mlua::Value::String(q)) => vec![q.to_string_lossy()],
                Ok(mlua::Value::Table(l)) => l.sequence_values::<String>().flatten().collect(),
                _ => Vec::new(),
            };
            let queries = queries
                .into_iter()
                .map(|q| if q.ends_with(".scm") { path_from(&dir, &q).to_string_lossy().into_owned() } else { q })
                .collect();
            (path_from(&dir, &g), symbol, queries)
        });
        Ok(FileLanguage {
            grammar,
            colors: Default::default(),
            title: t.get::<String>("title").unwrap_or_else(|_| name.clone()),
            extensions: list("extensions").unwrap_or_default(),
            commands: list("commands")?,
            args: list("args").unwrap_or_default(),
            host: s("host")?,
            template: t.get::<String>("template").unwrap_or_default(),
            name,
        })
    })
}

impl FileLanguage {
    /// What its host says SOURCE's signature is, asked once per text (and
    /// host: a changed one is asked again).
    fn signature(&self, source: &str) -> Result<Json, String> {
        type Cache = HashMap<(String, String), Result<Json, String>>;
        static SIGS: OnceLock<Mutex<Cache>> = OnceLock::new();
        let key = (format!("{}\n{}", self.name, self.host), source.to_string());
        let sigs = SIGS.get_or_init(Default::default);
        if let Some(r) = sigs.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
            return r.clone();
        }
        let r = self.ask_signature(source);
        let mut sigs = sigs.lock().unwrap_or_else(|e| e.into_inner());
        if sigs.len() > 256 {
            sigs.clear();
        }
        sigs.insert(key, r.clone());
        r
    }

    fn ask_signature(&self, source: &str) -> Result<Json, String> {
        let request = json!({ "op": "signature", "source": source }).to_string();
        let mut tried = Vec::new();
        for cmd in &self.commands {
            let child = Command::new(cmd)
                .args(&self.args)
                .arg(&self.host)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn();
            let mut child = match child {
                Ok(c) => c,
                Err(e) => {
                    tried.push(format!("{cmd}: {e}"));
                    continue;
                }
            };
            if let Some(mut to) = child.stdin.take() {
                let _ = to.write_all(request.as_bytes());
                let _ = to.write_all(b"\n");
            }
            // its answer, or (after a while) none
            let from = child.stdout.take().ok_or("no stdout")?;
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let mut line = String::new();
                let _ = std::io::BufReader::new(from).read_line(&mut line);
                let _ = tx.send(line);
            });
            let line = rx.recv_timeout(std::time::Duration::from_secs(20)).unwrap_or_default();
            let _ = child.kill();
            let status = child.wait().ok();
            if line.trim().is_empty() {
                let mut err = String::new();
                if let Some(mut e) = child.stderr.take() {
                    let _ = std::io::Read::read_to_string(&mut e, &mut err);
                }
                let err = err.lines().last().unwrap_or("").trim().to_string();
                tried.push(format!("{cmd}: it didn't answer{}", if err.is_empty() { String::new() } else { format!(" ({err})") }));
                let _ = status;
                continue;
            }
            let got: Json = serde_json::from_str(&line).map_err(|e| format!("{}: a bad answer ({e}): {line}", self.name))?;
            if let Some(e) = got.get("error") {
                return Err(e.as_str().map(String::from).unwrap_or_else(|| e.to_string()));
            }
            return Ok(got);
        }
        Err(format!("couldn't run {} ({})", self.title, tried.join("; ")))
    }
}

/// A type the host says, as lineit's.
fn lineit_type(t: &Json, cx: &dyn SigCx, what: &str) -> Result<lineit_rt::Type, String> {
    let s = t.as_str().ok_or_else(|| format!("`{what}`: its type isn't text: {t}"))?;
    cx.parse_type(s).map_err(|e| format!("`{what}`: {e}"))
}

impl Language for FileLanguage {
    fn name(&self) -> &str {
        &self.name
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn extensions(&self) -> Vec<String> {
        self.extensions.clone()
    }

    fn template(&self) -> &str {
        &self.template
    }

    fn highlights(&self) -> Option<(tree_sitter::Language, &str)> {
        self.colors().map(|(l, q)| (l.clone(), q.as_str()))
    }

    fn read(&self, source: &str, cx: &dyn SigCx) -> Result<Function, String> {
        let sig = self.signature(source)?;
        let text = |k: &str| sig.get(k).and_then(Json::as_str).unwrap_or("").to_string();
        let name = text("name");
        if name.is_empty() {
            return Err(format!("{} gave no function's name", self.title));
        }
        let mut params = Vec::new();
        for p in sig.get("params").and_then(Json::as_array).into_iter().flatten() {
            let pname = p.get("name").and_then(Json::as_str).ok_or("a parameter with no name")?.to_string();
            let role = match p.get("exit").filter(|e| !e.is_null()) {
                Some(values) => {
                    let mut v = Vec::new();
                    for value in values.as_array().into_iter().flatten() {
                        let n = value.get(0).and_then(Json::as_str).map(String::from);
                        let t = lineit_type(value.get(1).unwrap_or(&Json::Null), cx, &pname)?;
                        v.push((n, t));
                    }
                    Role::Exit(exit_values(&pname, v))
                }
                None => match p.get("type").filter(|t| !t.is_null()) {
                    Some(t) => Role::Input(Some(lineit_type(t, cx, &pname)?)),
                    None => Role::Input(None),
                },
            };
            let default = p.get("default").and_then(Json::as_str).map(String::from);
            params.push(Param { name: pname, role, default });
        }
        let mut returns = Vec::new();
        for r in sig.get("returns").and_then(Json::as_array).into_iter().flatten() {
            let n = r.get(0).and_then(Json::as_str).map(String::from);
            returns.push((n, lineit_type(r.get(1).unwrap_or(&Json::Null), cx, "what it returns")?));
        }
        let state = sig
            .get("state")
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
            .filter_map(|s| s.as_str().map(String::from))
            .collect();
        let shared = sig
            .get("shared")
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
            .filter_map(|s| Some((s.get(0)?.as_str()?.to_string(), s.get(1).and_then(Json::as_bool).unwrap_or(false))))
            .collect();
        Ok(Function {
            name,
            doc: text("doc"),
            params,
            returns: return_values(returns),
            state,
            shared,
            shared_types: Vec::new(),
            source: source.to_string(),
            file: None,
        })
    }

    fn ext(&self, f: &Function) -> Result<ExtKind, String> {
        let params: Vec<_> = f
            .params
            .iter()
            .map(|p| match &p.role {
                Role::Input(_) => json!({ "name": p.name, "exit": null }),
                Role::Exit(values) => json!({ "name": p.name, "exit": values.iter().map(|(pin, _)| pin).collect::<Vec<_>>() }),
            })
            .collect();
        let ext = self.extensions.first().cloned().unwrap_or_else(|| "txt".into());
        let load = json!({
            "op": "load",
            "name": f.name,
            "file": f.origin(&ext, true),
            "source": f.source,
            "params": params,
            "returns": f.returns.iter().map(|(pin, _)| pin).collect::<Vec<_>>(),
            "shared": f.shared.iter().map(|(g, set)| json!([g, set])).collect::<Vec<_>>(),
            "then": f.ticked() && f.exits().next().is_none(),
            "state": f.state,
            "signature": self.signature(&f.source)?,
        });
        Ok(ExtKind::Process {
            name: f.name.clone(),
            commands: self.commands.clone(),
            args: self.args.clone(),
            host: self.host.clone(),
            load: load.to_string(),
        })
    }
}

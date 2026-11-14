//! Modules: what a square can be.
//!
//! Every module says its params, and from their values its pins (its
//! [`Signature`]), and how it lowers to IR: [`Module::eval`] gives the
//! expression of a data output, [`Module::tick`] the statements a tick input
//! runs. Built-in modules are Rust ([`builtin`]); the others are loaded from
//! files: Lua modules ([`lua`]), subgraphs ([`subgraph`]) and the squares of
//! record types ([`record`]).

pub mod builtin;
pub mod code;
pub mod lua;
pub mod record;
pub mod subgraph;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use lineit_rt::{RecordType, Type, Value};

use crate::ir::{Block, Expr};
use crate::lower::Lx;

/// What can go in a param.
#[derive(Clone, Debug, PartialEq)]
pub enum ParamKind {
    Text,
    Int,
    Float,
    Bool,
    /// One of these.
    Choice(Vec<String>),
    /// A type (`int`, `list<str>`...), or `tick` too if ALLOW_TICK.
    Type {
        allow_tick: bool,
    },
    /// The name of a variable.
    Var,
    /// The path of a subgraph the square calls: a function. ARGS are the
    /// outputs of the square the subgraph's data inputs are wired from, in
    /// order (a `tick` input is wired from the square's `call` output).
    Function {
        args: Vec<String>,
    },
}

#[derive(Clone, Debug)]
pub struct ParamDef {
    pub name: String,
    pub kind: ParamKind,
    pub default: String,
}

impl ParamDef {
    pub fn new(name: &str, kind: ParamKind, default: &str) -> ParamDef {
        ParamDef { name: name.into(), kind, default: default.into() }
    }
}

/// A square's params, with the module's defaults for the missing ones.
pub struct Params<'a> {
    pub defs: &'a [ParamDef],
    pub values: &'a BTreeMap<String, String>,
}

impl Params<'_> {
    pub fn str(&self, name: &str) -> String {
        self.values
            .get(name)
            .cloned()
            .unwrap_or_else(|| self.defs.iter().find(|d| d.name == name).map(|d| d.default.clone()).unwrap_or_default())
    }
    pub fn int(&self, name: &str) -> i64 {
        lineit_rt::ops::str_to_int(&self.str(name))
    }
    pub fn float(&self, name: &str) -> f64 {
        lineit_rt::ops::str_to_float(&self.str(name))
    }
    pub fn bool(&self, name: &str) -> bool {
        lineit_rt::ops::str_to_bool(&self.str(name))
    }
}

/// A type in a signature: a type, or a type variable of the square (all the
/// pins with the same variable have the same type, found by inference), or a
/// list or map of one.
#[derive(Clone, Debug, PartialEq)]
pub enum TExpr {
    T(Type),
    Var(usize),
    ListOf(usize),
    MapOf(usize),
}

/// What a type variable can be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Any,
    /// int or float
    Num,
    /// what `compare/less` takes: numbers, strings, bools
    Ord,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PinTy {
    Tick,
    Data(TExpr),
}

#[derive(Clone, Debug)]
pub struct PinDef {
    pub name: String,
    pub ty: PinTy,
    /// For a data input: it has a field, with this text in it at first.
    pub field: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Signature {
    pub inputs: Vec<PinDef>,
    pub outputs: Vec<PinDef>,
    pub vars: Vec<Class>,
    /// A code square's function, as read when its pins were said: lowering
    /// uses it as it is, as its code may come from a file.
    pub code: Option<Arc<code::Function>>,
}

impl Signature {
    pub fn new() -> Signature {
        Signature::default()
    }

    /// Adds type variables, numbered from 0 in the order given.
    pub fn vars(mut self, v: &[Class]) -> Self {
        self.vars.extend_from_slice(v);
        self
    }

    /// A tick input.
    pub fn tick_in(mut self, name: &str) -> Self {
        self.inputs.push(PinDef { name: name.into(), ty: PinTy::Tick, field: None });
        self
    }

    /// A data input that only takes a wire.
    pub fn wire_in(mut self, name: &str, t: TExpr) -> Self {
        self.inputs.push(PinDef { name: name.into(), ty: PinTy::Data(t), field: None });
        self
    }

    /// A data input with a field, which starts with DEFAULT.
    pub fn input(mut self, name: &str, t: TExpr, default: &str) -> Self {
        self.inputs.push(PinDef { name: name.into(), ty: PinTy::Data(t), field: Some(default.into()) });
        self
    }

    pub fn tick_out(mut self, name: &str) -> Self {
        self.outputs.push(PinDef { name: name.into(), ty: PinTy::Tick, field: None });
        self
    }

    pub fn output(mut self, name: &str, t: TExpr) -> Self {
        self.outputs.push(PinDef { name: name.into(), ty: PinTy::Data(t), field: None });
        self
    }

    pub fn input_index(&self, name: &str) -> Option<usize> {
        self.inputs.iter().position(|p| p.name == name)
    }

    pub fn output_index(&self, name: &str) -> Option<usize> {
        self.outputs.iter().position(|p| p.name == name)
    }

    /// Whether it runs on ticks (and so keeps its outputs in slots), rather
    /// than being an expression.
    pub fn has_tick_input(&self) -> bool {
        self.inputs.iter().any(|p| p.ty == PinTy::Tick)
    }
}

/// The pins of a subgraph used as a function, as the square that calls it
/// sees them.
#[derive(Clone, Debug)]
pub struct FnPorts {
    /// It has a `tick` input: it runs on ticks, and the caller ticks it for
    /// each call. Without one it's pure, and the caller reads its outputs.
    pub tick: bool,
    /// Its data inputs, from top to bottom: (name, type, default).
    pub inputs: Vec<(String, Type, String)>,
    /// Its data outputs, from top to bottom.
    pub outputs: Vec<(String, Type)>,
}

/// What a module can look up to say its pins.
pub trait SigCx {
    /// The type of variable NAME, as seen from the square.
    fn var(&self, name: &str) -> Option<Type>;
    /// The variables the square sees, by name: its canvas's and the `::`
    /// ones.
    fn vars(&self) -> Vec<(String, Type)> {
        Vec::new()
    }
    fn record(&self, name: &str) -> Option<Arc<RecordType>>;
    /// The text of the file PATH (relative to the canvas's folder), which
    /// the pins then depend on: the editor lowers again when it changes.
    fn read(&self, path: &str) -> Result<String, String>;
    /// The pins of the subgraph PATH, to use it as a function.
    fn function(&self, path: &str) -> Result<FnPorts, String> {
        let _ = path;
        Err("there are no functions here".into())
    }
}

impl dyn SigCx + '_ {
    pub fn parse_type(&self, s: &str) -> Result<Type, String> {
        Type::parse(s, &|n| self.record(n)).ok_or_else(|| format!("`{s}` isn't a type"))
    }

    pub fn var_type(&self, name: &str) -> Result<Type, String> {
        if name.trim().is_empty() {
            return Err("it needs the name of a variable".into());
        }
        self.var(name).ok_or_else(|| format!("there's no variable `{name}` here (a var/var square declares one)"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Builtin,
    Lua,
    Native,
    Subgraph,
    Record,
}

pub trait Module: Send + Sync {
    fn path(&self) -> &str;
    fn doc(&self) -> &str;
    fn kind(&self) -> Kind;
    fn params(&self) -> &[ParamDef];
    fn signature(&self, params: &Params, cx: &dyn SigCx) -> Result<Signature, String>;

    /// The expression of data output OUTPUT. A square that runs on ticks
    /// keeps its outputs in slots, which is what this gives by default.
    fn eval(&self, cx: &mut Lx, output: usize) -> Result<Expr, String> {
        Ok(Expr::Slot(cx.out_slot(output)))
    }

    /// What tick input INPUT runs.
    fn tick(&self, cx: &mut Lx, input: usize) -> Result<Block, String> {
        let _ = (cx, input);
        Err("it has no tick inputs".into())
    }

    /// What it runs when the program starts, if anything.
    fn start(&self, cx: &mut Lx) -> Result<Option<Block>, String> {
        let _ = cx;
        Ok(None)
    }

    /// For a clock: the seconds between ticks and what each one runs.
    fn clock(&self, cx: &mut Lx) -> Result<Option<(Expr, Block)>, String> {
        let _ = cx;
        Ok(None)
    }

    /// The canvas, for a subgraph: lowering inlines it.
    fn subgraph(&self) -> Option<&subgraph::Subgraph> {
        None
    }
}

/// Every module lineit knows of, by path.
#[derive(Clone, Default)]
pub struct Registry {
    pub modules: BTreeMap<String, Arc<dyn Module>>,
    pub records: BTreeMap<String, Arc<RecordType>>,
    /// What couldn't be loaded, and why, to show the user.
    pub errors: Vec<String>,
}

impl Registry {
    /// The built-in modules only.
    pub fn builtin() -> Registry {
        let mut r = Registry::default();
        for m in builtin::all() {
            r.add(m);
        }
        r.add(Arc::new(code::CodeModule::new()));
        r
    }

    pub fn add(&mut self, m: Arc<dyn Module>) {
        self.modules.insert(m.path().to_string(), m);
    }

    pub fn get(&self, path: &str) -> Option<&Arc<dyn Module>> {
        self.modules.get(path)
    }

    /// The subgraphs that can be a function with INPUTS data inputs and one
    /// data output, for the editor to offer.
    pub fn functions(&self, inputs: usize) -> Vec<String> {
        let data = |s: &subgraph::Subgraph, m: &str| s.ports(m).iter().filter(|p| p.1 != "tick").count();
        self.modules
            .iter()
            .filter_map(|(path, m)| m.subgraph().map(|s| (path, s)))
            .filter(|(_, s)| data(s, "graph/input") == inputs && data(s, "graph/output") == 1)
            .map(|(path, _)| path.clone())
            .collect()
    }

    pub fn add_record(&mut self, r: Arc<RecordType>) {
        for m in record::modules(&r) {
            self.add(m);
        }
        self.records.insert(r.name.clone(), r);
    }

    /// Loads every module in DIR and its folders, with paths starting with
    /// PREFIX (`mypackage/`, or nothing): `*.type.lua` are record types,
    /// `*.graph.lua` subgraphs, `*.native.lua` the pins of Rust modules (of
    /// the crate in DIR's `../native`), and the other `*.lua` Lua modules.
    pub fn load_dir(&mut self, dir: &Path, prefix: &str) {
        let native = dir.parent().map(|p| p.join("native")).filter(|n| n.join("Cargo.toml").is_file());
        let mut files = Vec::new();
        collect(dir, "", &mut files);
        files.sort();
        // types first: the rest may use them
        for (rel, path) in files.iter().filter(|(r, _)| r.ends_with(".type.lua")) {
            match std::fs::read_to_string(path)
                .map_err(|e| e.to_string())
                .and_then(|src| record::read(&src, &rel_name(rel, ".type.lua"), &|n| self.records.get(n).cloned()))
            {
                Ok(r) => self.add_record(Arc::new(r)),
                Err(e) => self.errors.push(format!("{}: {e}", path.display())),
            }
        }
        for (rel, path) in files.iter().filter(|(r, _)| !r.ends_with(".type.lua")) {
            let loaded: Result<Arc<dyn Module>, String> =
                std::fs::read_to_string(path).map_err(|e| e.to_string()).and_then(|src| {
                    if rel.ends_with(".graph.lua") {
                        let p = format!("{prefix}{}", rel_name(rel, ".graph.lua"));
                        subgraph::Subgraph::read(&p, &src, Some(path)).map(|m| Arc::new(m) as Arc<dyn Module>)
                    } else if rel.ends_with(".native.lua") {
                        let p = format!("{prefix}{}", rel_name(rel, ".native.lua"));
                        match &native {
                            Some(crate_dir) => {
                                lua::ExtModuleDef::native(&p, &src, crate_dir).map(|m| Arc::new(m) as Arc<dyn Module>)
                            }
                            None => Err("a Rust module, and there's no native/ crate next to modules/".into()),
                        }
                    } else {
                        let p = format!("{prefix}{}", rel_name(rel, ".lua"));
                        lua::ExtModuleDef::lua(&p, &src).map(|m| Arc::new(m) as Arc<dyn Module>)
                    }
                });
            match loaded {
                Ok(m) => self.add(m),
                Err(e) => self.errors.push(format!("{}: {e}", path.display())),
            }
        }
    }

    /// The default value of a field of type TY, for a square made now.
    pub fn default_value(ty: &Type, text: &str) -> Value {
        Value::parse(text, ty).unwrap_or_else(|| Value::default_of(ty))
    }
}

fn rel_name(rel: &str, ext: &str) -> String {
    rel.strip_suffix(ext).unwrap_or(rel).to_string()
}

fn collect(dir: &Path, rel: &str, out: &mut Vec<(String, std::path::PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let rel = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
        let path = e.path();
        if path.is_dir() {
            collect(&path, &rel, out);
        } else if name.ends_with(".lua") {
            out.push((rel, path));
        }
    }
}

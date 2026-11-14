//! `code/function`: a square with a function in it, written in its square
//! or in a file, in a language lineit knows. Its pins are the function's,
//! read from its signature (with tree-sitter): lineit parses and runs
//! nothing of its own.
//!
//! What a square is, is said the language's own way (see each language's
//! file for how):
//!
//! - **The function** is the one the language would export: in Lua, the
//!   one function that isn't `local`. The others are its helpers.
//! - **Inputs** are its parameters.
//! - **Outputs** are what it returns (several, if the language returns
//!   several). If it runs on ticks, `then` fires when it returns.
//! - **Exits**: a parameter that is a function is a tick output, and what
//!   it's called with are that output's values, which fire, in the order
//!   they're called, when the function returns. A function with exits
//!   returns nothing.
//! - **State**: the variables at the top of its code that it changes. Each
//!   square has its own, and lineit can read them (to show them) and set
//!   them (to test it).
//! - **Globals**: `Shared.NAME` is variable `::NAME`. Those it sets are
//!   set when it returns.
//!
//! A function with state, exits, globals to set or nothing to return runs
//! on ticks (a `tick` input). The others are pure, like `math/add`: they
//! run each time a value of theirs is read.

mod c;
mod csharp;
pub use csharp::sdk_ready;
pub mod file;
mod lua;
mod python;
pub mod rust;

use std::sync::Arc;

use lineit_rt::Type;

use super::builtin::default_text;
use super::{Class, Kind, Module, ParamDef, ParamKind, Params, PinTy, SigCx, Signature, TExpr};
use crate::ir::*;
use crate::lower::Lx;

pub const PATH: &str = "code/function";

/// A language a code square can be written in.
pub trait Language: Send + Sync {
    fn name(&self) -> &str;
    /// What it's called, for people: `Lua`, `Python`...
    fn title(&self) -> &str;
    /// The extensions of its files: `lua`, `py`...
    fn extensions(&self) -> Vec<String>;
    /// What a new square has in it.
    fn template(&self) -> &str;
    /// Its grammar and the query that colors its code (the grammar's own),
    /// if it has them.
    fn highlights(&self) -> Option<(tree_sitter::Language, &str)>;
    /// The function in SOURCE.
    fn read(&self, source: &str, cx: &dyn SigCx) -> Result<Function, String>;
    /// How F runs: the extension that runs it, by its pin names.
    fn ext(&self, f: &Function) -> Result<ExtKind, String>;
}

/// The languages lineit knows.
pub fn languages() -> Vec<Box<dyn Language>> {
    let mut v: Vec<Box<dyn Language>> = vec![
        Box::new(lua::Lua),
        Box::new(python::Python),
        Box::new(rust::Rust),
        Box::new(c::C),
        Box::new(csharp::CSharp),
    ];
    // those from files, unless one is built in
    for l in file::file_languages() {
        if !v.iter().any(|b| b.name() == l.name()) {
            v.push(Box::new(l));
        }
    }
    v
}

/// What a piece of code is, for its color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paint {
    Keyword,
    String,
    Comment,
    Number,
    Function,
    Type,
    Constant,
    Operator,
}

const PAINTS: &[(&str, Paint)] = &[
    ("keyword", Paint::Keyword),
    ("string", Paint::String),
    ("comment", Paint::Comment),
    ("number", Paint::Number),
    ("function", Paint::Function),
    ("type", Paint::Type),
    ("constant", Paint::Constant),
    ("boolean", Paint::Constant),
    ("operator", Paint::Operator),
];

/// The pieces of SOURCE, in language LANG, to color: where each is (bytes)
/// and what it is. Remembers the last texts it colored.
pub fn highlight(lang: &str, source: &str) -> Vec<(std::ops::Range<usize>, Paint)> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};
    type Configs = HashMap<String, Option<HighlightConfiguration>>;
    static CONFIGS: OnceLock<Mutex<Configs>> = OnceLock::new();
    static DONE: OnceLock<Mutex<HashMap<(String, String), Vec<(std::ops::Range<usize>, Paint)>>>> = OnceLock::new();
    let key = (lang.to_string(), source.to_string());
    if let Some(v) = DONE.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
        return v.clone();
    }
    let mut configs = CONFIGS.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    let Some(l) = languages().into_iter().find(|l| l.name() == lang) else { return Vec::new() };
    let config = configs.entry(l.name().to_string()).or_insert_with(|| {
        let (grammar, query) = l.highlights()?;
        let mut c = HighlightConfiguration::new(grammar, l.name(), query, "", "").ok()?;
        let names: Vec<&str> = PAINTS.iter().map(|(n, _)| *n).collect();
        c.configure(&names);
        Some(c)
    });
    let Some(config) = config else { return Vec::new() };
    let mut out = Vec::new();
    let mut h = Highlighter::new();
    if let Ok(events) = h.highlight(config, source.as_bytes(), None, |_| None) {
        let mut paint: Vec<Paint> = Vec::new();
        for e in events.flatten() {
            match e {
                HighlightEvent::HighlightStart(s) => paint.push(PAINTS[s.0].1),
                HighlightEvent::HighlightEnd => {
                    paint.pop();
                }
                HighlightEvent::Source { start, end } => {
                    if let Some(p) = paint.last() {
                        out.push((start..end, *p));
                    }
                }
            }
        }
    }
    let mut done = DONE.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    if done.len() > 256 {
        done.clear();
    }
    done.insert(key, out.clone());
    out
}

/// The language of a square that doesn't say.
pub fn default_language() -> String {
    languages()[0].name().to_string()
}

/// How F, in language LANG, runs.
pub fn ext_of(lang: &str, f: &Function) -> Result<ExtKind, String> {
    let langs = languages();
    let l = langs.iter().find(|l| l.name() == lang).ok_or_else(|| format!("`{lang}` isn't a language it knows"))?;
    l.ext(f)
}

/// The code a square gets when its language goes from FROM to TO, if
/// CODE is FROM's template, untouched: TO's. None for FROM or CODE is the
/// square's default.
pub fn switched_template(from: Option<&str>, to: &str, code: Option<&str>) -> Option<String> {
    let langs = languages();
    let of = |name: &str| langs.iter().find(|l| l.name() == name).map(|l| l.template().to_string());
    let from = from.map(String::from).or_else(|| langs.first().map(|l| l.name().to_string()))?;
    let now = code.map(String::from).or_else(|| langs.first().map(|l| l.template().to_string()))?;
    if from == to || (of(&from).as_deref() != Some(now.as_str()) && !now.trim().is_empty()) {
        return None;
    }
    of(to)
}

/// A function, as any language says it.
#[derive(Debug)]
pub struct Function {
    pub name: String,
    /// What it says it does, from its doc comment.
    pub doc: String,
    /// Its parameters, in order.
    pub params: Vec<Param>,
    /// What it returns, as its outputs: pin name and type.
    pub returns: Vec<(String, Type)>,
    /// The names of what it keeps between calls.
    pub state: Vec<String>,
    /// The variables `::NAME` it uses, by NAME, and whether it sets them.
    pub shared: Vec<(String, bool)>,
    /// Their types, in the same order (the signature finds them).
    pub shared_types: Vec<Type>,
    /// The code, as written.
    pub source: String,
    /// The name of the file it's in, if it's in one (not in the square).
    pub file: Option<String>,
}

impl Function {
    /// What errors call the code: its file's name, or the function's (and
    /// EXT, the language's file extension, if WITH_EXT).
    pub fn origin(&self, ext: &str, with_ext: bool) -> String {
        match &self.file {
            Some(f) => f.clone(),
            None if with_ext => format!("{}.{ext}", self.name),
            None => self.name.clone(),
        }
    }
}

#[derive(Debug)]
pub struct Param {
    pub name: String,
    pub role: Role,
    /// What its field has at first, if the code gives it a default.
    pub default: Option<String>,
}

/// What a parameter is.
#[derive(Debug, PartialEq)]
pub enum Role {
    /// An input: of this type, or of the type of what's wired to it.
    Input(Option<Type>),
    /// A tick output: called with its values (pin name and type).
    Exit(Vec<(String, Type)>),
}

impl Function {
    pub fn inputs(&self) -> impl Iterator<Item = (&str, &Option<Type>)> {
        self.params.iter().filter_map(|p| match &p.role {
            Role::Input(t) => Some((p.name.as_str(), t)),
            _ => None,
        })
    }

    fn default_of(&self, input: &str) -> Option<&str> {
        self.params.iter().find(|p| p.name == input).and_then(|p| p.default.as_deref())
    }

    pub fn exits(&self) -> impl Iterator<Item = (&str, &[(String, Type)])> {
        self.params.iter().filter_map(|p| match &p.role {
            Role::Exit(v) => Some((p.name.as_str(), v.as_slice())),
            _ => None,
        })
    }

    /// Whether it runs on ticks, rather than being an expression.
    pub fn ticked(&self) -> bool {
        self.exits().next().is_some()
            || self.shared.iter().any(|(_, set)| *set)
            || !self.state.is_empty()
            || self.returns.is_empty()
    }
}

/// What a [`Function`] already knows, for reading its code again (for its
/// glue): the records it uses.
pub struct FunctionRecords<'a>(pub &'a Function);

impl SigCx for FunctionRecords<'_> {
    fn var(&self, _: &str) -> Option<Type> {
        None
    }
    fn record(&self, name: &str) -> Option<Arc<lineit_rt::RecordType>> {
        let mut found = None;
        let mut look = |t: &Type| visit(t, &mut |t| {
            if let Type::Record(r) = t
                && r.name == name
            {
                found = Some(r.clone());
            }
        });
        for p in &self.0.params {
            match &p.role {
                Role::Input(Some(t)) => look(t),
                Role::Exit(v) => v.iter().for_each(|(_, t)| look(t)),
                Role::Input(None) => {}
            }
        }
        self.0.returns.iter().for_each(|(_, t)| look(t));
        self.0.shared_types.iter().for_each(&mut look);
        found
    }
    fn read(&self, path: &str) -> Result<String, String> {
        Err(format!("{path}: no files here"))
    }
}

/// T and the types in it.
pub fn visit(t: &Type, f: &mut dyn FnMut(&Type)) {
    f(t);
    match t {
        Type::List(e) | Type::Map(e) => visit(e, f),
        Type::Record(r) => r.fields.iter().for_each(|(_, t)| visit(t, f)),
        _ => {}
    }
}

/// The pin name of variable `::NAME`, given to the function and back.
pub fn shared_pin(name: &str) -> String {
    format!("::{name}")
}

/// The pin names of an exit's values: their names, `NAME.value` for one
/// with none, `NAME.1`, `NAME.2`... for several.
pub fn exit_values(exit: &str, values: Vec<(Option<String>, Type)>) -> Vec<(String, Type)> {
    let n = values.len();
    values
        .into_iter()
        .enumerate()
        .map(|(i, (name, t))| match name {
            Some(name) => (name, t),
            None if n == 1 => (format!("{exit}.value"), t),
            None => (format!("{exit}.{}", i + 1), t),
        })
        .collect()
}

/// The pin names of what it returns: their names, `result` for one with
/// none, `out1`, `out2`... for several.
pub fn return_values(values: Vec<(Option<String>, Type)>) -> Vec<(String, Type)> {
    let n = values.len();
    values
        .into_iter()
        .enumerate()
        .map(|(i, (name, t))| match name {
            Some(name) => (name, t),
            None if n == 1 => ("result".into(), t),
            None => (format!("out{}", i + 1), t),
        })
        .collect()
}

pub struct CodeModule {
    params: Vec<ParamDef>,
    languages: Vec<Box<dyn Language>>,
}

impl CodeModule {
    pub fn new() -> CodeModule {
        let languages = languages();
        let names = languages.iter().map(|l| l.name().to_string()).collect();
        let template = languages[0].template().to_string();
        CodeModule {
            params: vec![
                ParamDef::new("language", ParamKind::Choice(names), languages[0].name()),
                ParamDef::new("code", ParamKind::Text, &template),
                ParamDef::new("file", ParamKind::Text, ""),
                // its tests (see crate::codetest)
                ParamDef::new("tests", ParamKind::Text, ""),
            ],
            languages,
        }
    }

    fn language(&self, name: &str) -> Result<&dyn Language, String> {
        self.languages.iter().find(|l| l.name() == name).map(|l| l.as_ref()).ok_or_else(|| {
            let names: Vec<&str> = self.languages.iter().map(|l| l.name()).collect();
            format!("`{name}` isn't a language it knows ({})", names.join(", "))
        })
    }
}

impl Default for CodeModule {
    fn default() -> Self {
        Self::new()
    }
}

fn function<'a>(cx: &Lx<'_, 'a>) -> Result<&'a Arc<Function>, String> {
    cx.sig().code.as_ref().ok_or_else(|| "it has no function".to_string())
}

/// The values it's given, by pin name: its inputs and the globals it uses.
fn args(cx: &mut Lx, f: &Function) -> Result<Vec<(String, Expr)>, String> {
    let mut v = Vec::new();
    for (name, _) in f.inputs() {
        let i = cx.sig().input_index(name).expect("an input of each");
        v.push((name.to_string(), cx.input(i)?));
    }
    for (name, _) in &f.shared {
        let pin = shared_pin(name);
        v.push((pin.clone(), Expr::Slot(cx.var(&pin)?)));
    }
    Ok(v)
}

impl CodeModule {
    fn instance(&self, cx: &mut Lx, f: &Function) -> Result<usize, String> {
        use std::hash::{Hash, Hasher};
        let lang = cx.params().str("language");
        let language = self.language(&lang)?;
        let kind = language.ext(f)?;
        // one module per text: squares with the same code share it (each
        // with its own state)
        let mut h = std::collections::hash_map::DefaultHasher::new();
        format!("{kind:?}").hash(&mut h);
        let ext = language.extensions().first().cloned().unwrap_or_default();
        let path = format!("{} ({lang} {:08x})", f.origin(&ext, true), h.finish() as u32);
        Ok(cx.ext_instance(&path, &kind, Vec::new()))
    }
}

impl Module for CodeModule {
    fn path(&self) -> &str {
        PATH
    }
    fn doc(&self) -> &str {
        "A function you write: its parameters are its inputs, what it returns its outputs, the functions it's given its exits. Its globals are its state, Shared.NAME is the variable ::NAME"
    }
    fn kind(&self) -> Kind {
        Kind::Builtin
    }
    fn params(&self) -> &[ParamDef] {
        &self.params
    }

    fn signature(&self, p: &Params, cx: &dyn SigCx) -> Result<Signature, String> {
        let file = p.str("file");
        let file = file.trim();
        let lang = self.language(&p.str("language"))?;
        let mut f = if file.is_empty() {
            lang.read(&p.str("code"), cx)?
        } else {
            let base = std::path::Path::new(file).file_name().map_or(file.to_string(), |b| b.to_string_lossy().into());
            // a file of another language: say so, rather than what reading it finds
            let ext = std::path::Path::new(file).extension().map(|e| e.to_string_lossy().to_lowercase());
            if let Some(ext) = ext.filter(|e| !lang.extensions().contains(e))
                && let Some(other) = self.languages.iter().find(|l| l.extensions().contains(&ext))
            {
                return Err(format!(
                    "`{base}` is {} (.{ext}), and this square's language is {}: set its language to {}",
                    other.title(),
                    lang.name(),
                    other.name()
                ));
            }
            let source = cx.read(file).map_err(|e| format!("can't read the code: {e}"))?;
            if source.trim().is_empty() {
                return Err(format!("`{base}` is empty: write a function in it"));
            }
            let mut f = lang.read(&source, cx).map_err(|e| format!("{base}: {e}"))?;
            f.file = Some(base);
            f
        };
        for (name, _) in &f.shared {
            let pin = shared_pin(name);
            let t = cx.var(&pin).ok_or_else(|| {
                format!("there's no variable `{pin}` for `Shared.{name}`: a var/var named `{pin}` declares it")
            })?;
            f.shared_types.push(t);
        }
        let mut s = Signature::new();
        let ticked = f.ticked();
        if ticked {
            s = s.tick_in("tick");
        }
        for (name, ty) in f.inputs() {
            if ticked && name == "tick" {
                return Err("an input can't be called `tick`: that's its tick input".into());
            }
            s = match ty {
                Some(t) if t.is_scalar() => {
                    s.input(name, TExpr::T(t.clone()), f.default_of(name).unwrap_or(default_text(t)))
                }
                Some(t) => s.wire_in(name, TExpr::T(t.clone())),
                None => {
                    let v = s.vars.len();
                    s.vars(&[Class::Any]).input(name, TExpr::Var(v), f.default_of(name).unwrap_or(""))
                }
            };
        }
        let mut outs: Vec<(&str, Option<&Type>)> = Vec::new();
        if f.exits().next().is_some() {
            if !f.returns.is_empty() {
                return Err(format!(
                    "`{}` has exits and returns values too: exits give the values, so it returns nothing",
                    f.name
                ));
            }
            for (name, values) in f.exits() {
                outs.push((name, None));
                outs.extend(values.iter().map(|(n, t)| (n.as_str(), Some(t))));
            }
        } else {
            if ticked {
                outs.push(("then", None));
            }
            outs.extend(f.returns.iter().map(|(n, t)| (n.as_str(), Some(t))));
        }
        for (k, (name, ty)) in outs.iter().enumerate() {
            if outs[..k].iter().any(|(n, _)| n == name) {
                return Err(format!("two outputs would be called `{name}`: give one of them another name"));
            }
            s = match ty {
                None => s.tick_out(name),
                Some(t) => s.output(name, TExpr::T((*t).clone())),
            };
        }
        s.code = Some(Arc::new(f));
        Ok(s)
    }

    fn eval(&self, cx: &mut Lx, output: usize) -> Result<Expr, String> {
        let f = function(cx)?.clone();
        if f.ticked() {
            return Ok(Expr::Slot(cx.out_slot(output)));
        }
        let inputs = args(cx, &f)?;
        let instance = self.instance(cx, &f)?;
        let pin = cx.sig().outputs[output].name.clone();
        Ok(Expr::Ext { instance, output: pin, ty: cx.out_ty(output), inputs })
    }

    fn tick(&self, cx: &mut Lx, _input: usize) -> Result<Block, String> {
        let f = function(cx)?.clone();
        let inputs = args(cx, &f)?;
        let mut outputs = Vec::new();
        let mut fires = Vec::new();
        for (o, pin) in cx.sig().outputs.iter().enumerate() {
            if pin.ty == PinTy::Tick {
                fires.push((pin.name.clone(), cx.fire_of(o)));
            } else {
                outputs.push((pin.name.clone(), cx.out_slot(o)));
            }
        }
        for (name, _) in f.shared.iter().filter(|(_, set)| *set) {
            let pin = shared_pin(name);
            outputs.push((pin.clone(), cx.var(&pin)?));
        }
        let instance = self.instance(cx, &f)?;
        Ok(vec![Stmt::Ext { instance, input: Some("tick".into()), inputs, outputs, fires }])
    }
}

/// How many crates of code squares lineit keeps, the last used.
const CRATES_KEPT: usize = 100;

/// The crate of a code square in DIR was just used: it's the last one to
/// go when there are more than [`CRATES_KEPT`], and the oldest go now.
pub fn crate_used(dir: &std::path::Path) {
    if let Ok(f) = std::fs::File::options().append(true).open(dir.join("Cargo.toml")) {
        let _ = f.set_modified(std::time::SystemTime::now());
    }
    evict(dir);
}

/// The folder of a C# square in DIR was just used, like [`crate_used`]. The
/// `used` file says when. A folder whose process runs isn't deleted (nor
/// are those of compiled programs that are running: one that isn't, and
/// finds its folder gone, doesn't run).
pub fn square_used(dir: &std::path::Path) {
    if let Ok(f) = std::fs::File::options().create(true).append(true).open(dir.join("used")) {
        let _ = f.set_modified(std::time::SystemTime::now());
    }
    evict(dir);
}

/// Deletes the oldest crates and squares' folders beside DIR (not DIR) that
/// are over [`CRATES_KEPT`] with it, those that aren't in use.
fn evict(dir: &std::path::Path) {
    let Some(all) = dir.parent() else { return };
    let Ok(entries) = std::fs::read_dir(all) else { return };
    let mut crates: Vec<(std::time::SystemTime, std::path::PathBuf)> = entries
        .flatten()
        .filter(|e| e.path() != dir)
        .filter_map(|e| {
            // a crate's manifest, or the `used` file of a C# square's folder
            let used = std::fs::metadata(e.path().join("Cargo.toml"))
                .or_else(|_| std::fs::metadata(e.path().join("used")))
                .and_then(|m| m.modified())
                .ok()?;
            Some((used, e.path()))
        })
        .collect();
    if crates.len() < CRATES_KEPT {
        return;
    }
    crates.sort();
    let mut over = crates.len() + 1 - CRATES_KEPT;
    for (_, old) in &crates {
        if over == 0 {
            break;
        }
        if !lineit_rt::process::busy(old) && std::fs::remove_dir_all(old).is_ok() {
            over -= 1;
        }
    }
}

/// The text of a string literal in Lua, Python, Rust or C (the escapes
/// they share): S, quoted.
pub fn quoted(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\t' => o.push_str("\\t"),
            '\r' => o.push_str("\\r"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\x{:02x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// A parse of SOURCE with LANGUAGE, refused if it has errors (with the
/// line of the first).
pub fn parse(source: &str, language: tree_sitter::Language, what: &str) -> Result<tree_sitter::Tree, String> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).map_err(|e| e.to_string())?;
    let tree = parser.parse(source, None).ok_or("couldn't read the code")?;
    let root = tree.root_node();
    if !root.has_error() {
        return Ok(tree);
    }
    // a token missing: where it should be
    if let Some(m) = descendants(root).into_iter().find(|n| n.is_missing()) {
        return Err(format!("line {}: `{}` is missing", m.start_position().row + 1, m.kind()));
    }
    // the first error, down to the error inside it, if any
    let Some(mut at) = descendants(root).into_iter().find(|n| n.is_error()) else {
        return Err(format!("this doesn't read as {what}"));
    };
    while let Some(inner) = descendants(at).into_iter().skip(1).find(|d| d.is_error()) {
        at = inner;
    }
    // what it's near: the error if it's short, else where it ends (where
    // reading it went wrong, more often than where it starts)
    let text = source[at.byte_range()].trim();
    let (row, near) = if text.lines().count() <= 1 && text.chars().count() <= 30 {
        (at.start_position().row, text.to_string())
    } else {
        let last = descendants(at).into_iter().rfind(|d| d.child_count() == 0).unwrap_or(at);
        (last.start_position().row, source[last.byte_range()].trim().to_string())
    };
    let near = if near.is_empty() { String::new() } else { format!(", near `{near}`") };
    Err(format!("line {}: this doesn't read as {what}{near}", row + 1))
}

/// The first `return` with a value in BODY (a `RETURN` node with a named
/// child), not counting those of the functions in it (NESTED kinds).
pub fn return_with_value<'a>(body: tree_sitter::Node<'a>, ret: &str, nested: &[&str]) -> Option<tree_sitter::Node<'a>> {
    let mut stack = vec![body];
    while let Some(n) = stack.pop() {
        if n.kind() == ret && n.named_child_count() > 0 {
            return Some(n);
        }
        let mut c = n.walk();
        let kids: Vec<_> = n.children(&mut c).filter(|k| !nested.contains(&k.kind())).collect();
        stack.extend(kids.into_iter().rev());
    }
    None
}

/// Every node under N (N too), depth first.
pub fn descendants(n: tree_sitter::Node) -> Vec<tree_sitter::Node> {
    let mut out = Vec::new();
    let mut stack = vec![n];
    while let Some(n) = stack.pop() {
        out.push(n);
        let mut c = n.walk();
        let kids: Vec<_> = n.children(&mut c).collect();
        stack.extend(kids.into_iter().rev());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lineit_rt::RecordType;
    use std::collections::BTreeMap;

    /// Files by name.
    struct Files(Vec<(&'static str, &'static str)>);
    impl SigCx for Files {
        fn var(&self, _: &str) -> Option<Type> {
            None
        }
        fn record(&self, _: &str) -> Option<Arc<RecordType>> {
            None
        }
        fn read(&self, path: &str) -> Result<String, String> {
            self.0.iter().find(|(n, _)| *n == path).map(|(_, s)| s.to_string()).ok_or_else(|| format!("{path}: not there"))
        }
    }

    fn sig(language: &str, file: &str, files: Vec<(&'static str, &'static str)>) -> Result<Signature, String> {
        let m = CodeModule::new();
        let values = BTreeMap::from([("language".to_string(), language.to_string()), ("file".to_string(), file.to_string())]);
        m.signature(&Params { defs: m.params(), values: &values }, &Files(files))
    }

    #[test]
    fn crates_kept() {
        let all = std::env::temp_dir().join(format!("lineit-crates-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&all);
        let make = |k: usize| {
            let d = all.join(format!("c{k:03}"));
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("Cargo.toml"), "").unwrap();
            let f = std::fs::File::options().append(true).open(d.join("Cargo.toml")).unwrap();
            f.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(k as u64 * 60)).unwrap();
            d
        };
        for k in 0..CRATES_KEPT + 5 {
            make(k);
        }
        // c000 is used again: it stays, the oldest others go
        crate_used(&all.join("c000"));
        let mut left: Vec<String> =
            std::fs::read_dir(&all).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into()).collect();
        left.sort();
        assert_eq!(left.len(), CRATES_KEPT);
        assert_eq!(left[..2], ["c000", "c006"]);
        let _ = std::fs::remove_dir_all(&all);
    }

    #[test]
    fn colors() {
        for l in languages() {
            let h = highlight(l.name(), l.template());
            let paints: Vec<Paint> = h.iter().map(|(_, p)| *p).collect();
            assert!(paints.contains(&Paint::Keyword) || paints.contains(&Paint::Function), "{}: {h:?}", l.name());
            assert!(paints.contains(&Paint::Comment) || paints.contains(&Paint::String), "{}: {h:?}", l.name());
        }
    }

    #[test]
    fn templates() {
        let lua = lua::Lua.template();
        assert_eq!(switched_template(None, "python", None).as_deref(), Some(python::Python.template()));
        assert_eq!(switched_template(Some("lua"), "c", Some(lua)).as_deref(), Some(c::C.template()));
        assert_eq!(switched_template(Some("lua"), "c", Some("function f() end")), None);
        assert_eq!(switched_template(Some("lua"), "lua", Some(lua)), None);
    }

    #[test]
    fn files() {
        let ok = sig("lua", "f.lua", vec![("f.lua", "---@return integer\nfunction f() return 1 end")]).unwrap();
        assert_eq!(ok.code.unwrap().origin("lua", false), "f.lua");
        let err = |l, f, files| sig(l, f, files).unwrap_err();
        assert_eq!(err("lua", "no.lua", vec![]), "can't read the code: no.lua: not there");
        assert_eq!(
            err("lua", "f.py", vec![("f.py", "def f(): pass")]),
            "`f.py` is Python (.py), and this square's language is lua: set its language to python"
        );
        assert_eq!(err("python", "e.py", vec![("e.py", "\n  \n")]), "`e.py` is empty: write a function in it");
        assert_eq!(
            err("lua", "b.lua", vec![("b.lua", "function f(s)\n  return s:reverse(\nend\n")]),
            "b.lua: line 3: this doesn't read as Lua, near `end`"
        );
        assert_eq!(
            err("python", "b.py", vec![("b.py", "def f(s):\n    return s +\n")]),
            "b.py: line 2: this doesn't read as Python, near `return s +`"
        );
        // a file with an extension no language has is read as the square's language
        assert!(sig("lua", "f.txt", vec![("f.txt", "---@return integer\nfunction f() return 1 end")]).is_ok());
    }
}

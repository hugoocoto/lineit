//! Compile: the IR to a Rust program, built by cargo with optimizations.
//!
//! What it makes is the program itself, with no interpreter in it: each
//! handler is a function, a pure square is an expression where it's read, a
//! `flow/loop` is a `for` loop, and the types are Rust's (`i64`, `f64`,
//! `String`, `Vec<T>`, a struct per record). What isn't plain Rust is a call
//! into `lineit-rt`, the same code the interpreter runs.
//!
//! - Handlers that fire each other in a cycle are one function looping over
//!   a `match`: a tick back into the cycle at the end of a handler is a
//!   jump, so it doesn't grow the stack.
//! - A program with no fan-out keeps its state in a struct passed as
//!   `&mut`. One with fan-outs runs its branches on threads and shares its
//!   state, as atomics and mutexes ([`lineit_rt::slot`]).
//! - Lua modules run in Lua, through `mlua`, and only then is it a
//!   dependency of the program; images bring the `image` crate.

use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use lineit_rt::{Type, Value};

use crate::ir::*;

pub struct Options {
    /// Where the program goes.
    pub out: PathBuf,
    /// The crate's name: the program's, and a hash of where its canvas is,
    /// so programs of canvases with the same name don't share a binary in
    /// the target folder they all use.
    pub name: String,
    /// Where the generated crate is built.
    pub build_dir: PathBuf,
    /// Without optimizations (faster to build).
    pub debug: bool,
}

impl Options {
    pub fn for_canvas(canvas: &Path, out: PathBuf) -> Options {
        let canonical = std::fs::canonicalize(canvas).unwrap_or_else(|_| canvas.to_path_buf());
        let name = format!("{}-{:016x}", program_name(canvas), fnv(canonical.to_string_lossy().as_bytes()));
        Options { out, build_dir: crate::paths::cache_dir().join("build").join(&name), name, debug: false }
    }
}

/// The name of the program a canvas compiles to.
pub fn program_name(canvas: &Path) -> String {
    let stem = canvas.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let mut name: String =
        stem.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    if !name.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
        name.insert_str(0, "program_");
    }
    name
}

pub(crate) fn fnv(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// The runtime's source, built into lineit, to write next to each program.
const RT_FILES: &[(&str, &str)] = &[
    ("Cargo.toml", include_str!("../../lineit-rt/Cargo.toml")),
    ("src/lib.rs", include_str!("../../lineit-rt/src/lib.rs")),
    ("src/branch.rs", include_str!("../../lineit-rt/src/branch.rs")),
    ("src/cabi.rs", include_str!("../../lineit-rt/src/cabi.rs")),
    ("src/ext.rs", include_str!("../../lineit-rt/src/ext.rs")),
    ("src/image.rs", include_str!("../../lineit-rt/src/image.rs")),
    ("src/io.rs", include_str!("../../lineit-rt/src/io.rs")),
    ("src/list.rs", include_str!("../../lineit-rt/src/list.rs")),
    ("src/lua.rs", include_str!("../../lineit-rt/src/lua.rs")),
    ("src/native.rs", include_str!("../../lineit-rt/src/native.rs")),
    ("src/ops.rs", include_str!("../../lineit-rt/src/ops.rs")),
    ("src/process.rs", include_str!("../../lineit-rt/src/process.rs")),
    ("src/rand.rs", include_str!("../../lineit-rt/src/rand.rs")),
    ("src/shader.rs", include_str!("../../lineit-rt/src/shader.rs")),
    ("src/shader/ast.rs", include_str!("../../lineit-rt/src/shader/ast.rs")),
    ("src/shader/builtins.rs", include_str!("../../lineit-rt/src/shader/builtins.rs")),
    ("src/shader/eval.rs", include_str!("../../lineit-rt/src/shader/eval.rs")),
    ("src/shader/parse.rs", include_str!("../../lineit-rt/src/shader/parse.rs")),
    ("src/shader/prep.rs", include_str!("../../lineit-rt/src/shader/prep.rs")),
    ("src/shader/val.rs", include_str!("../../lineit-rt/src/shader/val.rs")),
    ("src/show.rs", include_str!("../../lineit-rt/src/show.rs")),
    ("src/slot.rs", include_str!("../../lineit-rt/src/slot.rs")),
    ("src/text.rs", include_str!("../../lineit-rt/src/text.rs")),
    ("src/types.rs", include_str!("../../lineit-rt/src/types.rs")),
    ("src/value.rs", include_str!("../../lineit-rt/src/value.rs")),
];

/// Writes the runtime out, once per version of it, and says where.
pub fn write_runtime() -> Result<PathBuf, String> {
    let mut h = Vec::new();
    for (n, s) in RT_FILES {
        h.extend_from_slice(n.as_bytes());
        h.extend_from_slice(s.as_bytes());
    }
    let dir = crate::paths::cache_dir().join("rt").join(format!("{}-{:016x}", lineit_rt::VERSION, fnv(&h)));
    if !dir.join("src/lib.rs").exists() {
        let tmp = dir.with_extension("tmp");
        let _ = std::fs::remove_dir_all(&tmp);
        for (name, src) in RT_FILES {
            let p = tmp.join(name);
            std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| format!("{}: {e}", p.display()))?;
            std::fs::write(&p, src).map_err(|e| format!("{}: {e}", p.display()))?;
        }
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::rename(&tmp, &dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    Ok(dir)
}

/// Generates the program, builds it, and copies it to `opts.out`. LOG gets
/// what cargo says, a line at a time.
pub fn compile(prog: &Program, opts: &Options, log: &mut dyn FnMut(&str)) -> Result<PathBuf, String> {
    let rt = write_runtime()?;
    let src = generate(prog);
    let dir = &opts.build_dir;
    std::fs::create_dir_all(dir.join("src")).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut features = Vec::new();
    if uses_images(prog) {
        features.push("\"image\"");
    }
    if uses(prog, &|f| matches!(f, Func::ImgShader), false) {
        features.push("\"shader\"");
    }
    if prog.exts.iter().any(|e| matches!(e.kind, ExtKind::Lua { .. } | ExtKind::LuaCode { .. })) {
        features.push("\"lua\"");
    }
    if prog.exts.iter().any(|e| matches!(e.kind, ExtKind::Process { .. })) {
        features.push("\"process\"");
    }
    let rt_path = rt.to_string_lossy().replace('\\', "/");
    // Rust modules: their crates, using this runtime
    let mut natives = String::new();
    let mut crates = BTreeSet::new();
    for e in &prog.exts {
        if let ExtKind::Native { crate_dir, crate_name, .. } = &e.kind
            && crates.insert(crate_name.clone())
        {
            let dir = crate_dir.to_string_lossy().replace('\\', "/");
            let _ = writeln!(natives, "{crate_name} = {{ path = \"{dir}\" }}");
        }
    }
    let patch = if natives.is_empty() {
        String::new()
    } else {
        format!("[patch.crates-io]\nlineit-rt = {{ path = \"{rt_path}\" }}\n\n")
    };
    let manifest = format!(
        "# Made by lineit from a canvas: changes here are lost when it compiles it again.\n\
         [package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2024\"\npublish = false\n\n\
         [dependencies]\nlineit-rt = {{ path = \"{rt_path}\", features = [{}] }}\n{natives}\n{patch}\
         [profile.release]\nopt-level = 3\nlto = \"fat\"\ncodegen-units = 1\npanic = \"abort\"\nstrip = true\n\n\
         [workspace]\n",
        opts.name,
        features.join(", ")
    );
    write_if_changed(&dir.join("Cargo.toml"), &manifest)?;
    write_if_changed(&dir.join("src/main.rs"), &src)?;

    let cargo = std::env::var_os("CARGO").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("cargo"));
    let target = crate::paths::cache_dir().join("target");
    let build = |offline: bool, log: &mut dyn FnMut(&str)| -> Result<bool, String> {
        let mut cmd = lineit_rt::ext::command(&cargo);
        cmd.arg("build").arg("--manifest-path").arg(dir.join("Cargo.toml")).arg("--color=never");
        if !opts.debug {
            cmd.arg("--release");
        }
        if offline {
            cmd.arg("--offline");
        }
        cmd.env("CARGO_TARGET_DIR", &target).stdout(Stdio::null()).stderr(Stdio::piped());
        // the editor's own cargo settings, if it was started by cargo, aren't the program's
        cmd.env_remove("RUSTFLAGS").env_remove("CARGO_ENCODED_RUSTFLAGS");
        let mut child =
            cmd.spawn().map_err(|e| format!("couldn't run cargo ({e}): Compile needs Rust, from https://rustup.rs"))?;
        let stderr = child.stderr.take().unwrap();
        use std::io::BufRead;
        for line in std::io::BufReader::new(stderr).lines().map_while(Result::ok) {
            log(&line);
        }
        Ok(child.wait().map_err(|e| e.to_string())?.success())
    };
    if !build(false, log)? {
        log("lineit: trying again without the network (--offline)");
        if !build(true, log)? {
            return Err("cargo couldn't build the program (what it said is above)".into());
        }
    }
    let built = target.join(if opts.debug { "debug" } else { "release" }).join(crate::paths::exe(&opts.name));
    if let Some(p) = opts.out.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(p).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    // copy to a new file, then rename: the old one may be running
    let tmp = opts.out.with_extension("lineit-tmp");
    std::fs::copy(&built, &tmp).map_err(|e| format!("{} -> {}: {e}", built.display(), tmp.display()))?;
    std::fs::rename(&tmp, &opts.out).map_err(|e| format!("{}: {e}", opts.out.display()))?;
    Ok(opts.out.clone())
}

fn write_if_changed(path: &Path, text: &str) -> Result<(), String> {
    if std::fs::read_to_string(path).ok().as_deref() != Some(text) {
        std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

fn uses_images(prog: &Program) -> bool {
    uses(prog, &|f| matches!(f, Func::ImgOpen), true)
}

/// If the program calls a function PICK likes (or, if SAVE, saves an image).
fn uses(prog: &Program, pick: &dyn Fn(&Func) -> bool, save: bool) -> bool {
    fn expr(e: &Expr, pick: &dyn Fn(&Func) -> bool) -> bool {
        match e {
            Expr::Call(f, _) if pick(f) => true,
            Expr::Call(_, a) | Expr::Record(_, a) => a.iter().any(|e| expr(e, pick)),
            Expr::Un(_, a) | Expr::Field(a, ..) | Expr::ListGet(_, a) | Expr::MapGet(_, a) | Expr::MapHas(_, a) => {
                expr(a, pick)
            }
            Expr::Bin(_, a, b) => expr(a, pick) || expr(b, pick),
            Expr::If(a, b, c) => expr(a, pick) || expr(b, pick) || expr(c, pick),
            Expr::Ext { inputs, .. } => inputs.iter().any(|(_, e)| expr(e, pick)),
            _ => false,
        }
    }
    fn block(b: &Block, pick: &dyn Fn(&Func) -> bool, save: bool) -> bool {
        b.iter().any(|s| match s {
            Stmt::Do(Effect::ImageSave { .. }, _) if save => true,
            Stmt::Set(_, e) | Stmt::Let(_, e) | Stmt::Print(e) => expr(e, pick),
            Stmt::Do(_, a) => a.iter().any(|e| expr(e, pick)),
            Stmt::If(c, a, b) => expr(c, pick) || block(a, pick, save) || block(b, pick, save),
            Stmt::While(c, b) => expr(c, pick) || block(b, pick, save),
            Stmt::For { from, to, body, .. } => expr(from, pick) || expr(to, pick) || block(body, pick, save),
            Stmt::Ext { inputs, .. } => inputs.iter().any(|(_, e)| expr(e, pick)),
            _ => false,
        })
    }
    prog.handlers.iter().any(|h| block(&h.body, pick, save)) || prog.clocks.iter().any(|c| expr(&c.interval, pick))
}

/// The handlers in cycles of fires, grouped (Tarjan's strongly connected
/// components): for each handler, its group if it's in one.
fn cycles(prog: &Program) -> (Vec<Option<usize>>, Vec<Vec<HandlerId>>) {
    let calls = prog.calls();
    let n = calls.len();
    let mut index = vec![usize::MAX; n];
    let mut low = vec![0; n];
    let mut on = vec![false; n];
    let mut stack = Vec::new();
    let mut next = 0;
    let mut groups: Vec<Vec<HandlerId>> = Vec::new();
    // iterative, so a long chain of handlers doesn't overflow
    for root in 0..n {
        if index[root] != usize::MAX {
            continue;
        }
        let mut work: Vec<(usize, usize)> = vec![(root, 0)];
        while let Some((v, i)) = work.pop() {
            if i == 0 {
                index[v] = next;
                low[v] = next;
                next += 1;
                stack.push(v);
                on[v] = true;
            }
            if let Some(&w) = calls[v].get(i) {
                work.push((v, i + 1));
                let w = w as usize;
                if index[w] == usize::MAX {
                    work.push((w, 0));
                } else if on[w] {
                    low[v] = low[v].min(index[w]);
                }
                continue;
            }
            if let Some(&(parent, _)) = work.last() {
                low[parent] = low[parent].min(low[v]);
            }
            if low[v] == index[v] {
                let mut g = Vec::new();
                loop {
                    let w = stack.pop().unwrap();
                    on[w] = false;
                    g.push(w as HandlerId);
                    if w == v {
                        break;
                    }
                }
                let cyclic = g.len() > 1 || calls[v].contains(&(v as HandlerId));
                if cyclic {
                    g.sort_unstable();
                    groups.push(g);
                }
            }
        }
    }
    let mut of = vec![None; n];
    for (i, g) in groups.iter().enumerate() {
        for h in g {
            of[*h as usize] = Some(i);
        }
    }
    (of, groups)
}

struct Gen<'a> {
    prog: &'a Program,
    shared: bool,
    /// Per slot: whether the statements that change it hold its lock (with
    /// fan-outs only, see [`Program::read_and_changed`]).
    locked: Vec<bool>,
    meets: BTreeSet<HandlerId>,
    group_of: Vec<Option<usize>>,
}

fn rust_str(s: &str) -> String {
    format!("{s:?}")
}

/// The Rust for a [`Type`] value, for Lua calls.
fn type_value(t: &Type) -> String {
    match t {
        Type::Int => "lineit_rt::Type::Int".into(),
        Type::Float => "lineit_rt::Type::Float".into(),
        Type::Bool => "lineit_rt::Type::Bool".into(),
        Type::Str => "lineit_rt::Type::Str".into(),
        Type::Image => "lineit_rt::Type::Image".into(),
        Type::List(t) => format!("lineit_rt::Type::List(Box::new({}))", type_value(t)),
        Type::Map(t) => format!("lineit_rt::Type::Map(Box::new({}))", type_value(t)),
        Type::Record(r) => format!("lineit_rt::Type::Record(rec_{}())", r.name),
    }
}

fn value(v: &Value, t: &Type) -> String {
    match (v, t) {
        (Value::Int(i), _) => {
            if *i == i64::MIN {
                "i64::MIN".into()
            } else if *i < 0 {
                format!("({i}i64)")
            } else {
                format!("{i}i64")
            }
        }
        (Value::Float(f), _) => {
            if f.is_nan() {
                "f64::NAN".into()
            } else if f.is_infinite() {
                if *f > 0.0 { "f64::INFINITY".into() } else { "f64::NEG_INFINITY".into() }
            } else if f.is_sign_negative() {
                format!("({f:?}f64)")
            } else {
                format!("{f:?}f64")
            }
        }
        (Value::Bool(b), _) => b.to_string(),
        (Value::Str(s), _) => format!("String::from({})", rust_str(s)),
        (Value::List(l), Type::List(e)) if !l.is_empty() => {
            format!("vec![{}]", l.iter().map(|x| value(x, e)).collect::<Vec<_>>().join(", "))
        }
        (Value::Map(m), Type::Map(e)) if !m.is_empty() => format!(
            "std::collections::BTreeMap::from([{}])",
            m.iter()
                .map(|(k, x)| format!("(String::from({}), {})", rust_str(k), value(x, e)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        (Value::Record(r), Type::Record(rt)) => format!(
            "R_{} {{ {} }}",
            rt.name,
            rt.fields
                .iter()
                .zip(&r.fields)
                .map(|((n, ft), x)| format!("{}: {}", field(n), value(x, ft)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        _ => format!("<{}>::default()", t.rust()),
    }
}

/// A record field's name in Rust.
fn field(n: &str) -> String {
    format!("f_{n}")
}

fn is_copy(t: &Type) -> bool {
    matches!(t, Type::Int | Type::Float | Type::Bool)
}

fn join(v: impl IntoIterator<Item = String>) -> String {
    v.into_iter().collect::<Vec<_>>().join(", ")
}

impl Gen<'_> {
    fn slot_ty(&self, s: SlotId) -> &Type {
        &self.prog.slots[s as usize].ty
    }

    fn state_ty(&self) -> &'static str {
        if self.shared { "&State" } else { "&mut State" }
    }

    fn expr(&self, e: &Expr, locals: &[Type]) -> String {
        let x = |e: &Expr| self.expr(e, locals);
        match e {
            Expr::Const(v) => value(v, &v.ty()),
            Expr::Default(t) => format!("<{}>::default()", t.rust()),
            Expr::Slot(s) => format!("s.s{s}.get()"),
            Expr::Local(l) => {
                if is_copy(&locals[*l as usize]) {
                    format!("l{l}")
                } else {
                    format!("l{l}.clone()")
                }
            }
            Expr::Un(op, a) => {
                let a = x(a);
                match op {
                    Un::Neg(Num::Int) => format!("({a}).wrapping_neg()"),
                    Un::Neg(Num::Float) => format!("(-({a}))"),
                    Un::Abs(Num::Int) => format!("({a}).wrapping_abs()"),
                    Un::Abs(Num::Float) => format!("({a}).abs()"),
                    Un::Not => format!("(!({a}))"),
                    Un::Widen => format!("(({a}) as f64)"),
                    Un::Trunc => format!("lineit_rt::ops::float_to_int({a})"),
                    Un::Floor => format!("lineit_rt::ops::float_to_int(({a}).floor())"),
                    Un::Ceil => format!("lineit_rt::ops::float_to_int(({a}).ceil())"),
                    Un::Round => format!("lineit_rt::ops::float_to_int(({a}).round())"),
                    Un::Math(f) => format!("({a}).{}()", f.rust()),
                    Un::BoolToInt => format!("(({a}) as i64)"),
                    Un::IntToBool => format!("(({a}) != 0)"),
                    Un::FloatToBool => format!("(({a}) != 0.0)"),
                    Un::BoolToFloat => format!("((({a}) as i64) as f64)"),
                }
            }
            Expr::Bin(op, a, b) => {
                let (a, b) = (x(a), x(b));
                match op {
                    Bin::Add(Num::Int) => format!("({a}).wrapping_add({b})"),
                    Bin::Sub(Num::Int) => format!("({a}).wrapping_sub({b})"),
                    Bin::Mul(Num::Int) => format!("({a}).wrapping_mul({b})"),
                    Bin::Div(Num::Int) => format!("lineit_rt::ops::int_div({a}, {b})"),
                    Bin::Mod(Num::Int) => format!("lineit_rt::ops::int_mod({a}, {b})"),
                    Bin::Pow(Num::Int) => format!("lineit_rt::ops::int_pow({a}, {b})"),
                    Bin::Min(Num::Int) => format!("std::cmp::min::<i64>({a}, {b})"),
                    Bin::Max(Num::Int) => format!("std::cmp::max::<i64>({a}, {b})"),
                    Bin::Add(Num::Float) => format!("(({a}) + ({b}))"),
                    Bin::Sub(Num::Float) => format!("(({a}) - ({b}))"),
                    Bin::Mul(Num::Float) => format!("(({a}) * ({b}))"),
                    Bin::Div(Num::Float) => format!("(({a}) / ({b}))"),
                    Bin::Mod(Num::Float) => format!("lineit_rt::ops::float_mod({a}, {b})"),
                    Bin::Pow(Num::Float) => format!("f64::powf({a}, {b})"),
                    Bin::Min(Num::Float) => format!("f64::min({a}, {b})"),
                    Bin::Max(Num::Float) => format!("f64::max({a}, {b})"),
                    Bin::Eq(_) => format!("(({a}) == ({b}))"),
                    Bin::Ne(_) => format!("(({a}) != ({b}))"),
                    Bin::Lt(_) => format!("(({a}) < ({b}))"),
                    Bin::Le(_) => format!("(({a}) <= ({b}))"),
                    Bin::Gt(_) => format!("(({a}) > ({b}))"),
                    Bin::Ge(_) => format!("(({a}) >= ({b}))"),
                    Bin::And => format!("(({a}) && ({b}))"),
                    Bin::Or => format!("(({a}) || ({b}))"),
                    Bin::Xor => format!("(({a}) ^ ({b}))"),
                }
            }
            Expr::If(c, a, b) => format!("(if {} {{ {} }} else {{ {} }})", x(c), x(a), x(b)),
            Expr::Call(f, args) => self.call(f, args, locals),
            Expr::ListGet(s, i) => format!("{{ let i = {}; s.s{s}.with(|l| lineit_rt::list::get(l, i)) }}", x(i)),
            Expr::ListLen(s) => format!("s.s{s}.with(|l| l.len() as i64)"),
            Expr::MapGet(s, k) => format!("{{ let k = {}; s.s{s}.with(|m| lineit_rt::list::map_get(m, &k)) }}", x(k)),
            Expr::MapHas(s, k) => format!("{{ let k = {}; s.s{s}.with(|m| m.contains_key(&k)) }}", x(k)),
            Expr::MapKeys(s) => format!("s.s{s}.with(|m| lineit_rt::list::map_keys(m))"),
            Expr::MapLen(s) => format!("s.s{s}.with(|m| m.len() as i64)"),
            Expr::Record(r, fields) => format!(
                "R_{} {{ {} }}",
                r.name,
                join(r.fields.iter().zip(fields).map(|((n, _), e)| format!("{}: {}", field(n), x(e))))
            ),
            Expr::Field(e, r, i) => format!("({}).{}", x(e), field(&r.fields[*i].0)),
            Expr::Ext { instance, output, ty, inputs } => format!(
                "<{} as lineit_rt::value::Typed>::from_value(ext_eval(s, {instance}, {}, &{}, vec![{}]))",
                ty.rust(),
                rust_str(output),
                type_value(ty),
                self.lua_args(inputs, locals)
            ),
        }
    }

    fn lua_args(&self, inputs: &[(String, Expr)], locals: &[Type]) -> String {
        join(
            inputs.iter().map(|(n, e)| {
                format!("({}, lineit_rt::value::Typed::to_value(&{}))", rust_str(n), self.expr(e, locals))
            }),
        )
    }

    fn call(&self, f: &Func, args: &[Expr], locals: &[Type]) -> String {
        let a: Vec<String> = args.iter().map(|e| self.expr(e, locals)).collect();
        let r = |i: usize| format!("&{}", a[i]);
        let rt = "lineit_rt";
        match f {
            Func::ToStr(_) => format!("{rt}::show::text(&{})", a[0]),
            Func::StrToInt => format!("{rt}::ops::str_to_int({})", r(0)),
            Func::StrToFloat => format!("{rt}::ops::str_to_float({})", r(0)),
            Func::StrToBool => format!("{rt}::ops::str_to_bool({})", r(0)),
            Func::Concat => format!("{rt}::text::concat({}, {})", r(0), r(1)),
            Func::StrLen => format!("{rt}::text::length({})", r(0)),
            Func::Upper => format!("{rt}::text::upper({})", r(0)),
            Func::Lower => format!("{rt}::text::lower({})", r(0)),
            Func::Trim => format!("{rt}::text::trim({})", r(0)),
            Func::Find => format!("{rt}::text::find({}, {})", r(0), r(1)),
            Func::Substring => format!("{rt}::text::substring({}, {}, {})", r(0), a[1], a[2]),
            Func::Split => format!("{rt}::text::split({}, {})", r(0), r(1)),
            Func::Join => format!("{rt}::text::join({}, {})", r(0), r(1)),
            Func::Replace => format!("{rt}::text::replace({}, {}, {})", r(0), r(1), r(2)),
            Func::Repeat => format!("{rt}::text::repeat({}, {})", r(0), a[1]),
            Func::Format => {
                format!(
                    "{rt}::text::format({}, &[{}])",
                    r(0),
                    join(a[1..].iter().map(|x| format!("&{x} as &dyn {rt}::Show")))
                )
            }
            Func::Clamp(Num::Int) => format!("{rt}::ops::int_clamp({}, {}, {})", a[0], a[1], a[2]),
            Func::Clamp(Num::Float) => format!("{rt}::ops::float_clamp({}, {}, {})", a[0], a[1], a[2]),
            Func::Random(Num::Int) => format!("{rt}::rand::int({}, {})", a[0], a[1]),
            Func::Random(Num::Float) => format!("{rt}::rand::float({}, {})", a[0], a[1]),
            Func::Time => "s.start.elapsed().as_secs_f64()".into(),
            Func::ListAt(_) => format!("{rt}::list::get({}, {})", r(0), a[1]),
            Func::ListSize => format!("(({}).len() as i64)", a[0]),
            Func::Path => a[0].clone(),
            Func::ImgOpen => format!("{rt}::image::open({})", r(0)),
            Func::ImgNew => format!("{rt}::Image::new({}, {}, {rt}::image::color({}))", a[0], a[1], r(2)),
            Func::ImgWidth => format!("{rt}::image::width({})", r(0)),
            Func::ImgHeight => format!("{rt}::image::height({})", r(0)),
            Func::ImgPixel => format!("{rt}::image::pixel({}, {}, {})", r(0), a[1], a[2]),
            Func::ImgGray => format!("{rt}::image::grayscale({})", r(0)),
            Func::ImgInvert => format!("{rt}::image::invert({})", r(0)),
            Func::ImgFlip => format!("{rt}::image::flip({}, {})", r(0), a[1]),
            Func::ImgRotate => format!("{rt}::image::rotate({}, {})", r(0), a[1]),
            Func::ImgResize => format!("{rt}::image::resize({}, {}, {})", r(0), a[1], a[2]),
            Func::ImgCrop => format!("{rt}::image::crop({}, {}, {}, {}, {})", r(0), a[1], a[2], a[3], a[4]),
            Func::ImgBlur => format!("{rt}::image::blur({}, {})", r(0), a[1]),
            Func::ImgBrightness => format!("{rt}::image::brightness({}, {})", r(0), a[1]),
            Func::ImgContrast => format!("{rt}::image::contrast({}, {})", r(0), a[1]),
            Func::ImgTint => format!("{rt}::image::tint({}, {}, {})", r(0), r(1), a[2]),
            Func::ImgShader => format!(
                "{rt}::shader::apply_file({}, {}, {}, &[{}])",
                r(0),
                r(1),
                a[2],
                join(a[3..].iter().map(|x| format!("{rt}::value::Typed::to_value(&{x})")))
            ),
        }
    }

    /// Calls handler H. GROUP is the cycle the code is in, if any; TAIL
    /// whether nothing comes after in the handler.
    fn call_handler(&self, h: HandlerId, group: Option<usize>, tail: bool) -> String {
        let inner = match (group, self.group_of[h as usize]) {
            (Some(g), Some(hg)) if g == hg && tail => format!("{{ st = {h}; continue 'run; }}"),
            (Some(g), Some(hg)) if g == hg => format!("cycle{g}(s, {h});"),
            _ => format!("h{h}(s);"),
        };
        if self.shared && self.meets.contains(&h) {
            format!("if !lineit_rt::branch::arrive({h}) {{ {inner} }}")
        } else {
            inner
        }
    }

    fn fire(&self, f: &Fire, group: Option<usize>, tail: bool, ind: &str) -> String {
        match f.targets.len() {
            0 => String::new(),
            1 => format!("{ind}{}\n", self.call_handler(f.targets[0], group, tail)),
            _ => {
                let branches =
                    join(f.targets.iter().map(|h| format!("&|| {{ {} }}", self.call_handler(*h, None, false))));
                if f.meets.is_empty() {
                    format!("{ind}lineit_rt::branch::fan_out(&[], &[{branches}]);\n")
                } else {
                    let meets = join(f.meets.iter().map(|m| m.to_string()));
                    format!("{ind}for h in lineit_rt::branch::fan_out(&[{meets}], &[{branches}]) {{ meet(s, h); }}\n")
                }
            }
        }
    }

    /// What holds the locks of what of SLOTS needs one, until the end of
    /// the block it's in (nothing, if none does).
    fn hold(&self, slots: &[SlotId]) -> String {
        let locks = self.prog.locks_of(slots, &self.locked);
        if locks.is_empty() {
            return String::new();
        }
        let l: Vec<String> = locks.iter().map(|s| format!("lineit_rt::slot::lock(&LOCK{s})")).collect();
        format!("let _held = ({},); ", l.join(", "))
    }

    fn block(&self, b: &Block, locals: &[Type], group: Option<usize>, tail: bool, ind: &str) -> String {
        let mut out = String::new();
        let inner = format!("{ind}    ");
        for (i, st) in b.iter().enumerate() {
            let tail = tail && i == b.len() - 1;
            let e = |e: &Expr| self.expr(e, locals);
            match st {
                Stmt::Enter(n) => {
                    let _ = writeln!(out, "{ind}// {} ({})", self.prog.nodes[*n as usize].module, n);
                }
                Stmt::Set(s, x) => {
                    let _ = writeln!(out, "{ind}{{ {}let v = {}; s.s{s}.set(v); }}", self.hold(&[*s]), e(x));
                }
                Stmt::Let(l, x) => {
                    let _ = writeln!(out, "{ind}l{l} = {};", e(x));
                }
                Stmt::Print(Expr::Const(Value::Str(text))) => {
                    let _ = writeln!(out, "{ind}lineit_rt::io::print({});", rust_str(text));
                }
                Stmt::Print(x) => {
                    let _ = writeln!(out, "{ind}lineit_rt::io::print(&{});", e(x));
                }
                Stmt::Do(effect, args) => {
                    let mut lets = String::new();
                    for (i, a) in args.iter().enumerate() {
                        let _ = write!(lets, "let a{i} = {}; ", e(a));
                    }
                    let body = match effect {
                        Effect::ListPush(s) => format!("s.s{s}.with_mut(|l| l.push(a0));"),
                        Effect::ListPop { list, into } => {
                            format!("let v = s.s{list}.with_mut(|l| lineit_rt::list::pop(l)); s.s{into}.set(v);")
                        }
                        Effect::ListSet(s) => format!("s.s{s}.with_mut(|l| lineit_rt::list::set(l, a0, a1));"),
                        Effect::ListInsert(s) => format!("s.s{s}.with_mut(|l| lineit_rt::list::insert(l, a0, a1));"),
                        Effect::ListRemove(s) => format!("s.s{s}.with_mut(|l| lineit_rt::list::remove(l, a0));"),
                        Effect::ListClear(s) => format!("s.s{s}.with_mut(|l| l.clear());"),
                        Effect::MapSet(s) => format!("s.s{s}.with_mut(|m| {{ m.insert(a0, a1); }});"),
                        Effect::MapDelete(s) => format!("s.s{s}.with_mut(|m| {{ m.remove(&a0); }});"),
                        Effect::MapClear(s) => format!("s.s{s}.with_mut(|m| m.clear());"),
                        Effect::ImageSave { ok } => {
                            format!("let v = lineit_rt::image::save(&a0, &a1); s.s{ok}.set(v);")
                        }
                    };
                    let _ = writeln!(out, "{ind}{{ {}{lets}{body} }}", self.hold(&effect.writes()));
                }
                Stmt::If(c, a, b) => {
                    let _ = writeln!(out, "{ind}if {} {{", e(c));
                    out.push_str(&self.block(a, locals, group, tail, &inner));
                    if b.is_empty() {
                        let _ = writeln!(out, "{ind}}}");
                    } else {
                        let _ = writeln!(out, "{ind}}} else {{");
                        out.push_str(&self.block(b, locals, group, tail, &inner));
                        let _ = writeln!(out, "{ind}}}");
                    }
                }
                Stmt::While(c, b) => {
                    let _ = writeln!(out, "{ind}while {} {{", e(c));
                    out.push_str(&self.block(b, locals, group, false, &inner));
                    let _ = writeln!(out, "{ind}}}");
                }
                Stmt::For { local, from, to, body } => {
                    let _ = writeln!(out, "{ind}for i in ({})..({}) {{", e(from), e(to));
                    let _ = writeln!(out, "{inner}l{local} = i;");
                    out.push_str(&self.block(body, locals, group, false, &inner));
                    let _ = writeln!(out, "{ind}}}");
                }
                Stmt::Fire(f) => out.push_str(&self.fire(f, group, tail, ind)),
                // a failed check, said: a compiled program has no report
                Stmt::Check { label, value, expected, .. } => {
                    let label = if label.is_empty() { String::new() } else { format!(" ({label})") };
                    let _ = writeln!(
                        out,
                        "{ind}{{ let v = lineit_rt::value::Typed::to_value(&{}); let e = lineit_rt::value::Typed::to_value(&{}); if !lineit_rt::value::same(&v, &e) {{ lineit_rt::io::flush(); eprintln!(\"check failed{{}}: expected {{}}, got {{}}\", {}, e.text(), v.text()); }} }}",
                        e(value),
                        e(expected),
                        rust_str(&label)
                    );
                }
                Stmt::Stop => {
                    let _ = writeln!(out, "{ind}{{ lineit_rt::io::flush(); std::process::exit(0); }}");
                }
                Stmt::Ext { instance, input, inputs, outputs, fires } => {
                    let outs = join(
                        outputs.iter().map(|(n, s)| format!("({}, {})", rust_str(n), type_value(self.slot_ty(*s)))),
                    );
                    let input = match input {
                        Some(i) => format!("Some({})", rust_str(i)),
                        None => "None".into(),
                    };
                    // what it reads and sets, one step for the other branches
                    let slots: Vec<SlotId> = outputs.iter().map(|(_, s)| *s).collect();
                    let hold = self.hold(&slots);
                    let _ = writeln!(
                        out,
                        "{ind}let run = {{ {hold}let run = ext_tick(s, {instance}, {input}, vec![{}], vec![{outs}]);",
                        self.lua_args(inputs, locals)
                    );
                    for (i, (_, slot)) in outputs.iter().enumerate() {
                        let _ = writeln!(
                            out,
                            "{ind}    if let Some(v) = run.set[{i}].clone() {{ s.s{slot}.set(<{} as lineit_rt::value::Typed>::from_value(v)); }}",
                            self.slot_ty(*slot).rust()
                        );
                    }
                    let _ = writeln!(out, "{ind}    run\n{ind}}};");
                    if !fires.is_empty() {
                        let _ = writeln!(out, "{ind}for (f, values) in run.fire {{");
                        // each fire's own values, first
                        let _ = writeln!(out, "{inner}{{ {hold}for (o, v) in values {{");
                        let _ = writeln!(out, "{inner}    match o {{");
                        for (i, (_, slot)) in outputs.iter().enumerate() {
                            let _ = writeln!(
                                out,
                                "{inner}        {i} => s.s{slot}.set(<{} as lineit_rt::value::Typed>::from_value(v)),",
                                self.slot_ty(*slot).rust()
                            );
                        }
                        let _ = writeln!(out, "{inner}        _ => {{}}");
                        let _ = writeln!(out, "{inner}    }}");
                        let _ = writeln!(out, "{inner}}} }}");
                        let _ = writeln!(out, "{inner}match f.as_str() {{");
                        for (name, f) in fires {
                            let _ = writeln!(out, "{inner}    {} => {{", rust_str(name));
                            out.push_str(&self.fire(f, group, false, &format!("{inner}        ")));
                            let _ = writeln!(out, "{inner}    }}");
                        }
                        let _ = writeln!(out, "{inner}    _ => {{}}");
                        let _ = writeln!(out, "{inner}}}");
                        let _ = writeln!(out, "{ind}}}");
                    }
                }
            }
        }
        out
    }

    fn locals(&self, h: &Handler, ind: &str) -> String {
        h.locals
            .iter()
            .enumerate()
            .map(|(i, t)| format!("{ind}let mut l{i}: {} = Default::default();\n", t.rust()))
            .collect()
    }

    fn slot_type(&self, t: &Type) -> String {
        if !self.shared {
            return format!("lineit_rt::slot::Local<{}>", t.rust());
        }
        match t {
            Type::Int => "lineit_rt::slot::SharedInt".into(),
            Type::Float => "lineit_rt::slot::SharedFloat".into(),
            Type::Bool => "lineit_rt::slot::SharedBool".into(),
            t => format!("lineit_rt::slot::Shared<{}>", t.rust()),
        }
    }
}

fn records(o: &mut String, prog: &Program) {
    for r in &prog.records {
        let n = &r.name;
        let _ = writeln!(o, "#[derive(Clone, Debug, Default, PartialEq)]\npub struct R_{n} {{");
        for (f, t) in &r.fields {
            let _ = writeln!(o, "    pub {}: {},", field(f), t.rust());
        }
        o.push_str("}\n\n");
        let fields = join(r.fields.iter().map(|(f, _)| format!("({}, &self.{})", rust_str(f), field(f))));
        let _ = writeln!(
            o,
            "impl lineit_rt::Show for R_{n} {{\n    fn show(&self, out: &mut String) {{\n        lineit_rt::show::show_record(out, {}, &[{fields}]);\n    }}\n}}\n",
            rust_str(n)
        );
        let tys = join(r.fields.iter().map(|(f, t)| format!("(String::from({}), {})", rust_str(f), type_value(t))));
        let _ = writeln!(
            o,
            "fn rec_{n}() -> std::sync::Arc<lineit_rt::RecordType> {{\n    static R: std::sync::OnceLock<std::sync::Arc<lineit_rt::RecordType>> = std::sync::OnceLock::new();\n    R.get_or_init(|| std::sync::Arc::new(lineit_rt::RecordType {{ name: String::from({}), fields: vec![{tys}] }})).clone()\n}}\n",
            rust_str(n)
        );
        let to = join(r.fields.iter().map(|(f, _)| format!("lineit_rt::value::Typed::to_value(&self.{})", field(f))));
        let from =
            join(r.fields.iter().enumerate().map(|(i, (f, _))| {
                format!("{}: lineit_rt::value::Typed::from_value(r.fields[{i}].clone())", field(f))
            }));
        let _ = writeln!(
            o,
            "impl lineit_rt::value::Typed for R_{n} {{\n    fn to_value(&self) -> lineit_rt::Value {{\n        lineit_rt::Value::Record(std::sync::Arc::new(lineit_rt::Record {{ ty: rec_{n}(), fields: vec![{to}] }}))\n    }}\n    fn from_value(v: lineit_rt::Value) -> Self {{\n        match v {{\n            lineit_rt::Value::Record(r) => R_{n} {{ {from} }},\n            _ => Default::default(),\n        }}\n    }}\n}}\n"
        );
    }
}

/// The Rust source of PROG's `main.rs`.
pub fn generate(prog: &Program) -> String {
    let (group_of, groups) = cycles(prog);
    let shared = prog.has_branches();
    let mut meets = BTreeSet::new();
    meets.extend(prog.starts.meets.iter().copied());
    for h in &prog.handlers {
        for s in &h.body {
            s.fires(&mut |f| meets.extend(f.meets.iter().copied()));
        }
    }
    let locked = if shared { prog.read_and_changed() } else { vec![false; prog.slots.len()] };
    let g = Gen { prog, shared, locked, meets, group_of };
    let st = g.state_ty();
    let mut o = String::new();
    o.push_str("// Made by lineit from a canvas: changes here are lost when it compiles it again.\n");
    o.push_str("#![allow(warnings, clippy::all)]\n\n");
    records(&mut o, prog);
    for (s, l) in g.locked.iter().enumerate() {
        if *l {
            let _ = writeln!(o, "static LOCK{s}: std::sync::Mutex<()> = std::sync::Mutex::new(());");
        }
    }

    // state
    o.push_str("struct State {\n");
    for (i, s) in prog.slots.iter().enumerate() {
        let _ = writeln!(o, "    /// {}\n    s{i}: {},", s.name, g.slot_type(&s.ty));
    }
    o.push_str("    start: std::time::Instant,\n");
    let lua = !prog.exts.is_empty();
    if lua {
        o.push_str("    ext: Vec<std::sync::Mutex<Box<dyn lineit_rt::Ext>>>,\n");
    }
    o.push_str("}\n\n");
    if lua {
        o.push_str("/// Per instance: its module, and its number in it.\nconst EXT_INSTANCES: &[(usize, usize)] = &[");
        let mut count: HashMap<usize, usize> = HashMap::new();
        for i in &prog.ext_instances {
            let n = count.entry(i.module).or_default();
            let _ = write!(o, "({}, {}), ", i.module, n);
            *n += 1;
        }
        o.push_str("];\n\n");
    }
    o.push_str("impl State {\n    fn new() -> State {\n");
    if lua {
        o.push_str("        let mut ext: Vec<Box<dyn lineit_rt::Ext>> = Vec::new();\n");
        for l in &prog.exts {
            let make = match &l.kind {
                ExtKind::Lua { source } => {
                    format!("lineit_rt::lua::LuaModule::load({}, {})", rust_str(&l.path), rust_str(source))
                }
                ExtKind::LuaCode { name, glue } => {
                    format!("lineit_rt::lua::LuaCode::load({}, {})", rust_str(name), rust_str(glue))
                }
                ExtKind::Process { name, commands, args, env, lock, host, load } => format!(
                    "lineit_rt::process::Process::start({}, &[{}], &[{}], &[{}], {}, {}, {})",
                    rust_str(name),
                    join(commands.iter().map(|c| rust_str(c))),
                    join(args.iter().map(|a| rust_str(a))),
                    join(env.iter().map(|(k, v)| format!("({}, {})", rust_str(k), rust_str(v)))),
                    match lock {
                        Some(l) => format!("Some({})", rust_str(l)),
                        None => "None".into(),
                    },
                    rust_str(host),
                    rust_str(load)
                ),
                ExtKind::Native { crate_name, ty, .. } => {
                    format!(
                        "lineit_rt::native::Host::new({}::lineit_natives(), {})",
                        crate_name.replace('-', "_"),
                        rust_str(ty)
                    )
                }
            };
            let _ = writeln!(o, "        ext.push(Box::new({make}.unwrap_or_else(|e| lineit_rt::ext::fail(&e))));");
        }
        for i in &prog.ext_instances {
            let params =
                join(i.params.iter().map(|(k, v)| {
                    format!("({}, lineit_rt::value::Typed::to_value(&{}))", rust_str(k), value(v, &v.ty()))
                }));
            let _ = writeln!(
                o,
                "        ext[{}].instance(&[{params}]).unwrap_or_else(|e| lineit_rt::ext::fail(&e));",
                i.module
            );
        }
    }
    o.push_str("        State {\n");
    for (i, s) in prog.slots.iter().enumerate() {
        let ty = g.slot_type(&s.ty);
        let ctor = ty.split('<').next().unwrap();
        let _ = writeln!(o, "            s{i}: {ctor}::new({}),", value(&s.init, &s.ty));
    }
    o.push_str("            start: std::time::Instant::now(),\n");
    if lua {
        o.push_str("            ext: ext.into_iter().map(std::sync::Mutex::new).collect(),\n");
    }
    o.push_str("        }\n    }\n}\n\n");
    if lua {
        let _ = writeln!(
            o,
            "fn ext_eval(s: &State, inst: usize, output: &str, ty: &lineit_rt::Type, inputs: Vec<(&str, lineit_rt::Value)>) -> lineit_rt::Value {{\n    let (m, i) = EXT_INSTANCES[inst];\n    let mut m = s.ext[m].lock().unwrap_or_else(|e| e.into_inner());\n    let v = m.eval(i, &inputs, &[(output, ty)]);\n    drop(m);\n    for l in lineit_rt::ext::take_printed() {{ lineit_rt::io::print(&l); }}\n    v.unwrap_or_else(|e| lineit_rt::ext::fail(&e)).remove(0)\n}}\n"
        );
        let _ = writeln!(
            o,
            "fn ext_tick(s: &State, inst: usize, input: Option<&str>, inputs: Vec<(&str, lineit_rt::Value)>, outputs: Vec<(&str, lineit_rt::Type)>) -> lineit_rt::ext::Run {{\n    let (m, i) = EXT_INSTANCES[inst];\n    let outs: Vec<(&str, &lineit_rt::Type)> = outputs.iter().map(|(n, t)| (*n, t)).collect();\n    let mut m = s.ext[m].lock().unwrap_or_else(|e| e.into_inner());\n    let run = m.run(i, input, &inputs, &outs);\n    drop(m);\n    for l in lineit_rt::ext::take_printed() {{ lineit_rt::io::print(&l); }}\n    run.unwrap_or_else(|e| lineit_rt::ext::fail(&e))\n}}\n"
        );
    }

    // handlers
    for (i, h) in prog.handlers.iter().enumerate() {
        let _ = writeln!(o, "/// {}", h.name);
        match g.group_of[i] {
            Some(grp) => {
                let _ = writeln!(o, "fn h{i}(s: {st}) {{\n    cycle{grp}(s, {i})\n}}\n");
            }
            None => {
                let _ = writeln!(o, "fn h{i}(s: {st}) {{");
                o.push_str(&g.locals(h, "    "));
                o.push_str(&g.block(&h.body, &h.locals, None, true, "    "));
                o.push_str("}\n\n");
            }
        }
    }
    for (grp, members) in groups.iter().enumerate() {
        let _ = writeln!(
            o,
            "/// Handlers that fire each other: {members:?}\nfn cycle{grp}(s: {st}, mut st: u32) {{\n    'run: loop {{\n        match st {{"
        );
        for &m in members {
            let h = &prog.handlers[m as usize];
            let _ = writeln!(o, "            {m} => {{");
            o.push_str(&g.locals(h, "                "));
            o.push_str(&g.block(&h.body, &h.locals, Some(grp), true, "                "));
            o.push_str("                return;\n            }\n");
        }
        o.push_str("            _ => unreachable!(),\n        }\n    }\n}\n\n");
    }
    if g.shared {
        let _ = writeln!(o, "/// Runs a meet that branches got to.\nfn meet(s: {st}, h: u32) {{\n    match h {{");
        for &m in &g.meets {
            let _ = writeln!(o, "        {m} => {}", g.call_handler(m, None, false));
        }
        o.push_str("        _ => {}\n    }\n}\n\n");
    }

    // run
    let _ = writeln!(o, "fn run(s: {st}) {{");
    o.push_str(&g.fire(&prog.starts, None, false, "    "));
    if !prog.clocks.is_empty() {
        let n = prog.clocks.len();
        let interval = |c: &Clock| format!("f64::max({}, 0.001)", g.expr(&c.interval, &[]));
        let _ = writeln!(o, "    let mut next: [f64; {n}] = [{}];", join(prog.clocks.iter().map(interval)));
        o.push_str(
            "    loop {\n        let now = s.start.elapsed().as_secs_f64();\n        let first = next.iter().cloned().fold(f64::INFINITY, f64::min);\n        if first > now {\n            lineit_rt::io::flush();\n            std::thread::sleep(std::time::Duration::from_secs_f64(first - now));\n        }\n        let now = s.start.elapsed().as_secs_f64();\n",
        );
        for (i, c) in prog.clocks.iter().enumerate() {
            let fire = g.fire(&Fire { targets: vec![c.handler], meets: vec![] }, None, false, "            ");
            let _ = writeln!(
                o,
                "        if next[{i}] <= now {{\n            next[{i}] = f64::max(next[{i}] + {}, now);\n{fire}        }}",
                interval(c)
            );
        }
        o.push_str("    }\n");
    }
    o.push_str("}\n\n");
    let (mutable, borrow) = if g.shared { ("", "&s") } else { ("mut ", "&mut s") };
    let _ = writeln!(
        o,
        "fn main() {{\n    lineit_rt::branch::main_thread(|| {{\n        let {mutable}s = State::new();\n        run({borrow});\n        lineit_rt::io::flush();\n    }});\n}}"
    );
    o
}

#[cfg(test)]
mod tests {
    #[test]
    fn runtime_files_are_all_there() {
        // every file, folders too
        fn files(dir: &std::path::Path, prefix: &str, out: &mut Vec<String>) {
            for e in std::fs::read_dir(dir).unwrap().flatten() {
                let name = format!("{prefix}/{}", e.file_name().to_string_lossy());
                if e.path().is_dir() {
                    files(&e.path(), &name, out);
                } else {
                    out.push(name);
                }
            }
        }
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../lineit-rt/src");
        let mut on_disk = Vec::new();
        files(&dir, "src", &mut on_disk);
        on_disk.sort();
        let mut embedded: Vec<String> =
            super::RT_FILES.iter().map(|(n, _)| n.to_string()).filter(|n| n.starts_with("src/")).collect();
        embedded.sort();
        assert_eq!(on_disk, embedded, "codegen::RT_FILES should list every file of lineit-rt");
    }
}

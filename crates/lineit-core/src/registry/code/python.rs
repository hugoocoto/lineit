//! Code squares in Python, run by the machine's `python3` (or `python`).
//!
//! ```python
//! from lineit import Out, Shared
//!
//! count = 0  # the square's state
//!
//! def collatz(n: int, even: Out["half": int], odd: Out["calls": int, "n": int]):
//!     """Halves even numbers; counts and passes on the odd ones."""
//!     global count
//!     Shared.total += n
//!     if n % 2 == 0:
//!         return even(n // 2)
//!     count += 1
//!     return odd(count, n)
//! ```
//!
//! - The function is the one whose name doesn't start with `_`.
//! - Types are Python's: `int`, `float`, `bool`, `str`, `list[T]`,
//!   `dict[str, T]`, or a record's name. A parameter with none takes what's
//!   wired to it, and its default (`k=2`) is what its field has at first.
//! - What it returns: `-> int` is `result`, `-> tuple[int, str]` is `out1`
//!   and `out2`, `-> Name` with `class Name(NamedTuple)` in the code has
//!   the names of its fields.
//! - A parameter of type `Out[...]` is an exit: `Out[int]` gives
//!   `NAME.value`, `Out["half": int]` gives `half`.
//! - Its state: the variables at the top of the code that aren't in
//!   capitals (those are constants), and those it declares `global`. Each
//!   square runs the code as a module of its own.
//!
//! [`HOST`] runs it (see [`lineit_rt::process::Process`]).

use lineit_rt::Type;
use serde_json::json;
use tree_sitter::Node;

use super::{Function, Language, Param, Role, descendants, exit_values, return_values};
use crate::ir::ExtKind;
use crate::registry::SigCx;

pub struct Python;

impl Language for Python {
    fn name(&self) -> &str {
        "python"
    }

    fn title(&self) -> &str {
        "Python"
    }

    fn extensions(&self) -> Vec<String> {
        vec!["py".into()]
    }

    fn highlights(&self) -> Option<(tree_sitter::Language, &str)> {
        Some((tree_sitter_python::LANGUAGE.into(), tree_sitter_python::HIGHLIGHTS_QUERY))
    }

    fn template(&self) -> &str {
        "def double(x: int) -> int:\n    \"\"\"Twice x.\"\"\"\n    return x * 2\n"
    }

    fn read(&self, source: &str, cx: &dyn SigCx) -> Result<Function, String> {
        read(source, cx)
    }

    fn ext(&self, f: &Function) -> Result<ExtKind, String> {
        let params: Vec<_> = f
            .params
            .iter()
            .map(|p| match &p.role {
                Role::Input(_) => json!({ "name": p.name, "exit": null }),
                Role::Exit(values) => {
                    json!({ "name": p.name, "exit": values.iter().map(|(pin, _)| pin).collect::<Vec<_>>() })
                }
            })
            .collect();
        let load = json!({
            "name": f.name,
            "file": f.origin("py", true),
            "source": f.source,
            "params": params,
            "returns": f.returns.iter().map(|(pin, _)| pin).collect::<Vec<_>>(),
            "shared": f.shared.iter().map(|(g, set)| json!([g, set])).collect::<Vec<_>>(),
            "then": f.ticked() && f.exits().next().is_none(),
            "state": f.state,
        });
        Ok(ExtKind::Process {
            name: f.name.clone(),
            commands: vec!["python3".into(), "python".into()],
            args: vec!["-c".into()],
            env: Vec::new(),
            lock: None,
            host: HOST.into(),
            load: load.to_string(),
        })
    }
}

/// The program `python -c` runs: it reads what to load, then answers
/// requests (see [`lineit_rt::process`]). What the code prints comes with
/// each answer, not mixed with them.
pub const HOST: &str = r#"
import io, json, sys, types, traceback

out = sys.stdout
# what the code prints goes to lineit with each answer
printed = io.StringIO()
sys.stdout = printed

def send(m):
    global printed
    lines = printed.getvalue().splitlines()
    if lines:
        m["printed"] = lines
    printed = io.StringIO()
    sys.stdout = printed
    out.write(json.dumps(m) + "\n")
    out.flush()

load = json.loads(sys.stdin.readline())

class Out:
    """An exit of the square: calling it fires it, with its values."""
    def __class_getitem__(cls, item):
        return cls

class Shared:
    """The program's variables: Shared.NAME is ::NAME."""

lineit = types.ModuleType("lineit")
lineit.Out = Out
lineit.Shared = Shared
sys.modules["lineit"] = lineit

def error(e):
    frames = [f for f in traceback.extract_tb(e.__traceback__) if f.filename == load["file"]]
    at = f"{frames[-1].filename}:{frames[-1].lineno}: " if frames else ""
    return {"error": f"{at}{type(e).__name__}: {e}"}

def plain(v):
    try:
        json.dumps(v)
        return True
    except (TypeError, ValueError):
        return False

def exit_fn(fire, name, pins):
    def fn(*values):
        fire.append([name, dict(zip(pins, values))])
    return fn

try:
    code = compile(load["source"], load["file"], "exec")
    send({})
except Exception as e:
    send(error(e))
    sys.exit(1)

squares = []
for line in sys.stdin:
    req = json.loads(line)
    try:
        op = req["op"]
        if op == "new":
            g = {"__name__": load["name"], "Out": Out, "Shared": Shared}
            exec(code, g)
            squares.append(g)
            send({"sq": len(squares) - 1})
        elif op == "call":
            g = squares[req["sq"]]
            i = req["i"]
            shared = g["Shared"]
            for name, _ in load["shared"]:
                setattr(shared, name, i["::" + name])
            fire = []
            args = [i[p["name"]] if p["exit"] is None else exit_fn(fire, p["name"], p["exit"]) for p in load["params"]]
            r = g[load["name"]](*args)
            got = {}
            pins = load["returns"]
            if len(pins) == 1:
                got[pins[0]] = r
            elif pins:
                got.update(zip(pins, r))
            for name, sets in load["shared"]:
                if sets:
                    got["::" + name] = getattr(shared, name)
            if load["then"]:
                fire.append(["then", {}])
            send({"set": got, "fire": fire})
        elif op == "state":
            g = squares[req["sq"]]
            send({"state": {k: g[k] for k in load["state"] if k in g and plain(g[k])}})
        elif op == "set_state":
            squares[req["sq"]].update(req["state"])
            send({})
    except Exception as e:
        send(error(e))
"#;

/// What a type in Python is: a lineit type, or an exit's values.
enum PyType {
    T(Type),
    Exit(Vec<(Option<String>, Type)>),
}

/// The types in the `[...]` of N.
fn type_params(n: Node) -> Vec<Node> {
    let mut c = n.walk();
    let tp = n.named_children(&mut c).find(|c| c.kind() == "type_parameter");
    let mut v = Vec::new();
    if let Some(tp) = tp {
        let mut c = tp.walk();
        v.extend(tp.named_children(&mut c));
    }
    v
}

fn py_type(n: Node, src: &str, cx: &dyn SigCx) -> Result<PyType, String> {
    let text = |n: Node| src[n.byte_range()].to_string();
    let plain = |n: Node| match py_type(n, src, cx)? {
        PyType::T(t) => Ok(t),
        PyType::Exit(_) => Err(format!("`{}`: an Out is a parameter of its own", text(n))),
    };
    Ok(match n.kind() {
        "type" => {
            let inner = n.named_child(0).ok_or_else(|| format!("`{}` isn't a type", text(n)))?;
            return py_type(inner, src, cx);
        }
        "identifier" => match text(n).as_str() {
            "int" => PyType::T(Type::Int),
            "float" => PyType::T(Type::Float),
            "bool" => PyType::T(Type::Bool),
            "str" => PyType::T(Type::Str),
            "Out" => PyType::Exit(Vec::new()),
            name => {
                PyType::T(Type::Record(cx.record(name).ok_or_else(|| format!("`{name}` isn't a type lineit knows"))?))
            }
        },
        "generic_type" => {
            let head = n.named_child(0).map(text).unwrap_or_default();
            let ps = type_params(n);
            match (head.as_str(), ps.as_slice()) {
                ("list" | "List" | "Sequence", [t]) => PyType::T(Type::List(Box::new(plain(*t)?))),
                ("dict" | "Dict" | "Mapping", [k, v]) => {
                    if !matches!(plain(*k)?, Type::Str) {
                        return Err(format!("`{}`: a dict's keys are str here", text(n)));
                    }
                    PyType::T(Type::Map(Box::new(plain(*v)?)))
                }
                ("Out", values) => {
                    let mut v = Vec::new();
                    for value in values {
                        // `"half": int` names it
                        let named = value.named_child(0).filter(|c| c.kind() == "constrained_type");
                        v.push(match named {
                            Some(c) => {
                                let (Some(name), Some(t)) = (c.named_child(0), c.named_child(1)) else {
                                    return Err(format!("`{}`: it's \"name\": type", text(*value)));
                                };
                                (Some(text(name).trim_matches(['"', '\'']).to_string()), plain(t)?)
                            }
                            None => (None, plain(*value)?),
                        });
                    }
                    PyType::Exit(v)
                }
                _ => return Err(format!("`{}` isn't a type lineit knows", text(n))),
            }
        }
        _ => return Err(format!("`{}` isn't a type lineit knows", text(n))),
    })
}

/// A default value as the text of a field: `2`, `"hi"` without its
/// quotes, `True` as `true`.
fn default_text(n: Node, src: &str) -> Option<String> {
    let t = &src[n.byte_range()];
    Some(match n.kind() {
        "integer" | "float" => t.to_string(),
        "true" => "true".into(),
        "false" => "false".into(),
        "string" => {
            let mut c = n.walk();
            n.named_children(&mut c)
                .find(|c| c.kind() == "string_content")
                .map_or(String::new(), |c| src[c.byte_range()].to_string())
        }
        "unary_operator" => t.replace(' ', ""),
        _ => return None,
    })
}

fn read(source: &str, cx: &dyn SigCx) -> Result<Function, String> {
    let tree = super::parse(source, tree_sitter_python::LANGUAGE.into(), "Python")?;
    let root = tree.root_node();
    let text = |n: Node| &source[n.byte_range()];

    // what's at the top: the function, its helpers, the variables, the
    // named tuples
    let mut functions = Vec::new();
    let mut classes = Vec::new();
    let mut state: Vec<String> = Vec::new();
    let mut c = root.walk();
    for n in root.named_children(&mut c) {
        let n = if n.kind() == "decorated_definition" { n.child_by_field_name("definition").unwrap_or(n) } else { n };
        match n.kind() {
            "function_definition" => {
                if let Some(name) = n.child_by_field_name("name")
                    && !text(name).starts_with('_')
                {
                    functions.push(n);
                }
            }
            "class_definition" => classes.push(n),
            "expression_statement" => {
                let mut c = n.walk();
                for a in n.named_children(&mut c).filter(|a| a.kind() == "assignment") {
                    if let Some(left) = a.child_by_field_name("left").filter(|l| l.kind() == "identifier") {
                        let v = text(left);
                        // in capitals: a constant
                        if v != v.to_uppercase() && !v.starts_with('_') {
                            state.push(v.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let f = match functions.as_slice() {
        [f] => *f,
        [] => return Err("there's no function: `def name(x): ...` (helpers start with `_`)".into()),
        fs => {
            let names: Vec<&str> = fs.iter().filter_map(|f| f.child_by_field_name("name")).map(text).collect();
            return Err(format!(
                "it runs one function, and there are {}: start the helpers' names with `_`",
                names.join(", ")
            ));
        }
    };
    let name = text(f.child_by_field_name("name").ok_or("the function has no name")?).to_string();

    // its doc: the string its body starts with
    let body = f.child_by_field_name("body");
    let doc = body
        .and_then(|b| b.named_child(0))
        .filter(|s| s.kind() == "expression_statement")
        .and_then(|s| s.named_child(0))
        .filter(|s| s.kind() == "string")
        .map(|s| {
            let mut c = s.walk();
            let t: String = s.named_children(&mut c).filter(|c| c.kind() == "string_content").map(text).collect();
            t.split_whitespace().collect::<Vec<_>>().join(" ")
        })
        .unwrap_or_default();

    // its parameters
    let mut params = Vec::new();
    if let Some(ps) = f.child_by_field_name("parameters") {
        let mut c = ps.walk();
        for p in ps.named_children(&mut c) {
            let (pname, ty, default) = match p.kind() {
                "identifier" => (p, None, None),
                "typed_parameter" => {
                    (p.named_child(0).ok_or("a parameter with no name")?, p.child_by_field_name("type"), None)
                }
                "default_parameter" | "typed_default_parameter" => (
                    p.child_by_field_name("name").ok_or("a parameter with no name")?,
                    p.child_by_field_name("type"),
                    p.child_by_field_name("value"),
                ),
                "comment" => continue,
                _ => return Err(format!("`{}`: each parameter is a name (no *args, **kwargs, / or *)", text(p))),
            };
            let pname = text(pname).to_string();
            let role = match ty.map(|t| py_type(t, source, cx)).transpose().map_err(|e| format!("`{pname}`: {e}"))? {
                None => Role::Input(None),
                Some(PyType::T(t)) => Role::Input(Some(t)),
                Some(PyType::Exit(v)) if v.is_empty() => {
                    return Err(format!("`{pname}`: say what it gives, like `Out[int]` or `Out[\"half\": int]`"));
                }
                Some(PyType::Exit(v)) => Role::Exit(exit_values(&pname, v)),
            };
            let default = default.and_then(|d| default_text(d, source));
            params.push(Param { name: pname, role, default });
        }
    }

    // what it returns
    let returns = match f.child_by_field_name("return_type") {
        None => Vec::new(),
        Some(r) if text(r) == "None" => Vec::new(),
        Some(r) => {
            let inner = r.named_child(0).unwrap_or(r);
            let head = if inner.kind() == "generic_type" { inner.named_child(0).map(text) } else { None };
            if matches!(head, Some("tuple" | "Tuple")) {
                let mut c = inner.walk();
                let tp = inner.named_children(&mut c).find(|c| c.kind() == "type_parameter");
                let mut v = Vec::new();
                if let Some(tp) = tp {
                    let mut c = tp.walk();
                    for t in tp.named_children(&mut c) {
                        match py_type(t, source, cx)? {
                            PyType::T(t) => v.push((None, t)),
                            PyType::Exit(_) => return Err("it returns values, not exits".into()),
                        }
                    }
                }
                return_values(v)
            } else if let Some(class) = classes
                .iter()
                .find(|c| inner.kind() == "identifier" && c.child_by_field_name("name").map(text) == Some(text(inner)))
            {
                // a NamedTuple: its fields
                let mut v = Vec::new();
                for d in class.child_by_field_name("body").map(descendants).unwrap_or_default() {
                    if d.kind() == "assignment"
                        && let (Some(l), Some(t)) = (d.child_by_field_name("left"), d.child_by_field_name("type"))
                    {
                        match py_type(t, source, cx)? {
                            PyType::T(t) => v.push((Some(text(l).to_string()), t)),
                            PyType::Exit(_) => return Err("it returns values, not exits".into()),
                        }
                    }
                }
                return_values(v)
            } else {
                match py_type(r, source, cx)? {
                    PyType::T(t) => return_values(vec![(None, t)]),
                    PyType::Exit(_) => return Err("it returns values, not exits".into()),
                }
            }
        }
    };

    // a value returned with no type: say so, rather than lose it
    let has_exits = params.iter().any(|p| matches!(p.role, Role::Exit(_)));
    if f.child_by_field_name("return_type").is_none()
        && !has_exits
        && let Some(r) = body.and_then(|b| {
            super::return_with_value(b, "return_statement", &["function_definition", "lambda", "class_definition"])
        })
    {
        return Err(format!(
            "line {}: `{name}` returns a value with no type: say it, like `def {name}(...) -> int:`",
            r.start_position().row + 1
        ));
    }

    // what its body uses
    let nodes = body.map(descendants).unwrap_or_default();
    let mut shared: Vec<(String, bool)> = Vec::new();
    for n in &nodes {
        match n.kind() {
            "global_statement" => {
                let mut c = n.walk();
                for g in n.named_children(&mut c) {
                    let g = text(g).to_string();
                    if !state.contains(&g) {
                        state.push(g);
                    }
                }
            }
            "call" => {
                if let Some(callee) = n.child_by_field_name("function").filter(|c| c.kind() == "identifier") {
                    let callee = text(callee);
                    if params.iter().any(|p| p.name == callee && matches!(p.role, Role::Input(None))) {
                        return Err(format!(
                            "`{name}` calls `{callee}`, which makes it an exit: say what it gives, like `{callee}: Out[int]`"
                        ));
                    }
                }
            }
            "attribute" => {
                let obj = n.child_by_field_name("object");
                let attr = n.child_by_field_name("attribute");
                if let (Some(o), Some(a)) = (obj, attr)
                    && text(o) == "Shared"
                {
                    let set = n.parent().is_some_and(|p| {
                        matches!(p.kind(), "assignment" | "augmented_assignment")
                            && p.child_by_field_name("left") == Some(*n)
                    });
                    let g = text(a).to_string();
                    match shared.iter_mut().find(|(s, _)| *s == g) {
                        Some(s) => s.1 |= set,
                        None => shared.push((g, set)),
                    }
                }
            }
            _ => {}
        }
    }

    Ok(Function {
        name,
        doc,
        params,
        returns,
        state,
        shared,
        shared_types: Vec::new(),
        source: source.to_string(),
        file: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lineit_rt::ext::{Ext, Run};
    use lineit_rt::{RecordType, Value};
    use std::sync::Arc;

    struct Cx;
    impl SigCx for Cx {
        fn var(&self, name: &str) -> Option<Type> {
            (name == "::total").then_some(Type::Int)
        }
        fn record(&self, _: &str) -> Option<Arc<RecordType>> {
            None
        }
        fn read(&self, path: &str) -> Result<String, String> {
            Err(format!("{path}: no files here"))
        }
    }

    const COLLATZ: &str = r#"from lineit import Out, Shared

count = 0
LIMIT = 3

def collatz(n: int, even: Out["half": int], odd: Out["calls": int, "n": int]):
    """Halves even numbers;
    counts the odd ones."""
    global count
    Shared.total += n
    if n % 2 == 0:
        return even(n // 2)
    count += 1
    return odd(count, n)
"#;

    #[test]
    fn reads() {
        let f = read(COLLATZ, &Cx).unwrap();
        assert_eq!(f.name, "collatz");
        assert_eq!(f.doc, "Halves even numbers; counts the odd ones.");
        assert_eq!(f.params[1].role, Role::Exit(vec![("half".into(), Type::Int)]));
        assert_eq!(f.state, ["count"]);
        assert_eq!(f.shared, [("total".to_string(), true)]);
        let f = read(
            "from typing import NamedTuple\nclass R(NamedTuple):\n    sq: int\n    text: str\ndef _h(x): return x\ndef f(x, k: int = -2, s='hi') -> R:\n    return R(x * x, 'ok')\n",
            &Cx,
        )
        .unwrap();
        assert_eq!(f.params[0].role, Role::Input(None));
        assert_eq!(f.params[1].default.as_deref(), Some("-2"));
        assert_eq!(f.params[2].default.as_deref(), Some("hi"));
        assert_eq!(f.returns, [("sq".to_string(), Type::Int), ("text".to_string(), Type::Str)]);
        assert!(!f.ticked());
        let f = read("def f(xs: list[float], m: dict[str, int]) -> tuple[int, str]: pass", &Cx).unwrap();
        assert_eq!(f.returns[1], ("out2".to_string(), Type::Str));
        assert_eq!(f.params[1].role, Role::Input(Some(Type::Map(Box::new(Type::Int)))));
    }

    #[test]
    fn errors() {
        let err = |s: &str| read(s, &Cx).unwrap_err();
        assert!(err("x = 1").contains("there's no function"));
        assert!(err("def f(): pass\ndef g(): pass").contains("there are f, g"));
        assert!(err("def f(out): out(1)").contains("which makes it an exit"));
        assert!(err("def f(e: Out): pass").contains("say what it gives"));
        assert!(err("def f(x: foo): pass").contains("`foo` isn't a type"));
        assert!(err("def f(*a): pass").contains("no *args"));
        assert!(err("def f(:").contains("line 1"));
        assert!(err("def f(x):\n    return x * 2\n").contains("line 2: `f` returns a value with no type"));
        assert!(
            read("def f(xs):\n    def key(x):\n        return -x\n    xs.sort(key=key)\n    return\n", &Cx).is_ok()
        );
    }

    fn start(f: &Function) -> Option<lineit_rt::process::Process> {
        let ExtKind::Process { name, commands, args, host, load, .. } = Python.ext(f).unwrap() else { panic!() };
        let commands: Vec<&str> = commands.iter().map(String::as_str).collect();
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        match lineit_rt::process::Process::start(&name, &commands, &args, &[], None, &host, &load) {
            Ok(p) => Some(p),
            // no Python here: what can be checked without it is
            Err(e) if e.contains("couldn't run") => None,
            Err(e) => panic!("{e}"),
        }
    }

    #[test]
    fn runs() {
        let f = read(COLLATZ, &Cx).unwrap();
        let Some(mut m) = start(&f) else { return };
        let (a, b) = (m.instance(&[]).unwrap(), m.instance(&[]).unwrap());
        let outs = [("half", &Type::Int), ("calls", &Type::Int), ("n", &Type::Int), ("::total", &Type::Int)];
        let call = |m: &mut lineit_rt::process::Process, sq, n| {
            m.run(sq, Some("tick"), &[("n", Value::Int(n)), ("::total", Value::Int(100))], &outs).unwrap()
        };
        assert_eq!(
            call(&mut m, a, 4),
            Run {
                set: vec![None, None, None, Some(Value::Int(104))],
                fire: vec![("even".into(), vec![(0, Value::Int(2))])]
            }
        );
        call(&mut m, a, 3);
        assert_eq!(call(&mut m, a, 5).fire, [("odd".into(), vec![(1, Value::Int(2)), (2, Value::Int(5))])]);
        assert_eq!(m.state(a).unwrap(), [("count".to_string(), Value::Int(2))]);
        m.set_state(b, &[("count", Value::Int(41))]).unwrap();
        assert_eq!(call(&mut m, b, 1).fire[0].1[0], (1, Value::Int(42)));

        // a command that ends without answering (Windows' `python3` that
        // offers to install it): the next one runs it
        let ExtKind::Process { name, args, host, load, .. } = Python.ext(&f).unwrap() else { panic!() };
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        if cfg!(unix) {
            let mut p = lineit_rt::process::Process::start(&name, &["true", "python3"], &args, &[], None, &host, &load)
                .unwrap();
            assert!(p.instance(&[]).is_ok());
            let e = lineit_rt::process::Process::start(&name, &["true"], &args, &[], None, &host, &load).err().unwrap();
            assert!(e.contains("true: it ended without answering"), "{e}");
        }
        // code Python doesn't take: its error, not another command's
        let mut bad = read("def f(x) -> int:\n    return x\n", &Cx).unwrap();
        bad.source = "def f(x) -> int:\n    return x +\n".into();
        let ExtKind::Process { name, args, host, load, .. } = Python.ext(&bad).unwrap() else { panic!() };
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let e = lineit_rt::process::Process::start(&name, &["python3", "python"], &args, &[], None, &host, &load)
            .err()
            .unwrap();
        assert!(e.contains("SyntaxError"), "{e}");

        // errors are on the code's lines
        let f = read("def f(x) -> int:\n    y = 1\n    return x + None\n", &Cx).unwrap();
        let mut m = start(&f).unwrap();
        let sq = m.instance(&[]).unwrap();
        let e = m.eval(sq, &[("x", Value::Int(1))], &[("result", &Type::Int)]).unwrap_err();
        assert!(e.starts_with("f.py:3: TypeError"), "{e}");
    }
}

//! Code squares in Lua.
//!
//! ```lua
//! count = 0  -- a global: the square's state
//!
//! --- Halves even numbers; counts and passes on the odd ones.
//! ---@param n integer
//! ---@param even fun(half: integer)
//! ---@param odd fun(calls: integer, n: integer)
//! function collatz(n, even, odd)
//!     Shared.total = Shared.total + n
//!     if n % 2 == 0 then
//!         return even(n // 2)
//!     end
//!     count = count + 1
//!     return odd(count, n)
//! end
//! ```
//!
//! - The function is the one that isn't `local`.
//! - Types are LuaLS annotations (`integer`, `number`, `string`, `boolean`,
//!   `T[]`, `table<string, T>`, or lineit's: `int`, `list<str>`, a record).
//!   A parameter with none (or `any`) takes what's wired to it; what it
//!   returns needs a `---@return TYPE NAME` each.
//! - A parameter of type `fun(name: type, ...)` is an exit.
//! - Its globals (what's set at the top of the code without `local`, or in
//!   the function without being declared) are the square's state: each
//!   square runs the code in an environment of its own.
//!
//! The glue around the code ([`glue`]) runs it, and gives and takes values
//! by pin name ([`lineit_rt::lua::LuaCode`]).

use lineit_rt::Type;
use tree_sitter::Node;

use super::{Function, Language, Param, Role, descendants, exit_values, quoted, return_values, shared_pin};
use crate::ir::ExtKind;
use crate::registry::SigCx;

pub struct Lua;

impl Language for Lua {
    fn name(&self) -> &str {
        "lua"
    }

    fn title(&self) -> &str {
        "Lua"
    }

    fn extensions(&self) -> Vec<String> {
        vec!["lua".into()]
    }

    fn highlights(&self) -> Option<(tree_sitter::Language, &str)> {
        Some((tree_sitter_lua::LANGUAGE.into(), tree_sitter_lua::HIGHLIGHTS_QUERY))
    }

    fn template(&self) -> &str {
        "--- Twice x.\n---@param x integer\n---@return integer\nfunction double(x)\n    return x * 2\nend\n"
    }

    fn read(&self, source: &str, cx: &dyn SigCx) -> Result<Function, String> {
        read(source, cx)
    }

    fn ext(&self, f: &Function) -> Result<ExtKind, String> {
        Ok(ExtKind::LuaCode { name: f.origin("lua", false), glue: glue(f) })
    }
}

/// The words of S, split by spaces, but not inside `<...>` or `(...)`.
fn words(s: &str) -> Vec<&str> {
    split_top(s, |c| c.is_whitespace())
}

/// S split where IS_SEP is true outside of brackets, without empty parts.
fn split_top(s: &str, is_sep: impl Fn(char) -> bool) -> Vec<&str> {
    let (mut v, mut depth, mut start) = (Vec::new(), 0i32, None);
    for (i, c) in s.char_indices() {
        match c {
            '<' | '(' => depth += 1,
            '>' | ')' => depth -= 1,
            _ => {}
        }
        if is_sep(c) && depth <= 0 {
            if let Some(b) = start.take() {
                v.push(s[b..i].trim());
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(b) = start {
        v.push(s[b..].trim());
    }
    v.retain(|w| !w.is_empty());
    v
}

/// A LuaLS type (or one of lineit's) as a lineit type.
fn lua_type(s: &str, cx: &dyn SigCx) -> Result<Type, String> {
    let s = s.trim();
    let inner =
        |prefix: &str| s.strip_prefix(prefix).and_then(|r| r.strip_prefix('<')).and_then(|r| r.strip_suffix('>'));
    Ok(match s {
        "integer" | "int" => Type::Int,
        "number" | "float" => Type::Float,
        "boolean" | "bool" => Type::Bool,
        "string" | "str" => Type::Str,
        _ => {
            if let Some(e) = s.strip_suffix("[]") {
                Type::List(Box::new(lua_type(e, cx)?))
            } else if let Some(e) = inner("list") {
                Type::List(Box::new(lua_type(e, cx)?))
            } else if let Some(e) = inner("map") {
                Type::Map(Box::new(lua_type(e, cx)?))
            } else if let Some(kv) = inner("table") {
                match kv.split_once(',') {
                    Some((k, v)) if k.trim() == "string" => Type::Map(Box::new(lua_type(v, cx)?)),
                    _ => return Err(format!("`{s}`: a table's keys are strings here (table<string, T>)")),
                }
            } else {
                Type::Record(cx.record(s).ok_or_else(|| format!("`{s}` isn't a type lineit knows"))?)
            }
        }
    })
}

/// The values of exit NAME, from its type `fun(half: integer, ...)`.
fn exit_type(name: &str, ty: &str, cx: &dyn SigCx) -> Result<Vec<(String, Type)>, String> {
    let inner = ty
        .strip_prefix("fun(")
        .and_then(|r| r.rfind(')').map(|e| &r[..e]))
        .ok_or_else(|| format!("`{ty}`: an exit's type is `fun(name: type, ...)`"))?;
    let mut values = Vec::new();
    for part in split_top(inner, |c| c == ',') {
        let (n, t) = match part.split_once(':') {
            Some((n, t)) => (Some(n.trim().to_string()), t),
            None => (None, part),
        };
        let t = lua_type(t, cx).map_err(|e| format!("`{name}`: {e} (each value of an exit is `name: type`)"))?;
        values.push((n, t));
    }
    Ok(exit_values(name, values))
}

/// The identifiers a `variable_list` (what's on the left of an `=`) sets.
fn set_names<'a>(list: Node, src: &'a str) -> Vec<&'a str> {
    let mut c = list.walk();
    list.children(&mut c).filter(|n| n.kind() == "identifier").map(|n| &src[n.byte_range()]).collect()
}

fn read(source: &str, cx: &dyn SigCx) -> Result<Function, String> {
    let tree = super::parse(source, tree_sitter_lua::LANGUAGE.into(), "Lua")?;
    let root = tree.root_node();
    let text = |n: Node| &source[n.byte_range()];

    // what's at the top: the function, its helpers, the globals and locals
    let mut top = Vec::new();
    let mut c = root.walk();
    for (i, n) in root.children(&mut c).enumerate() {
        top.push((n, root.field_name_for_child(i as u32) == Some("local_declaration")));
    }
    let mut globals: Vec<String> = Vec::new();
    let mut top_locals: Vec<String> = Vec::new();
    let mut functions = Vec::new();
    for (k, (n, local)) in top.iter().enumerate() {
        match (n.kind(), local) {
            ("function_declaration", false) => functions.push(k),
            ("function_declaration", true) => {
                if let Some(name) = n.child_by_field_name("name") {
                    top_locals.push(text(name).into());
                }
            }
            ("variable_declaration", _) => {
                for list in descendants(*n).into_iter().filter(|d| d.kind() == "variable_list") {
                    top_locals.extend(set_names(list, source).into_iter().map(String::from));
                }
            }
            ("assignment_statement", _) => {
                for list in descendants(*n).into_iter().filter(|d| d.kind() == "variable_list") {
                    globals.extend(set_names(list, source).into_iter().map(String::from));
                }
            }
            _ => {}
        }
    }
    let at = match functions.as_slice() {
        [at] => *at,
        [] if top_locals.is_empty() => return Err("there's no function: `function name(x) ... end`".into()),
        [] => return Err("its function isn't `local`: the square runs the one that isn't".into()),
        _ => {
            let names: Vec<&str> =
                functions.iter().filter_map(|k| top[*k].0.child_by_field_name("name")).map(text).collect();
            return Err(format!(
                "it runs one function, and {} aren't `local`: make the helpers `local function`",
                names.join(", ")
            ));
        }
    };
    let f = top[at].0;
    let name_node = f.child_by_field_name("name").ok_or("the function has no name")?;
    if name_node.kind() != "identifier" {
        return Err(format!("`{}`: the function's name is a plain name, like `double`", text(name_node)));
    }
    let name = text(name_node).to_string();

    // its comments: the doc and the annotations
    let mut notes = Vec::new();
    for (n, _) in top[..at].iter().rev() {
        if n.kind() != "comment" {
            break;
        }
        notes.push(text(*n));
    }
    notes.reverse();
    let mut doc = Vec::new();
    let mut types: Vec<(String, String)> = Vec::new();
    let mut returns = Vec::new();
    for note in notes {
        let Some(line) = note.strip_prefix("---") else { continue };
        let Some(a) = line.strip_prefix('@') else {
            doc.push(line.trim());
            continue;
        };
        let w = words(a);
        match w.first().copied() {
            Some("param") => match (w.get(1), w.get(2)) {
                (Some(n), Some(t)) => types.push((n.trim_end_matches('?').to_string(), t.to_string())),
                _ => return Err(format!("`{note}`: it's ---@param NAME TYPE")),
            },
            Some("return") => {
                let t = w.get(1).ok_or_else(|| format!("`{note}`: it's ---@return TYPE NAME"))?;
                let n = w.get(2).filter(|n| !n.starts_with('#')).map(|n| n.to_string());
                returns.push((n, lua_type(t, cx).map_err(|e| format!("`{note}`: {e}"))?));
            }
            _ => {}
        }
    }

    // its parameters
    let mut params = Vec::new();
    if let Some(p) = f.child_by_field_name("parameters") {
        let mut c = p.walk();
        for n in p.named_children(&mut c) {
            match n.kind() {
                "identifier" => params.push(text(n).to_string()),
                "comment" => {}
                _ => return Err(format!("`{}`: each parameter is a name (no `...`)", text(n))),
            }
        }
    }
    for (n, _) in &types {
        if !params.contains(n) {
            return Err(format!("`---@param {n}`: `{name}` has no parameter `{n}`"));
        }
    }

    // what its body does: what it calls, sets and declares
    let body = f.child_by_field_name("body");
    let nodes = body.map(descendants).unwrap_or_default();
    let mut called: Vec<&str> = Vec::new();
    let mut declared: Vec<&str> = Vec::new();
    let mut assigned: Vec<&str> = Vec::new();
    let mut shared: Vec<(String, bool)> = Vec::new();
    for n in &nodes {
        match n.kind() {
            "function_call" => {
                if let Some(c) = n.child_by_field_name("name").filter(|c| c.kind() == "identifier") {
                    called.push(text(c));
                }
            }
            "variable_declaration" => {
                for list in descendants(*n).into_iter().filter(|d| d.kind() == "variable_list") {
                    declared.extend(set_names(list, source));
                }
                if let Some(fname) = n.child_by_field_name("name") {
                    declared.push(text(fname));
                }
            }
            "function_declaration" | "for_numeric_clause" | "for_generic_clause" | "parameters" => {
                for d in descendants(*n) {
                    if d.kind() == "identifier"
                        && d.parent()
                            .is_some_and(|p| p == *n || p.kind() == "variable_list" || p.kind() == "parameters")
                    {
                        declared.push(text(d));
                    }
                }
            }
            "assignment_statement" => {
                let mut c = n.walk();
                for list in n.children(&mut c).filter(|d| d.kind() == "variable_list") {
                    // not `local x = ...`: that's a declaration
                    if n.parent().is_some_and(|p| p.kind() != "variable_declaration") {
                        assigned.extend(set_names(list, source));
                    }
                }
            }
            "dot_index_expression" => {
                let table = n.child_by_field_name("table");
                let field = n.child_by_field_name("field");
                if let (Some(t), Some(fld)) = (table, field)
                    && text(t) == "Shared"
                {
                    let set = n.parent().is_some_and(|p| p.kind() == "variable_list");
                    let g = text(fld).to_string();
                    match shared.iter_mut().find(|(s, _)| *s == g) {
                        Some(s) => s.1 |= set,
                        None => shared.push((g, set)),
                    }
                }
            }
            _ => {}
        }
    }
    // its globals: those at the top, and those it sets without declaring
    let mut state = globals;
    for a in assigned {
        if params.iter().any(|p| p == a) || declared.contains(&a) || state.iter().any(|s| s == a) {
            continue;
        }
        if top_locals.iter().any(|l| l == a) {
            return Err(format!(
                "`{name}` changes `{a}`, which is `local` at the top: drop the `local` to make it the square's state, which lineit can see"
            ));
        }
        state.push(a.to_string());
    }
    state.retain(|s| s != "Shared");

    // a value returned with no type: say so, rather than lose it
    let has_exits = types.iter().any(|(_, t)| t.starts_with("fun("));
    if returns.is_empty()
        && !has_exits
        && let Some(r) = body.and_then(|b| {
            super::return_with_value(b, "return_statement", &["function_declaration", "function_definition"])
        })
    {
        return Err(format!(
            "line {}: `{name}` returns a value with no type: say it above the function, like `---@return integer result`",
            r.start_position().row + 1
        ));
    }

    let mut ps = Vec::new();
    for p in &params {
        let ty = types.iter().find(|(n, _)| n == p).map(|(_, t)| t.as_str());
        let role = match ty {
            Some(t) if t.starts_with("fun(") => Role::Exit(exit_type(p, t, cx)?),
            Some("any") | None if called.contains(&p.as_str()) => {
                return Err(format!(
                    "`{name}` calls `{p}`, which makes it an exit: say what it gives it, like `---@param {p} fun(value: integer)`"
                ));
            }
            Some("any") | None => Role::Input(None),
            Some(t) => Role::Input(Some(lua_type(t, cx).map_err(|e| format!("`{p}`: {e}"))?)),
        };
        ps.push(Param { name: p.clone(), role, default: None });
    }

    Ok(Function {
        name,
        doc: doc.join(" "),
        params: ps,
        returns: return_values(returns),
        state,
        shared,
        shared_types: Vec::new(),
        source: source.to_string(),
        file: None,
    })
}

/// The Lua that runs F for lineit (see [`lineit_rt::lua::LuaCode`]): the
/// code in a function of its environment (on its first line, so the lines
/// of errors are the code's), and the table that calls it.
pub fn glue(f: &Function) -> String {
    let mut args = Vec::new();
    for p in &f.params {
        match &p.role {
            Role::Input(_) => args.push(format!("i[{}]", quoted(&p.name))),
            Role::Exit(values) => {
                let set: Vec<String> = values
                    .iter()
                    .enumerate()
                    .map(|(k, (pin, _))| format!("[{}] = v[{}]", quoted(pin), k + 1))
                    .collect();
                args.push(format!(
                    "function(...) local v = table.pack(...); fire[#fire + 1] = {{ {}, {{ {} }} }} end",
                    quoted(&p.name),
                    set.join(", ")
                ));
            }
        }
    }
    let mut call = String::new();
    for (g, _) in &f.shared {
        call.push_str(&format!("    shared[{}] = i[{}]\n", quoted(g), quoted(&shared_pin(g))));
    }
    call.push_str(&format!(
        "    local set, fire = {{}}, {{}}\n    local r = table.pack(rawget(env, {})({}))\n",
        quoted(&f.name),
        args.join(", ")
    ));
    for (k, (pin, _)) in f.returns.iter().enumerate() {
        call.push_str(&format!("    set[{}] = r[{}]\n", quoted(pin), k + 1));
    }
    for (g, _) in f.shared.iter().filter(|(_, set)| *set) {
        call.push_str(&format!("    set[{}] = shared[{}]\n", quoted(&shared_pin(g)), quoted(g)));
    }
    if f.ticked() && f.exits().next().is_none() {
        call.push_str("    fire[#fire + 1] = { \"then\", {} }\n");
    }
    format!(
        "local function __code(_ENV) {}\nend
local M = {{}}
function M.new()
    local env = setmetatable({{ Shared = {{}} }}, {{ __index = _G }})
    __code(env)
    return env
end
function M.call(env, i)
    local shared = rawget(env, \"Shared\")
{call}    return set, fire
end
function M.state(env)
    local t = {{}}
    for k, v in pairs(env) do
        if k ~= \"Shared\" and type(v) ~= \"function\" then t[k] = v end
    end
    return t
end
function M.set_state(env, t)
    for k, v in pairs(t) do rawset(env, k, v) end
end
return M
",
        f.source
    )
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

    fn err(src: &str) -> String {
        read(src, &Cx).unwrap_err()
    }

    const COLLATZ: &str = "count = 0

--- Halves even numbers; counts the odd ones.
---@param n integer
---@param even fun(half: integer)
---@param odd fun(calls: integer, n: integer)
function collatz(n, even, odd)
    Shared.total = Shared.total + n
    if n % 2 == 0 then
        return even(n // 2)
    end
    count = count + 1
    return odd(count, n)
end
";

    #[test]
    fn reads() {
        let f = read(COLLATZ, &Cx).unwrap();
        assert_eq!(f.name, "collatz");
        assert_eq!(f.doc, "Halves even numbers; counts the odd ones.");
        assert_eq!(f.params[0].role, Role::Input(Some(Type::Int)));
        assert_eq!(f.params[1].role, Role::Exit(vec![("half".into(), Type::Int)]));
        assert_eq!(f.state, ["count"]);
        assert_eq!(f.shared, [("total".to_string(), true)]);
        assert!(f.ticked());

        let f = read(
            "local K <const> = 2\nlocal function h(x) return x * K end\n---@return integer\nfunction f(x, xs) local y = h(x) return y end",
            &Cx,
        )
        .unwrap();
        assert_eq!(f.params[0].role, Role::Input(None));
        assert!(f.state.is_empty() && !f.ticked());
        assert_eq!(f.returns, [("result".to_string(), Type::Int)]);
    }

    #[test]
    fn errors() {
        assert!(err("x = 1").contains("there's no function"));
        assert!(err("local function f() end").contains("isn't `local`"));
        assert!(err("function f() end\nfunction g() end").contains("f, g aren't `local`"));
        assert!(err("---@param y integer\nfunction f() end").contains("no parameter `y`"));
        assert!(err("function f(out) out(1) end").contains("which makes it an exit"));
        assert!(err("local n = 0\nfunction f() n = n + 1 end").contains("drop the `local`"));
        assert!(err("---@param x foo\nfunction f(x) end").contains("`foo` isn't a type"));
        assert!(err("function f(").contains("line 1"));
        assert!(err("function f(x)\n  return x * 2\nend").contains("line 2: `f` returns a value with no type"));
        // its helpers' returns are theirs; an exit's call can be returned
        assert!(read("function f(xs) table.sort(xs, function(a, b) return a < b end) end", &Cx).is_ok());
        assert!(read("---@param e fun(v: integer)\nfunction f(e) return e(1) end", &Cx).is_ok());
    }

    #[test]
    fn runs() {
        let f = read(COLLATZ, &Cx).unwrap();
        let mut m = lineit_rt::lua::LuaCode::load("collatz", &glue(&f)).unwrap();
        let (a, b) = (m.instance(&[]).unwrap(), m.instance(&[]).unwrap());
        let outs = [("half", &Type::Int), ("calls", &Type::Int), ("n", &Type::Int), ("::total", &Type::Int)];
        let call = |m: &mut lineit_rt::lua::LuaCode, sq, n| {
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
        let r = call(&mut m, a, 5);
        assert_eq!(r.fire, [("odd".into(), vec![(1, Value::Int(2)), (2, Value::Int(5))])]);
        // each square has its own state, which can be seen and set
        assert_eq!(m.state(a).unwrap(), [("count".to_string(), Value::Int(2))]);
        assert_eq!(m.state(b).unwrap(), [("count".to_string(), Value::Int(0))]);
        m.set_state(b, &[("count", Value::Int(41))]).unwrap();
        assert_eq!(call(&mut m, b, 1).fire[0].1[0], (1, Value::Int(42)));

        // an exit called in a loop: each fire with its value
        let f = read("---@param xs integer[]\n---@param item fun(x: integer)\nfunction each(xs, item)\n  for _, x in ipairs(xs) do item(x) end\nend", &Cx).unwrap();
        let mut m = lineit_rt::lua::LuaCode::load("each", &glue(&f)).unwrap();
        let sq = m.instance(&[]).unwrap();
        let xs = Value::List(Arc::new(vec![Value::Int(7), Value::Int(8)]));
        let r = m.run(sq, Some("tick"), &[("xs", xs)], &[("x", &Type::Int)]).unwrap();
        assert_eq!(r.fire, [("item".into(), vec![(0, Value::Int(7))]), ("item".into(), vec![(0, Value::Int(8))])]);

        // errors are on the code's lines
        let f = read("---@return integer\nfunction f()\n  return nil + 1\nend", &Cx).unwrap();
        let mut m = lineit_rt::lua::LuaCode::load("f", &glue(&f)).unwrap();
        let sq = m.instance(&[]).unwrap();
        let e = m.eval(sq, &[], &[("result", &Type::Int)]).unwrap_err();
        assert!(e.contains("f:3:"), "{e}");
    }
}

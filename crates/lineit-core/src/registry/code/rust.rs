//! Code squares in Rust, built with cargo as a Rust module (see
//! [`crate::native`]): the editor loads it, a compiled program links it.
//!
//! ```rust,ignore
//! #[derive(Default)]
//! pub struct Collatz {
//!     count: i64, // the square's state
//! }
//!
//! pub struct Odd {
//!     calls: i64,
//!     n: i64,
//! }
//!
//! impl Collatz {
//!     /// Halves even numbers; counts and passes on the odd ones.
//!     pub fn collatz(&mut self, n: i64, shared: &mut Shared, mut even: impl FnMut(i64), mut odd: impl FnMut(Odd)) {
//!         shared.total += n;
//!         if n % 2 == 0 {
//!             return even(n / 2);
//!         }
//!         self.count += 1;
//!         odd(Odd { calls: self.count, n })
//!     }
//! }
//! ```
//!
//! - The function is the one `pub fn`. A method with `&mut self` keeps
//!   its struct (which is `Default`) as the square's state: lineit sees
//!   and sets its fields.
//! - Types: `i64`, `f64`, `bool`, `String` (`&str` to take one), `Vec<T>`
//!   (`&[T]`), `BTreeMap<String, T>`, and lineit's records by their name
//!   (`point`: the glue makes the struct, with `pub` fields).
//! - What it returns: one value is `result`, a tuple `out1`, `out2`..., a
//!   struct of the code the names of its fields.
//! - A parameter `impl FnMut(T)` (or `&mut dyn FnMut(T)`) is an exit: `T`
//!   a struct of the code gives its fields' names, other types give
//!   `NAME.value`, or `NAME.1`, `NAME.2`... for several.
//! - A parameter `&Shared` (or `&mut Shared`, to set them) has the
//!   variables `::NAME` it uses as fields: `shared.total`.
//!
//! The code goes in a crate of its own as it is, in a module with the glue
//! after it ([`glue`]), so the lines of cargo's errors are the code's.

use std::path::PathBuf;

use lineit_rt::Type;
use tree_sitter::Node;

use super::{Function, Language, Param, Role, descendants, exit_values, quoted, return_values, shared_pin};
use crate::ir::ExtKind;
use crate::registry::SigCx;

pub struct Rust;

/// The names of the crates of code squares start with this.
pub const CRATE_PREFIX: &str = "lineit-code-";

/// How the glue gives a parameter its value.
#[derive(Debug, Clone, PartialEq)]
enum Pass {
    Value,
    /// `&T` of what it's given (`&str` from a `String`...).
    Ref,
    /// A closure: `impl Fn`, `&mut dyn FnMut` or `&dyn Fn`.
    /// Its values come as ARGS (their Rust types), or as one struct with
    /// these FIELDS.
    Exit { how: &'static str, args: Vec<String>, fields: Option<Vec<String>> },
    Shared { set: bool },
}

impl Language for Rust {
    fn name(&self) -> &str {
        "rust"
    }

    fn title(&self) -> &str {
        "Rust"
    }

    fn extensions(&self) -> Vec<String> {
        vec!["rs".into()]
    }

    fn highlights(&self) -> Option<(tree_sitter::Language, &str)> {
        Some((tree_sitter_rust::LANGUAGE.into(), tree_sitter_rust::HIGHLIGHTS_QUERY))
    }

    fn template(&self) -> &str {
        "/// Twice x.\npub fn double(x: i64) -> i64 {\n    x * 2\n}\n"
    }

    fn read(&self, source: &str, cx: &dyn SigCx) -> Result<Function, String> {
        read_with(source, cx).map(|(f, _)| f)
    }

    fn ext(&self, f: &Function) -> Result<ExtKind, String> {
        let (_, read) = read_with(&f.source, &super::FunctionRecords(f))?;
        let glue = glue(f, &read)?;
        let (crate_dir, crate_name) = write_crate(&f.origin("rs", true), &f.source, &glue)?;
        Ok(ExtKind::Native { crate_dir, crate_name, ty: "LineitSquare".into() })
    }
}

/// What the glue needs that a [`Function`] doesn't say.
#[derive(Debug, Default)]
struct Read {
    passes: Vec<Pass>,
    /// The struct its `&mut self` is, and its fields.
    state: Option<(String, Vec<String>)>,
    /// How its result is read: one value, a tuple, or a struct's fields.
    returns: Returns,
    /// The names of the structs the code has.
    structs: Vec<String>,
}

#[derive(Debug, Default, PartialEq)]
enum Returns {
    #[default]
    Nothing,
    One,
    Tuple(usize),
    Fields(Vec<String>),
}

/// A struct of the code: its name and fields (name, type).
struct Struct<'a> {
    name: String,
    fields: Vec<(String, Node<'a>)>,
}

fn structs<'a>(root: Node<'a>, src: &str) -> Vec<Struct<'a>> {
    let mut v = Vec::new();
    for n in descendants(root).into_iter().filter(|n| n.kind() == "struct_item") {
        let Some(name) = n.child_by_field_name("name") else { continue };
        let mut fields = Vec::new();
        if let Some(body) = n.child_by_field_name("body").filter(|b| b.kind() == "field_declaration_list") {
            let mut c = body.walk();
            for f in body.named_children(&mut c).filter(|f| f.kind() == "field_declaration") {
                if let (Some(fname), Some(t)) = (f.child_by_field_name("name"), f.child_by_field_name("type")) {
                    fields.push((src[fname.byte_range()].to_string(), t));
                }
            }
        }
        v.push(Struct { name: src[name.byte_range()].to_string(), fields });
    }
    v
}

/// The types in the `<...>` of N.
fn type_args(n: Node) -> Vec<Node> {
    n.child_by_field_name("type_arguments")
        .map(|a| {
            let mut c = a.walk();
            a.named_children(&mut c).collect()
        })
        .unwrap_or_default()
}

/// A Rust type as lineit's, if it's one lineit can give and take.
fn rust_type(n: Node, src: &str, cx: &dyn SigCx) -> Result<Type, String> {
    let text = &src[n.byte_range()];
    let bad = || format!("`{text}` isn't a type a code square takes (i64, f64, bool, String, Vec<T>, BTreeMap<String, T>, a record)");
    Ok(match n.kind() {
        "primitive_type" => match text {
            "i64" => Type::Int,
            "f64" => Type::Float,
            "bool" => Type::Bool,
            "str" => Type::Str,
            _ => return Err(format!("`{text}`: numbers are i64 and f64 here")),
        },
        "type_identifier" if text == "String" => Type::Str,
        // a record of lineit's: the glue makes its struct
        "type_identifier" => Type::Record(cx.record(text).ok_or_else(bad)?),
        "generic_type" => {
            let head = n.child_by_field_name("type").map(|t| &src[t.byte_range()]).unwrap_or("");
            match (head, type_args(n).as_slice()) {
                ("Vec", [t]) => Type::List(Box::new(rust_type(*t, src, cx)?)),
                ("BTreeMap" | "std::collections::BTreeMap", [k, v]) if &src[k.byte_range()] == "String" => {
                    Type::Map(Box::new(rust_type(*v, src, cx)?))
                }
                _ => return Err(bad()),
            }
        }
        "array_type" => {
            // [T], as in &[T]
            let e = n.child_by_field_name("element").ok_or_else(bad)?;
            Type::List(Box::new(rust_type(e, src, cx)?))
        }
        "reference_type" => {
            let t = n.child_by_field_name("type").ok_or_else(bad)?;
            rust_type(t, src, cx)?
        }
        _ => return Err(bad()),
    })
}

fn read_with(source: &str, cx: &dyn SigCx) -> Result<(Function, Read), String> {
    let tree = super::parse(source, tree_sitter_rust::LANGUAGE.into(), "Rust")?;
    let root = tree.root_node();
    let text = |n: Node| &source[n.byte_range()];
    let structs = structs(root, source);

    // the `pub fn`, at the top or in an `impl`
    let mut found: Vec<(Node, Option<String>)> = Vec::new();
    let mut c = root.walk();
    for n in root.named_children(&mut c) {
        let public = |f: Node| {
            let mut c = f.walk();
            f.children(&mut c).any(|c| c.kind() == "visibility_modifier")
        };
        match n.kind() {
            "function_item" if public(n) => found.push((n, None)),
            "impl_item" => {
                let ty = n.child_by_field_name("type").map(|t| text(t).to_string());
                if let Some(body) = n.child_by_field_name("body") {
                    let mut c = body.walk();
                    for f in body.named_children(&mut c).filter(|f| f.kind() == "function_item" && public(*f)) {
                        found.push((f, ty.clone()));
                    }
                }
            }
            _ => {}
        }
    }
    let (f, owner) = match found.as_slice() {
        [one] => one.clone(),
        [] => return Err("there's no `pub fn`: the square runs the one that's `pub`".into()),
        many => {
            let names: Vec<&str> = many.iter().filter_map(|(f, _)| f.child_by_field_name("name")).map(text).collect();
            return Err(format!("it runs one function, and {} are `pub`: the others aren't", names.join(", ")));
        }
    };
    let name = text(f.child_by_field_name("name").ok_or("the function has no name")?).to_string();

    // its doc: the `///` lines right before it
    let mut doc = Vec::new();
    let mut prev = f.prev_named_sibling();
    while let Some(p) = prev.filter(|p| p.kind() == "line_comment" || p.kind() == "attribute_item") {
        if let Some(d) = p.child_by_field_name("doc") {
            doc.push(text(d).trim().to_string());
        }
        prev = p.prev_named_sibling();
    }
    doc.reverse();

    let mut read = Read { structs: structs.iter().map(|s| s.name.clone()).collect(), ..Read::default() };
    let mut params = Vec::new();
    let mut shared_name = None;
    let ps = f.child_by_field_name("parameters").ok_or("the function has no parameters")?;
    let mut c = ps.walk();
    for p in ps.named_children(&mut c) {
        if p.kind() == "self_parameter" {
            let owner = owner.clone().ok_or("`self` outside of an `impl`")?;
            let fields = structs
                .iter()
                .find(|s| s.name == owner)
                .map(|s| s.fields.iter().map(|(n, _)| n.clone()).collect())
                .unwrap_or_default();
            read.state = Some((owner, fields));
            continue;
        }
        if p.kind() != "parameter" {
            continue;
        }
        let pname = p.child_by_field_name("pattern").filter(|n| n.kind() == "identifier");
        let pname = pname.map(|n| text(n).to_string()).ok_or_else(|| format!("`{}`: name each parameter", text(p)))?;
        let ty = p.child_by_field_name("type").ok_or_else(|| format!("`{pname}` has no type"))?;

        // an exit: impl Fn(..), &mut dyn FnMut(..), &dyn Fn(..)
        let (how, inner) = match ty.kind() {
            "abstract_type" => ("", ty.child_by_field_name("trait")),
            "reference_type" => {
                let t = ty.child_by_field_name("type");
                let m = {
                    let mut c = ty.walk();
                    ty.children(&mut c).any(|c| c.kind() == "mutable_specifier")
                };
                match t.filter(|t| t.kind() == "dynamic_type") {
                    Some(d) => (if m { "&mut " } else { "&" }, d.child_by_field_name("trait")),
                    None => ("", None),
                }
            }
            _ => ("", None),
        };
        if let Some(func) = inner.filter(|t| t.kind() == "function_type") {
            let mut values = Vec::new();
            let mut arg_types = Vec::new();
            let mut fields = None;
            if let Some(fps) = func.child_by_field_name("parameters") {
                let mut c = fps.walk();
                let tys: Vec<Node> = fps.named_children(&mut c).collect();
                // one struct of the code: its fields are the values
                let one_struct = match tys.as_slice() {
                    [t] => structs.iter().find(|s| s.name == text(*t)),
                    _ => None,
                };
                if let Some(s) = one_struct {
                    for (fname, ft) in &s.fields {
                        values.push((Some(fname.clone()), rust_type(*ft, source, cx).map_err(|e| format!("`{pname}`: {e}"))?));
                    }
                    fields = Some(s.fields.iter().map(|(n, _)| n.clone()).collect());
                    arg_types.push(s.name.clone());
                } else {
                    for t in tys {
                        values.push((None, rust_type(t, source, cx).map_err(|e| format!("`{pname}`: {e}"))?));
                        arg_types.push(text(t).to_string());
                    }
                }
            }
            read.passes.push(Pass::Exit { how, args: arg_types, fields });
            params.push(Param { name: pname.clone(), role: Role::Exit(exit_values(&pname, values)), default: None });
            continue;
        }
        // Shared
        if text(ty).trim_start_matches('&').trim_start_matches("mut ").trim() == "Shared" {
            shared_name = Some(pname);
            read.passes.push(Pass::Shared { set: false });
            continue;
        }
        let t = rust_type(ty, source, cx).map_err(|e| format!("`{pname}`: {e}"))?;
        read.passes.push(if ty.kind() == "reference_type" { Pass::Ref } else { Pass::Value });
        params.push(Param { name: pname, role: Role::Input(Some(t)), default: None });
    }

    // what it returns
    let returns = match f.child_by_field_name("return_type") {
        None => Vec::new(),
        Some(r) if text(r) == "()" => Vec::new(),
        Some(r) if r.kind() == "tuple_type" => {
            let mut c = r.walk();
            let mut v = Vec::new();
            for t in r.named_children(&mut c) {
                v.push((None, rust_type(t, source, cx)?));
            }
            read.returns = Returns::Tuple(v.len());
            return_values(v)
        }
        Some(r) => match structs.iter().find(|s| s.name == text(r)) {
            Some(s) => {
                let mut v = Vec::new();
                for (fname, ft) in &s.fields {
                    v.push((Some(fname.clone()), rust_type(*ft, source, cx)?));
                }
                read.returns = Returns::Fields(s.fields.iter().map(|(n, _)| n.clone()).collect());
                return_values(v)
            }
            None => {
                read.returns = Returns::One;
                return_values(vec![(None, rust_type(r, source, cx)?)])
            }
        },
    };

    // the variables it uses through its Shared
    let mut shared: Vec<(String, bool)> = Vec::new();
    if let Some(sname) = &shared_name {
        for n in f.child_by_field_name("body").map(descendants).unwrap_or_default() {
            if n.kind() != "field_expression" {
                continue;
            }
            let (Some(v), Some(fld)) = (n.child_by_field_name("value"), n.child_by_field_name("field")) else { continue };
            if text(v) != sname {
                continue;
            }
            let set = n.parent().is_some_and(|p| {
                matches!(p.kind(), "assignment_expression" | "compound_assignment_expr")
                    && p.child_by_field_name("left") == Some(n)
            });
            let g = text(fld).to_string();
            match shared.iter_mut().find(|(s, _)| *s == g) {
                Some(s) => s.1 |= set,
                None => shared.push((g, set)),
            }
        }
        let set = shared.iter().any(|(_, s)| *s);
        for p in read.passes.iter_mut() {
            if let Pass::Shared { set: s } = p {
                *s = set;
            }
        }
    }

    let state = read.state.as_ref().map(|(_, f)| f.clone()).unwrap_or_default();
    // a `&mut self` with no fields still keeps it on ticks
    let state = if read.state.is_some() && state.is_empty() { vec!["self".into()] } else { state };
    let f = Function {
        name,
        doc: doc.join(" "),
        params,
        returns,
        state,
        shared,
        shared_types: Vec::new(),
        source: source.to_string(),
        file: None,
    };
    Ok((f, read))
}

/// The Rust after the code that makes it a lineit module.
fn glue(f: &Function, read: &Read) -> Result<String, String> {
    let mut o = String::from("\n\n// ---- lineit's: what makes the code above a module\n\n");
    // Shared, with the variables' types (the signature has them)
    o.push_str("#[allow(dead_code)]\npub struct Shared {\n");
    for (k, (g, _)) in f.shared.iter().enumerate() {
        let t = f.shared_types.get(k).ok_or_else(|| format!("there's no variable `{}`", shared_pin(g)))?;
        o.push_str(&format!("    pub {g}: {},\n", rust_of(t)));
    }
    o.push_str("}\n\n");
    o.push_str(&record_structs(f, read));
    let state_ty = read.state.as_ref().map(|(t, _)| t.clone()).unwrap_or_else(|| "()".into());
    o.push_str(&format!("pub struct LineitSquare {{\n    state: {state_ty},\n}}\n\n"));
    o.push_str("impl lineit_rt::native::Native for LineitSquare {\n");
    o.push_str("    fn new(_: &lineit_rt::native::Values) -> Self {\n        LineitSquare { state: Default::default() }\n    }\n\n");
    o.push_str("    fn eval(&mut self, i: &lineit_rt::native::Values) -> Result<lineit_rt::native::Values, String> {\n        self.run(None, i).map(|(set, _)| set)\n    }\n\n");
    o.push_str("    #[allow(unused_mut, unused_variables, clippy::all)]\n");
    o.push_str("    fn run(&mut self, _input: Option<&str>, i: &lineit_rt::native::Values) -> Result<(lineit_rt::native::Values, Vec<(String, lineit_rt::native::Values)>), String> {\n");
    o.push_str("        use lineit_rt::value::Typed;\n");
    o.push_str("        let get = |k: &str| i.get(k).cloned().ok_or_else(|| format!(\"no value for `{k}`\"));\n");
    o.push_str("        let fire = std::cell::RefCell::new(Vec::<(String, lineit_rt::native::Values)>::new());\n");
    if !f.shared.is_empty() || read.passes.iter().any(|p| matches!(p, Pass::Shared { .. })) {
        let fields: Vec<String> = f
            .shared
            .iter()
            .map(|(g, _)| format!("{g}: Typed::from_value(get({})?)", quoted(&shared_pin(g))))
            .collect();
        o.push_str(&format!("        let mut shared = Shared {{ {} }};\n", fields.join(", ")));
    }
    let mut args = Vec::new();
    let mut k = 0;
    for (p, pass) in f.params_and_shared(read) {
        match (p, pass) {
            (None, Pass::Shared { set }) => args.push(if *set { "&mut shared".into() } else { "&shared".into() }),
            (Some(p), Pass::Value) => args.push(format!("Typed::from_value(get({})?)", quoted(&p.name))),
            (Some(p), Pass::Ref) => {
                let Role::Input(Some(t)) = &p.role else { unreachable!() };
                o.push_str(&format!("        let a{k}: {} = Typed::from_value(get({})?);\n", rust_of(t), quoted(&p.name)));
                args.push(format!("&a{k}"));
                k += 1;
            }
            (Some(p), Pass::Exit { how, args: arg_types, fields }) => {
                let Role::Exit(values) = &p.role else { unreachable!() };
                // the closure's parameters, and each value's pin from them
                let (params, set): (String, Vec<String>) = match fields {
                    Some(fields) => (
                        format!("v: {}", arg_types[0]),
                        fields.iter().zip(values).map(|(fl, (pin, _))| format!("({}.to_string(), v.{fl}.to_value())", quoted(pin))).collect(),
                    ),
                    None => (
                        arg_types.iter().enumerate().map(|(j, t)| format!("v{j}: {t}")).collect::<Vec<_>>().join(", "),
                        values.iter().enumerate().map(|(j, (pin, _))| format!("({}.to_string(), v{j}.to_value())", quoted(pin))).collect(),
                    ),
                };
                o.push_str(&format!(
                    "        let mut x{k} = |{params}| fire.borrow_mut().push(({}.to_string(), lineit_rt::native::Values::from([{}])));\n",
                    quoted(&p.name),
                    set.join(", ")
                ));
                args.push(format!("{how}x{k}"));
                k += 1;
            }
            _ => unreachable!(),
        }
    }
    let target = if read.state.is_some() { format!("self.state.{}", f.name) } else { f.name.clone() };
    o.push_str(&format!("        let r = {target}({});\n", args.join(", ")));
    o.push_str("        let mut set = lineit_rt::native::Values::new();\n");
    match &read.returns {
        Returns::Nothing => o.push_str("        let _ = r;\n"),
        Returns::One => o.push_str(&format!("        set.insert({}.to_string(), r.to_value());\n", quoted(&f.returns[0].0))),
        Returns::Tuple(n) => {
            for j in 0..*n {
                o.push_str(&format!("        set.insert({}.to_string(), r.{j}.to_value());\n", quoted(&f.returns[j].0)));
            }
        }
        Returns::Fields(fields) => {
            for (fl, (pin, _)) in fields.iter().zip(&f.returns) {
                o.push_str(&format!("        set.insert({}.to_string(), r.{fl}.to_value());\n", quoted(pin)));
            }
        }
    }
    for (g, _) in f.shared.iter().filter(|(_, s)| *s) {
        o.push_str(&format!("        set.insert({}.to_string(), shared.{g}.to_value());\n", quoted(&shared_pin(g))));
    }
    o.push_str("        let mut fired = fire.take();\n");
    if f.ticked() && f.exits().next().is_none() {
        o.push_str("        fired.push((\"then\".to_string(), lineit_rt::native::Values::new()));\n");
    }
    o.push_str("        Ok((set, fired))\n    }\n");
    if let Some((_, fields)) = &read.state {
        o.push_str("\n    fn state(&mut self) -> lineit_rt::native::Values {\n        use lineit_rt::value::Typed;\n        lineit_rt::native::Values::from([");
        let v: Vec<String> = fields.iter().map(|fl| format!("({}.to_string(), self.state.{fl}.to_value())", quoted(fl))).collect();
        o.push_str(&v.join(", "));
        o.push_str("])\n    }\n");
        o.push_str("\n    fn set_state(&mut self, s: &lineit_rt::native::Values) -> Result<(), String> {\n        use lineit_rt::value::Typed;\n");
        for fl in fields {
            o.push_str(&format!(
                "        if let Some(v) = s.get({}) {{ self.state.{fl} = Typed::from_value(v.clone()); }}\n",
                quoted(fl)
            ));
        }
        o.push_str("        Ok(())\n    }\n");
    }
    o.push_str("}\n");
    Ok(o)
}

/// The structs of the records F uses (that the code doesn't have itself),
/// and how they're lineit's values and back.
fn record_structs(f: &Function, read: &Read) -> String {
    let mut records: Vec<std::sync::Arc<lineit_rt::RecordType>> = Vec::new();
    let mut add = |t: &Type| {
        super::visit(t, &mut |t| {
            if let Type::Record(r) = t
                && !records.iter().any(|x| x.name == r.name)
                && !read.structs.contains(&r.name)
            {
                records.push(r.clone());
            }
        })
    };
    for p in &f.params {
        match &p.role {
            Role::Input(Some(t)) => add(t),
            Role::Exit(v) => v.iter().for_each(|(_, t)| add(t)),
            Role::Input(None) => {}
        }
    }
    f.returns.iter().for_each(|(_, t)| add(t));
    f.shared_types.iter().for_each(&mut add);
    let mut o = String::new();
    for r in records {
        let name = &r.name;
        let fields: Vec<String> = r.fields.iter().map(|(n, t)| format!("    pub {n}: {},\n", rust_of(t))).collect();
        o.push_str(&format!(
            "/// The record `{name}` of lineit.\n#[allow(non_camel_case_types, dead_code)]\n#[derive(Clone, Debug, Default)]\npub struct {name} {{\n{}}}\n\n",
            fields.concat()
        ));
        let to: Vec<String> = r.fields.iter().map(|(n, _)| format!("self.{n}.to_value()")).collect();
        let names: Vec<String> = r.fields.iter().map(|(n, _)| quoted(n)).collect();
        let from: Vec<String> = r
            .fields
            .iter()
            .map(|(n, _)| format!("{n}: field({}).map(lineit_rt::value::Typed::from_value).unwrap_or_default()", quoted(n)))
            .collect();
        o.push_str(&format!(
            "impl lineit_rt::value::Typed for {name} {{
    fn to_value(&self) -> lineit_rt::Value {{
        use lineit_rt::value::Typed;
        let values = vec![{to}];
        let names = [{names}];
        let fields = names.iter().zip(&values).map(|(n, v)| (n.to_string(), v.ty())).collect();
        let ty = std::sync::Arc::new(lineit_rt::RecordType {{ name: {qname}.into(), fields }});
        lineit_rt::Value::Record(std::sync::Arc::new(lineit_rt::value::Record {{ ty, fields: values }}))
    }}
    fn from_value(v: lineit_rt::Value) -> Self {{
        let lineit_rt::Value::Record(r) = v else {{ return Self::default() }};
        let field = |n: &str| r.ty.fields.iter().position(|(f, _)| f == n).map(|i| r.fields[i].clone());
        {name} {{ {from} }}
    }}
}}

",
            to = to.join(", "),
            names = names.join(", "),
            qname = quoted(name),
            from = from.join(", "),
        ));
    }
    o
}

/// The Rust type the glue uses for T.
fn rust_of(t: &Type) -> String {
    match t {
        Type::List(e) => format!("Vec<{}>", rust_of(e)),
        Type::Map(e) => format!("std::collections::BTreeMap<String, {}>", rust_of(e)),
        Type::Record(r) => r.name.clone(),
        t => t.rust(),
    }
}

impl Function {
    /// Its parameters and the glue's way to pass each, with `Shared` (which
    /// isn't an input) as None.
    fn params_and_shared<'a>(&'a self, read: &'a Read) -> Vec<(Option<&'a Param>, &'a Pass)> {
        let mut ps = self.params.iter();
        read.passes.iter().map(|pass| (if matches!(pass, Pass::Shared { .. }) { None } else { ps.next() }, pass)).collect()
    }
}

/// Writes the crate of a code square (the code, as `src/FILE`, and its
/// glue) where lineit keeps them, if it isn't there. Returns its folder and
/// name.
fn write_crate(file: &str, source: &str, glue: &str) -> Result<(PathBuf, String), String> {
    let code = format!("{source}{glue}");
    // the runtime lineit carries, as it is: no crates.io needed
    let rt = crate::codegen::write_runtime()?.to_string_lossy().replace('\\', "/");
    let crate_name = format!("{CRATE_PREFIX}{:016x}", crate::codegen::fnv(code.as_bytes()));
    let dir = crate::paths::cache_dir().join("code").join(&crate_name);
    let write = |path: PathBuf, text: &str| -> Result<(), String> {
        if std::fs::read_to_string(&path).ok().as_deref() == Some(text) {
            return Ok(());
        }
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| format!("{}: {e}", path.display()))?;
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
    };
    write(
        dir.join("Cargo.toml"),
        &format!(
            "[package]\nname = \"{crate_name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\ncrate-type = [\"cdylib\", \"rlib\"]\n\n[dependencies]\nlineit-rt = {{ path = \"{rt}\" }}\n\n[workspace]\n"
        ),
    )?;
    write(
        dir.join("src/lib.rs"),
        &format!(
            "/// What the code prints goes where the program's lines go.\n#[allow(unused_macros)]\nmacro_rules! println {{\n    () => {{ lineit_rt::ext::print(\"\") }};\n    ($($t:tt)*) => {{ lineit_rt::ext::print(&format!($($t)*)) }};\n}}\n\n#[allow(dead_code, unused_imports, clippy::all)]\n#[path = \"{file}\"]\nmod code;\n\nuse code::LineitSquare;\nlineit_rt::export_modules!(LineitSquare);\n"
        ),
    )?;
    write(dir.join("src").join(file), &code)?;
    super::crate_used(&dir);
    Ok((dir, crate_name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lineit_rt::RecordType;
    use std::sync::Arc;

    fn read(s: &str, cx: &dyn SigCx) -> Result<(Function, Read), String> {
        read_with(s, cx)
    }

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

    pub const COLLATZ: &str = "#[derive(Default)]
pub struct Collatz {
    count: i64,
}

pub struct Odd {
    calls: i64,
    n: i64,
}

impl Collatz {
    /// Halves even numbers; counts and passes on the odd ones.
    pub fn collatz(&mut self, n: i64, shared: &mut Shared, mut even: impl FnMut(i64), mut odd: impl FnMut(Odd)) {
        shared.total += n;
        if n % 2 == 0 {
            return even(n / 2);
        }
        self.count += 1;
        odd(Odd { calls: self.count, n })
    }
}
";

    #[test]
    fn reads() {
        let (f, _) = read(COLLATZ, &Cx).unwrap();
        assert_eq!(f.name, "collatz");
        assert_eq!(f.doc, "Halves even numbers; counts and passes on the odd ones.");
        assert_eq!(f.params[0].role, Role::Input(Some(Type::Int)));
        assert_eq!(f.params[1].role, Role::Exit(vec![("even.value".into(), Type::Int)]));
        assert_eq!(f.params[2].role, Role::Exit(vec![("calls".into(), Type::Int), ("n".into(), Type::Int)]));
        assert_eq!(f.state, ["count"]);
        assert_eq!(f.shared, [("total".to_string(), true)]);

        let (f, r) = read(
            "fn h(x: i64) -> i64 { x }\npub struct R { sq: i64, text: String }\npub fn f(x: i64, s: &str, xs: &[f64]) -> R { R { sq: h(x), text: s.into() } }",
            &Cx,
        )
        .unwrap();
        assert!(!f.ticked());
        assert_eq!(f.returns, [("sq".to_string(), Type::Int), ("text".to_string(), Type::Str)]);
        assert_eq!(r.passes, [Pass::Value, Pass::Ref, Pass::Ref]);
    }

    #[test]
    fn errors() {
        let err = |s: &str| read(s, &Cx).unwrap_err();
        assert!(err("fn f() {}").contains("there's no `pub fn`"));
        assert!(err("pub fn f() {}\npub fn g() {}").contains("f, g are `pub`"));
        assert!(err("pub fn f(x: i32) {}").contains("i64 and f64"));
        assert!(err("pub fn f(x: Foo) {}").contains("`Foo` isn't a type"));
        assert!(err("pub fn f(").contains("line 1"));
    }
}

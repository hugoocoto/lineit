//! Code squares in C, compiled by the machine's C compiler (through the
//! `cc` crate) into a Rust module, like a Rust code square.
//!
//! ```c
//! static int64_t count = 0; // the square's state
//!
//! typedef struct { int64_t calls; int64_t n; } Odd;
//!
//! /** Halves even numbers; counts and passes on the odd ones. */
//! void collatz(int64_t n, Shared *shared, void (*even)(int64_t half), void (*odd)(Odd v)) {
//!     shared->total += n;
//!     if (n % 2 == 0) { even(n / 2); return; }
//!     count++;
//!     odd((Odd){ count, n });
//! }
//! ```
//!
//! - The function is the one that isn't `static`; its helpers are.
//! - Types: `int64_t`, `double`, `bool`, `const char *`, and lineit's
//!   `list_T` (`.items`, `.len`), `map_T` (`.keys`, `.values`, `.len`) and
//!   records (a struct with the record's name), with T `int`, `float`,
//!   `bool`, `str`, `list_T`... `lineit.h` has them, and `lineit_alloc(n)`:
//!   memory that's freed after each call, to return lists in.
//! - What it returns is `result`; a struct of the code gives its fields.
//!   Strings it returns are copied once it returns: literals, `static`
//!   ones and `lineit_alloc`'s are fine.
//! - A parameter `void (*name)(T value, ...)` is an exit; its parameters'
//!   names are its values' (one struct of the code: its fields').
//! - `Shared *shared` (`const Shared *` to only read) has the variables
//!   `::NAME` it uses as fields: `shared->total`.
//! - Its state: the variables at the top of the code (not `const`): int64_t,
//!   double or bool. Each square has its own; lineit puts them in the
//!   variables before each call, and takes them back after.
//!
//! The code goes in a crate as it is (`#include`d by the glue, so the
//! compiler's errors have its lines), with its glue in C ([`glue_c`]),
//! which turns lineit's values (`lineit_value`, see
//! [`lineit_rt::cabi`]) into the code's types and back.

use std::path::PathBuf;

use lineit_rt::Type;
use lineit_rt::cabi::c_name;
use tree_sitter::Node;

use super::{Function, Language, Param, Role, descendants, exit_values, quoted, return_values};
use crate::ir::ExtKind;
use crate::registry::SigCx;

pub struct C;

impl Language for C {
    fn name(&self) -> &str {
        "c"
    }

    fn title(&self) -> &str {
        "C"
    }

    fn extensions(&self) -> Vec<String> {
        vec!["c".into(), "h".into()]
    }

    fn highlights(&self) -> Option<(tree_sitter::Language, &str)> {
        Some((tree_sitter_c::LANGUAGE.into(), tree_sitter_c::HIGHLIGHT_QUERY))
    }

    fn template(&self) -> &str {
        "/** Twice x. */\nint64_t twice(int64_t x) {\n    return x * 2;\n}\n"
    }

    fn read(&self, source: &str, cx: &dyn SigCx) -> Result<Function, String> {
        read(source, cx).map(|(f, _)| f)
    }

    fn ext(&self, f: &Function) -> Result<ExtKind, String> {
        let (_, read) = read(&f.source, &super::FunctionRecords(f))?;
        let (crate_dir, crate_name) = write_crate(f, &read)?;
        Ok(ExtKind::Native { crate_dir, crate_name, ty: "LineitSquare".into() })
    }
}

/// What the glue needs that a [`Function`] doesn't say.
#[derive(Debug, Default)]
struct Read {
    /// Per parameter (`Shared` too, as None): how it's passed.
    passes: Vec<Pass>,
    /// How its result is read: nothing, a value, a struct's fields.
    returns: Returns,
    /// Per exit: its values' C types, and the struct they're in, if they're
    /// one struct's fields.
    exits: Vec<(Vec<String>, Option<String>)>,
    /// Its state, with each one's type.
    state: Vec<(String, Type)>,
    shared_param: Option<String>,
}

#[derive(Debug, PartialEq)]
enum Pass {
    Input,
    Exit,
    Shared,
}

#[derive(Debug, Default, PartialEq)]
enum Returns {
    #[default]
    Nothing,
    One(Type),
    /// A struct of the code: its name and fields.
    Fields(String, Vec<String>),
}

/// The C type of a lineit type.
fn c_type(t: &Type) -> String {
    match t {
        Type::Int => "int64_t".into(),
        Type::Float => "double".into(),
        Type::Bool => "bool".into(),
        Type::Str => "const char *".into(),
        _ => c_name(t),
    }
}

/// The type a C type name says (`int64_t`, `list_int`, a record...).
fn named_type(name: &str, cx: &dyn SigCx) -> Result<Type, String> {
    Ok(match name {
        "int64_t" => Type::Int,
        "double" => Type::Float,
        "bool" | "_Bool" => Type::Bool,
        "int" | "long" | "int32_t" | "short" | "char" | "unsigned" | "size_t" | "uint64_t" | "float" => {
            return Err(format!("`{name}`: numbers are int64_t and double here"));
        }
        _ => mangled(name, cx)?,
    })
}

/// `int`, `float`, `bool`, `str`, `list_T`, `map_T` or a record: what
/// `list_` and `map_` say their items are.
fn mangled(name: &str, cx: &dyn SigCx) -> Result<Type, String> {
    Ok(match name {
        "int" => Type::Int,
        "float" => Type::Float,
        "bool" => Type::Bool,
        "str" => Type::Str,
        _ => {
            if let Some(e) = name.strip_prefix("list_") {
                Type::List(Box::new(mangled(e, cx)?))
            } else if let Some(e) = name.strip_prefix("map_") {
                Type::Map(Box::new(mangled(e, cx)?))
            } else {
                Type::Record(cx.record(name).ok_or_else(|| format!("`{name}` isn't a type lineit knows"))?)
            }
        }
    })
}

/// The name a declarator declares (under its pointers, parentheses...).
fn declared<'a>(d: Node<'a>, src: &'a str) -> Option<&'a str> {
    match d.kind() {
        "identifier" | "field_identifier" | "type_identifier" => Some(&src[d.byte_range()]),
        _ => d.child_by_field_name("declarator").or_else(|| d.named_child(0)).and_then(|c| declared(c, src)),
    }
}

/// A declaration's type: its TYPE node, with DECL's pointers.
fn decl_type(ty: Node, decl: Option<Node>, src: &str, cx: &dyn SigCx) -> Result<Type, String> {
    let name = &src[ty.byte_range()];
    let pointer = decl.is_some_and(|d| d.kind() == "pointer_declarator");
    if pointer {
        return match name {
            "char" => Ok(Type::Str),
            _ => Err(format!("`{name} *`: the pointers lineit gives are `const char *` and `Shared *`")),
        };
    }
    named_type(name, cx)
}

/// The structs the code declares with `typedef`: name and fields (name,
/// type, declarator).
fn typedefs<'a>(root: Node<'a>, src: &'a str) -> Vec<(String, Vec<(String, Node<'a>, Node<'a>)>)> {
    let mut v = Vec::new();
    let mut c = root.walk();
    for n in root.named_children(&mut c).filter(|n| n.kind() == "type_definition") {
        let (Some(t), Some(d)) = (n.child_by_field_name("type"), n.child_by_field_name("declarator")) else { continue };
        let Some(body) = t.child_by_field_name("body") else { continue };
        let mut fields = Vec::new();
        let mut c = body.walk();
        for f in body.named_children(&mut c).filter(|f| f.kind() == "field_declaration") {
            if let (Some(ft), Some(fd)) = (f.child_by_field_name("type"), f.child_by_field_name("declarator"))
                && let Some(fname) = declared(fd, src)
            {
                fields.push((fname.to_string(), ft, fd));
            }
        }
        v.push((src[d.byte_range()].to_string(), fields));
    }
    v
}

fn has_child(n: Node, kind: &str, text: Option<&str>, src: &str) -> bool {
    let mut c = n.walk();
    n.children(&mut c).any(|c| c.kind() == kind && text.is_none_or(|t| &src[c.byte_range()] == t))
}

fn read(source: &str, cx: &dyn SigCx) -> Result<(Function, Read), String> {
    let tree = super::parse(source, tree_sitter_c::LANGUAGE.into(), "C")?;
    let root = tree.root_node();
    let text = |n: Node| &source[n.byte_range()];
    let typedefs = typedefs(root, source);
    let mut read = Read::default();

    // the function that isn't static, and the variables at the top
    let mut found = Vec::new();
    let mut c = root.walk();
    for n in root.named_children(&mut c) {
        let is_static = has_child(n, "storage_class_specifier", Some("static"), source);
        match n.kind() {
            "function_definition" if !is_static => found.push(n),
            "declaration" => {
                let constant = has_child(n, "type_qualifier", Some("const"), source)
                    || has_child(n, "storage_class_specifier", Some("extern"), source);
                let (Some(ty), Some(d)) = (n.child_by_field_name("type"), n.child_by_field_name("declarator")) else {
                    continue;
                };
                // a function's declaration isn't a variable
                if constant || descendants(d).iter().any(|x| x.kind() == "function_declarator") {
                    continue;
                }
                let mut c = n.walk();
                for d in n.children_by_field_name("declarator", &mut c) {
                    let Some(name) = declared(d, source) else { continue };
                    let inner =
                        if d.kind() == "init_declarator" { d.child_by_field_name("declarator") } else { Some(d) };
                    let t = decl_type(ty, inner, source, cx)?;
                    if !matches!(t, Type::Int | Type::Float | Type::Bool) {
                        return Err(format!(
                            "`{name}`: what a C square keeps is int64_t, double or bool (make it `const` if it's a constant)"
                        ));
                    }
                    read.state.push((name.to_string(), t));
                }
            }
            _ => {}
        }
    }
    let f = match found.as_slice() {
        [f] => *f,
        [] => return Err("there's no function that isn't `static`: the square runs that one".into()),
        many => {
            let names: Vec<&str> = many
                .iter()
                .filter_map(|f| f.child_by_field_name("declarator"))
                .filter_map(|d| declared(d, source))
                .collect();
            return Err(format!(
                "it runs one function, and {} aren't `static`: make the helpers `static`",
                names.join(", ")
            ));
        }
    };
    let decl = f.child_by_field_name("declarator").ok_or("the function has no name")?;
    let fdecl =
        descendants(decl).into_iter().find(|d| d.kind() == "function_declarator").ok_or("the function has no name")?;
    let name = declared(fdecl, source).ok_or("the function has no name")?.to_string();

    // its doc: the comment right before it
    let doc = f
        .prev_named_sibling()
        .filter(|p| p.kind() == "comment" && p.end_position().row + 1 >= f.start_position().row)
        .map(|p| {
            let t = text(p);
            let t =
                t.trim_start_matches("/**").trim_start_matches("/*").trim_start_matches("//").trim_end_matches("*/");
            t.lines()
                .map(|l| l.trim().trim_start_matches('*').trim())
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();

    // its parameters
    let mut params = Vec::new();
    if let Some(ps) = fdecl.child_by_field_name("parameters") {
        let mut c = ps.walk();
        for p in ps.named_children(&mut c).filter(|p| p.kind() == "parameter_declaration") {
            let ty = p.child_by_field_name("type").ok_or("a parameter with no type")?;
            let Some(d) = p.child_by_field_name("declarator") else {
                if text(ty) == "void" {
                    continue;
                }
                return Err(format!("`{}`: name each parameter", text(p)));
            };
            let pname = declared(d, source).ok_or_else(|| format!("`{}`: name each parameter", text(p)))?.to_string();
            // an exit: void (*name)(...)
            if d.kind() == "function_declarator" {
                if text(ty) != "void" {
                    return Err(format!("`{pname}`: an exit returns nothing (`void (*{pname})(...)`)"));
                }
                let mut values = Vec::new();
                let mut ctypes = Vec::new();
                let mut in_struct = None;
                if let Some(eps) = d.child_by_field_name("parameters") {
                    let mut c = eps.walk();
                    let eps: Vec<Node> =
                        eps.named_children(&mut c).filter(|e| e.kind() == "parameter_declaration").collect();
                    let one = match eps.as_slice() {
                        [e] => e.child_by_field_name("type").and_then(|t| typedefs.iter().find(|(n, _)| n == text(t))),
                        _ => None,
                    };
                    if let Some((sname, fields)) = one {
                        for (fname, ft, fd) in fields {
                            values.push((Some(fname.clone()), decl_type(*ft, Some(*fd), source, cx)?));
                        }
                        in_struct = Some(sname.clone());
                        ctypes.push(sname.clone());
                    } else {
                        for e in eps {
                            let et = e.child_by_field_name("type").ok_or("a parameter with no type")?;
                            let ed = e.child_by_field_name("declarator");
                            if text(et) == "void" && ed.is_none() {
                                continue;
                            }
                            let t = decl_type(et, ed, source, cx).map_err(|err| format!("`{pname}`: {err}"))?;
                            ctypes.push(c_type(&t));
                            values.push((ed.and_then(|d| declared(d, source)).map(String::from), t));
                        }
                    }
                }
                read.exits.push((ctypes, in_struct));
                read.passes.push(Pass::Exit);
                params.push(Param {
                    name: pname.clone(),
                    role: Role::Exit(exit_values(&pname, values)),
                    default: None,
                });
                continue;
            }
            if text(ty) == "Shared" {
                read.shared_param = Some(pname);
                read.passes.push(Pass::Shared);
                continue;
            }
            let t = decl_type(ty, Some(d), source, cx).map_err(|e| format!("`{pname}`: {e}"))?;
            read.passes.push(Pass::Input);
            params.push(Param { name: pname, role: Role::Input(Some(t)), default: None });
        }
    }

    // what it returns
    let rty = f.child_by_field_name("type").ok_or("the function has no type")?;
    let returns = if text(rty) == "void" && decl.kind() != "pointer_declarator" {
        Vec::new()
    } else if let Some((sname, fields)) = typedefs.iter().find(|(n, _)| n == text(rty)) {
        let mut v = Vec::new();
        for (fname, ft, fd) in fields {
            v.push((Some(fname.clone()), decl_type(*ft, Some(*fd), source, cx)?));
        }
        read.returns = Returns::Fields(sname.clone(), fields.iter().map(|(n, _, _)| n.clone()).collect());
        return_values(v)
    } else {
        let t = decl_type(rty, Some(decl), source, cx)?;
        read.returns = Returns::One(t.clone());
        return_values(vec![(None, t)])
    };

    // the variables it uses through its Shared
    let mut shared: Vec<(String, bool)> = Vec::new();
    if let Some(sname) = &read.shared_param {
        for n in f.child_by_field_name("body").map(descendants).unwrap_or_default() {
            if n.kind() != "field_expression" {
                continue;
            }
            let (Some(a), Some(fld)) = (n.child_by_field_name("argument"), n.child_by_field_name("field")) else {
                continue;
            };
            if text(a) != sname {
                continue;
            }
            let set = n.parent().is_some_and(|p| {
                (p.kind() == "assignment_expression" && p.child_by_field_name("left") == Some(n))
                    || p.kind() == "update_expression"
            });
            let g = text(fld).to_string();
            match shared.iter_mut().find(|(s, _)| *s == g) {
                Some(s) => s.1 |= set,
                None => shared.push((g, set)),
            }
        }
    }

    let state = read.state.iter().map(|(n, _)| n.clone()).collect();
    let f = Function {
        name,
        doc,
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

/// The types F gives and takes, each after the types in it.
fn used_types(f: &Function, read: &Read) -> Vec<Type> {
    fn walk(t: &Type, out: &mut Vec<Type>) {
        match t {
            Type::List(e) | Type::Map(e) => walk(e, out),
            Type::Record(r) => r.fields.iter().for_each(|(_, t)| walk(t, out)),
            _ => {}
        }
        if !out.contains(t) {
            out.push(t.clone());
        }
    }
    let mut types = Vec::new();
    for p in &f.params {
        match &p.role {
            Role::Input(Some(t)) => walk(t, &mut types),
            Role::Exit(v) => v.iter().for_each(|(_, t)| walk(t, &mut types)),
            Role::Input(None) => {}
        }
    }
    f.returns.iter().for_each(|(_, t)| walk(t, &mut types));
    f.shared_types.iter().for_each(|t| walk(t, &mut types));
    read.state.iter().for_each(|(_, t)| walk(t, &mut types));
    types
}

/// `lineit.h`: lineit's value, `lineit_alloc`, the types F uses and its
/// `Shared`. H makes the names of the crate's own functions its own.
fn header(f: &Function, read: &Read, h: &str) -> String {
    let types = used_types(f, read);

    let mut o = format!(
        "/* lineit's: the types a code square in C gives and takes. */
#ifndef LINEIT_H
#define LINEIT_H
#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdio.h>

typedef struct lineit_value {{
    uint32_t tag;
    int64_t i;
    double f;
    const char *s;
    const struct lineit_value *items;
    const char *const *keys;
    size_t len;
}} lineit_value;

#define lineit_alloc lineit_alloc_{h}
/* N bytes, zeroed, freed after the call: what returned lists are made of */
void *lineit_alloc(size_t n);
void lineit_fire_{h}(size_t exit, const lineit_value *values, size_t n);

/* what the code prints goes where the program's lines go */
void lineit_print_{h}(const char *line);
static int lineit_printf(const char *format, ...);
static int lineit_puts(const char *s);
#define printf(...) lineit_printf(__VA_ARGS__)
#define puts(s) lineit_puts(s)

"
    );
    for t in &types {
        match t {
            Type::List(e) => {
                o.push_str(&format!("typedef struct {{ {} *items; size_t len; }} {};\n", c_type(e), c_name(t)))
            }
            Type::Map(e) => o.push_str(&format!(
                "typedef struct {{ const char **keys; {} *values; size_t len; }} {};\n",
                c_type(e),
                c_name(t)
            )),
            Type::Record(r) => {
                let fields: Vec<String> = r.fields.iter().map(|(n, t)| format!("{} {n};", c_type(t))).collect();
                o.push_str(&format!("typedef struct {{ {} }} {};\n", fields.join(" "), r.name));
            }
            _ => {}
        }
    }
    let fields: Vec<String> =
        f.shared.iter().zip(&f.shared_types).map(|((g, _), t)| format!("{} {g};", c_type(t))).collect();
    o.push_str(&format!(
        "typedef struct {{ {} }} Shared;\n\n#endif\n",
        if fields.is_empty() { "char none;".to_string() } else { fields.join(" ") }
    ));
    o
}

/// The C that turns lineit's values into a T and back: `lineit_from_NAME`
/// and `lineit_to_NAME`.
fn conversions(f: &Function, read: &Read) -> String {
    let mut types = used_types(f, read);
    // the scalars, which the others use
    for t in [Type::Int, Type::Float, Type::Bool, Type::Str] {
        if !types.contains(&t) {
            types.insert(0, t);
        }
    }

    let mut o = String::new();
    for t in &types {
        let (n, ct) = (c_name(t), c_type(t));
        let (from, to) = match t {
            Type::Int => (
                "return v->tag == 1 ? (int64_t)v->f : v->i;".to_string(),
                "lineit_value v = {0}; v.tag = 0; v.i = x; return v;".to_string(),
            ),
            Type::Float => (
                "return v->tag == 1 ? v->f : (double)v->i;".into(),
                "lineit_value v = {0}; v.tag = 1; v.f = x; return v;".into(),
            ),
            Type::Bool => {
                ("return v->i != 0;".into(), "lineit_value v = {0}; v.tag = 2; v.i = x ? 1 : 0; return v;".into())
            }
            Type::Str => {
                ("return v->s ? v->s : \"\";".into(), "lineit_value v = {0}; v.tag = 3; v.s = x; return v;".into())
            }
            Type::List(e) => {
                let en = c_name(e);
                (
                    format!(
                        "{ct} x; x.len = v->len; x.items = lineit_alloc(sizeof(*x.items) * (v->len + 1)); for (size_t k = 0; k < v->len; k++) x.items[k] = lineit_from_{en}(&v->items[k]); return x;"
                    ),
                    format!(
                        "lineit_value v = {{0}}; lineit_value *items = lineit_alloc(sizeof(lineit_value) * (x.len + 1)); for (size_t k = 0; k < x.len; k++) items[k] = lineit_to_{en}(x.items[k]); v.tag = 4; v.items = items; v.len = x.len; return v;"
                    ),
                )
            }
            Type::Map(e) => {
                let en = c_name(e);
                (
                    format!(
                        "{ct} x; x.len = v->len; x.keys = lineit_alloc(sizeof(*x.keys) * (v->len + 1)); x.values = lineit_alloc(sizeof(*x.values) * (v->len + 1)); for (size_t k = 0; k < v->len; k++) {{ x.keys[k] = v->keys[k]; x.values[k] = lineit_from_{en}(&v->items[k]); }} return x;"
                    ),
                    format!(
                        "lineit_value v = {{0}}; lineit_value *items = lineit_alloc(sizeof(lineit_value) * (x.len + 1)); for (size_t k = 0; k < x.len; k++) items[k] = lineit_to_{en}(x.values[k]); v.tag = 5; v.items = items; v.keys = (const char *const *)x.keys; v.len = x.len; return v;"
                    ),
                )
            }
            Type::Record(r) => {
                let from: Vec<String> = r
                    .fields
                    .iter()
                    .enumerate()
                    .map(|(k, (fname, ft))| format!("x.{fname} = lineit_from_{}(&v->items[{k}]);", c_name(ft)))
                    .collect();
                let to: Vec<String> = r
                    .fields
                    .iter()
                    .enumerate()
                    .map(|(k, (fname, ft))| format!("items[{k}] = lineit_to_{}(x.{fname});", c_name(ft)))
                    .collect();
                let keys: Vec<String> = r.fields.iter().map(|(fname, _)| quoted(fname)).collect();
                (
                    format!("{ct} x; {}; return x;", from.join(" ")),
                    format!(
                        "static const char *const keys[] = {{ {} }}; lineit_value v = {{0}}; lineit_value *items = lineit_alloc(sizeof(lineit_value) * {}); {} v.tag = 6; v.items = items; v.keys = keys; v.len = {}; return v;",
                        keys.join(", "),
                        r.fields.len().max(1),
                        to.join(" "),
                        r.fields.len()
                    ),
                )
            }
            Type::Image => continue,
        };
        o.push_str(&format!(
            "static {ct} lineit_from_{n}(const lineit_value *v);\nstatic lineit_value lineit_to_{n}({ct} x);\n"
        ));
        o.push_str(&format!("static {ct} lineit_from_{n}(const lineit_value *v) {{ {from} }}\n"));
        o.push_str(&format!("static lineit_value lineit_to_{n}({ct} x) {{ {to} }}\n"));
    }
    o
}

/// The C that calls F: `lineit_call_H`, `lineit_state_get_H` and
/// `lineit_state_set_H` (see [`lineit_rt::cabi::CSquare`]).
pub fn glue_c(f: &Function, file: &str, h: &str) -> Result<String, String> {
    let (_, read) = read(&f.source, &super::FunctionRecords(f))?;
    let fname = &f.name;
    let mut o = format!(
        "/* lineit's: what calls the code in {file} */
#include \"lineit.h\"
/* its own name, so a program can have several functions of the same name
   (where a `(` follows it: the function, not a field of the same name) */
#define {fname}(...) lineit_fn_{h}(__VA_ARGS__)
#include \"{file}\"
#undef {fname}

"
    );
    o.push_str(&format!(
        "#include <stdarg.h>
#include <stdlib.h>
#include <string.h>

/* the line being printed, until its end */
static char *lineit_line = NULL;
static size_t lineit_line_len = 0;

static void lineit_line_end(void) {{
    lineit_line = realloc(lineit_line, lineit_line_len + 1);
    lineit_line[lineit_line_len] = 0;
    lineit_print_{h}(lineit_line);
    lineit_line_len = 0;
}}

static void lineit_out(const char *s, size_t n) {{
    for (size_t k = 0; k < n; k++) {{
        if (s[k] == '\\n') {{
            lineit_line_end();
        }} else {{
            lineit_line = realloc(lineit_line, lineit_line_len + 2);
            lineit_line[lineit_line_len++] = s[k];
        }}
    }}
}}

static int lineit_printf(const char *format, ...) {{
    va_list a;
    va_start(a, format);
    int n = vsnprintf(NULL, 0, format, a);
    va_end(a);
    if (n < 0) return n;
    char *b = malloc((size_t)n + 1);
    va_start(a, format);
    vsnprintf(b, (size_t)n + 1, format, a);
    va_end(a);
    lineit_out(b, (size_t)n);
    free(b);
    return n;
}}

static int lineit_puts(const char *s) {{
    lineit_out(s, strlen(s));
    lineit_out(\"\\n\", 1);
    return 1;
}}

"
    ));
    o.push_str(&conversions(f, &read));
    // the exits: they give lineit their values
    let mut exit = 0;
    for p in &f.params {
        let Role::Exit(values) = &p.role else { continue };
        let (ctypes, in_struct) = &read.exits[exit];
        let (params, items): (String, Vec<String>) = match in_struct {
            Some(s) => (
                format!("{s} v"),
                values
                    .iter()
                    .zip(read_fields(&f.source, s))
                    .map(|((_, t), fl)| format!("lineit_to_{}(v.{fl})", c_name(t)))
                    .collect(),
            ),
            None => (
                if ctypes.is_empty() {
                    "void".into()
                } else {
                    ctypes.iter().enumerate().map(|(k, t)| format!("{t} v{k}")).collect::<Vec<_>>().join(", ")
                },
                values.iter().enumerate().map(|(k, (_, t))| format!("lineit_to_{}(v{k})", c_name(t))).collect(),
            ),
        };
        let n = items.len();
        o.push_str(&format!(
            "static void lineit_exit_{exit}({params}) {{ lineit_value v_[{}] = {{ {} }}; lineit_fire_{h}({exit}, v_, {n}); }}\n",
            n.max(1),
            if items.is_empty() { "{0}".to_string() } else { items.join(", ") }
        ));
        exit += 1;
    }
    o.push_str(&format!("\nint lineit_call_{h}(const lineit_value *a, lineit_value *r) {{\n"));
    let n_inputs = f.inputs().count();
    if read.shared_param.is_some() {
        o.push_str("    Shared shared = {0};\n");
        for (j, ((g, _), t)) in f.shared.iter().zip(&f.shared_types).enumerate() {
            o.push_str(&format!("    shared.{g} = lineit_from_{}(&a[{}]);\n", c_name(t), n_inputs + j));
        }
    }
    let mut args = Vec::new();
    let (mut input, mut exit) = (0, 0);
    let mut ps = f.params.iter();
    for pass in &read.passes {
        match pass {
            Pass::Shared => args.push("&shared".to_string()),
            Pass::Input => {
                let p = ps.next().unwrap();
                let Role::Input(Some(t)) = &p.role else { unreachable!() };
                args.push(format!("lineit_from_{}(&a[{input}])", c_name(t)));
                input += 1;
            }
            Pass::Exit => {
                ps.next();
                args.push(format!("lineit_exit_{exit}"));
                exit += 1;
            }
        }
    }
    let call = format!("lineit_fn_{h}({})", args.join(", "));
    match &read.returns {
        Returns::Nothing => o.push_str(&format!("    {call};\n")),
        Returns::One(t) => o.push_str(&format!("    r[0] = lineit_to_{}({call});\n", c_name(t))),
        Returns::Fields(sname, fields) => {
            o.push_str(&format!("    {sname} x = {call};\n"));
            for (k, (fl, (_, t))) in fields.iter().zip(&f.returns).enumerate() {
                o.push_str(&format!("    r[{k}] = lineit_to_{}(x.{fl});\n", c_name(t)));
            }
        }
    }
    if read.shared_param.is_some() {
        for (j, ((g, _), t)) in f.shared.iter().zip(&f.shared_types).enumerate() {
            o.push_str(&format!("    r[{}] = lineit_to_{}(shared.{g});\n", f.returns.len() + j, c_name(t)));
        }
    }
    // a line printed without its end, ends with the call
    o.push_str("    if (lineit_line_len > 0) lineit_line_end();\n    return 0;\n}\n\n");
    o.push_str(&format!("void lineit_state_get_{h}(lineit_value *r) {{\n    (void)r;\n"));
    for (k, (s, t)) in read.state.iter().enumerate() {
        o.push_str(&format!("    r[{k}] = lineit_to_{}({s});\n", c_name(t)));
    }
    o.push_str(&format!("}}\n\nvoid lineit_state_set_{h}(const lineit_value *a) {{\n    (void)a;\n"));
    for (k, (s, t)) in read.state.iter().enumerate() {
        o.push_str(&format!("    {s} = lineit_from_{}(&a[{k}]);\n", c_name(t)));
    }
    o.push_str("}\n");
    Ok(o)
}

/// The fields of struct NAME of SOURCE, in order.
fn read_fields(source: &str, name: &str) -> Vec<String> {
    let Ok(tree) = super::parse(source, tree_sitter_c::LANGUAGE.into(), "C") else { return Vec::new() };
    typedefs(tree.root_node(), source)
        .into_iter()
        .find(|(n, _)| n == name)
        .map(|(_, fields)| fields.into_iter().map(|(n, _, _)| n).collect())
        .unwrap_or_default()
}

/// The Rust of the crate: `CSquare` with F's spec, around the C.
fn lib_rs(f: &Function, h: &str) -> String {
    let strs = |v: Vec<&str>| v.iter().map(|s| quoted(s)).collect::<Vec<_>>().join(", ");
    let inputs = strs(f.inputs().map(|(n, _)| n).collect());
    let shared: Vec<String> = f.shared.iter().map(|(g, s)| format!("({}, {s})", quoted(g))).collect();
    let returns = strs(f.returns.iter().map(|(n, _)| n.as_str()).collect());
    let exits: Vec<String> = f
        .exits()
        .map(|(n, v)| format!("({}, &[{}])", quoted(n), strs(v.iter().map(|(p, _)| p.as_str()).collect())))
        .collect();
    let state = strs(f.state.iter().map(String::as_str).collect());
    let then = f.ticked() && f.exits().next().is_none();
    format!(
        "use lineit_rt::cabi::{{CSquare, Lv, Spec}};
use lineit_rt::native::{{Native, Values}};

unsafe extern \"C\" {{
    fn lineit_call_{h}(a: *const Lv, r: *mut Lv) -> i32;
    fn lineit_state_get_{h}(r: *mut Lv);
    fn lineit_state_set_{h}(a: *const Lv);
}}

#[unsafe(no_mangle)]
pub extern \"C\" fn lineit_alloc_{h}(n: usize) -> *mut std::ffi::c_void {{
    lineit_rt::cabi::alloc(n)
}}

/// # Safety
/// LINE is a C string.
#[unsafe(no_mangle)]
pub unsafe extern \"C\" fn lineit_print_{h}(line: *const std::ffi::c_char) {{
    let line = unsafe {{ std::ffi::CStr::from_ptr(line) }};
    lineit_rt::ext::print(&line.to_string_lossy());
}}

/// # Safety
/// V has N values.
#[unsafe(no_mangle)]
pub unsafe extern \"C\" fn lineit_fire_{h}(exit: usize, v: *const Lv, n: usize) {{
    unsafe {{ lineit_rt::cabi::fire(exit, v, n) }}
}}

/// What the code's variables have before anything runs.
static START: std::sync::OnceLock<Vec<lineit_rt::Value>> = std::sync::OnceLock::new();

static SPEC: Spec = Spec {{
    inputs: &[{inputs}],
    shared: &[{}],
    returns: &[{returns}],
    exits: &[{}],
    state: &[{state}],
    then: {then},
}};

pub struct LineitSquare(CSquare);

impl Native for LineitSquare {{
    fn new(_: &Values) -> Self {{
        LineitSquare(CSquare::new(&SPEC, lineit_call_{h}, lineit_state_get_{h}, lineit_state_set_{h}, &START))
    }}
    fn eval(&mut self, i: &Values) -> Result<Values, String> {{
        self.0.run(i).map(|(set, _)| set)
    }}
    fn run(&mut self, _: Option<&str>, i: &Values) -> Result<(Values, Vec<(String, Values)>), String> {{
        self.0.run(i)
    }}
    fn state(&mut self) -> Values {{
        self.0.state()
    }}
    fn set_state(&mut self, s: &Values) -> Result<(), String> {{
        self.0.set_state(s)
    }}
}}

lineit_rt::export_modules!(LineitSquare);
",
        shared.join(", "),
        exits.join(", "),
    )
}

/// Writes the crate of a C code square where lineit keeps them: the code
/// (as `src/FILE`), `lineit.h`, the glue in C and in Rust, and the
/// `build.rs` that compiles the C. Returns its folder and name.
fn write_crate(f: &Function, read: &Read) -> Result<(PathBuf, String), String> {
    let file = f.origin("c", true);
    // the crate's names are its own: a program can link several
    let key = format!("{}{file}{:?}{:?}", f.source, f.shared, f.shared_types);
    let h = format!("{:016x}", crate::codegen::fnv(key.as_bytes()));
    let crate_name = format!("{}{h}", super::rust::CRATE_PREFIX);
    let dir = crate::paths::cache_dir().join("code").join(&crate_name);
    let rt = crate::codegen::write_runtime()?.to_string_lossy().replace('\\', "/");
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
            "[package]\nname = \"{crate_name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\nbuild = \"build.rs\"\n\n[lib]\ncrate-type = [\"cdylib\", \"rlib\"]\n\n[dependencies]\nlineit-rt = {{ path = \"{rt}\" }}\n\n[build-dependencies]\ncc = \"1\"\n\n[workspace]\n"
        ),
    )?;
    write(
        dir.join("build.rs"),
        &format!(
            "fn main() {{\n    println!(\"cargo:rerun-if-changed=src\");\n    cc::Build::new().file(\"src/lineit_glue.c\").include(\"src\").warnings(false).compile(\"lineit_glue_{h}\");\n}}\n"
        ),
    )?;
    write(dir.join("src/lineit.h"), &header(f, read, &h))?;
    write(dir.join("src").join(&file), &f.source)?;
    write(dir.join("src/lineit_glue.c"), &glue_c(f, &file, &h)?)?;
    write(dir.join("src/lib.rs"), &lib_rs(f, &h))?;
    super::crate_used(&dir);
    Ok((dir, crate_name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lineit_rt::RecordType;
    use std::sync::Arc;

    struct Cx;
    impl SigCx for Cx {
        fn var(&self, name: &str) -> Option<Type> {
            (name == "::total").then_some(Type::Int)
        }
        fn record(&self, name: &str) -> Option<Arc<RecordType>> {
            (name == "point").then(|| {
                Arc::new(RecordType {
                    name: "point".into(),
                    fields: vec![("x".into(), Type::Float), ("y".into(), Type::Float)],
                })
            })
        }
        fn read(&self, path: &str) -> Result<String, String> {
            Err(format!("{path}: no files here"))
        }
    }

    const COLLATZ: &str = "static int64_t count = 0;

typedef struct { int64_t calls; int64_t n; } Odd;

/** Halves even numbers; counts and passes on the odd ones. */
void collatz(int64_t n, Shared *shared, void (*even)(int64_t half), void (*odd)(Odd v)) {
    shared->total += n;
    if (n % 2 == 0) { even(n / 2); return; }
    count++;
    odd((Odd){ count, n });
}
";

    #[test]
    fn reads() {
        let (f, r) = read(COLLATZ, &Cx).unwrap();
        assert_eq!(f.name, "collatz");
        assert_eq!(f.doc, "Halves even numbers; counts and passes on the odd ones.");
        assert_eq!(f.params[1].role, Role::Exit(vec![("half".into(), Type::Int)]));
        assert_eq!(f.params[2].role, Role::Exit(vec![("calls".into(), Type::Int), ("n".into(), Type::Int)]));
        assert_eq!(f.state, ["count"]);
        assert_eq!(f.shared, [("total".to_string(), true)]);
        assert_eq!(r.passes, [Pass::Input, Pass::Shared, Pass::Exit, Pass::Exit]);

        let (f, _) = read(
            "static const int K = 2;\nstatic int64_t h(int64_t x) { return x * K; }\nlist_float scale(list_float xs, double k, const char *name, point p, map_list_str m) { return xs; }",
            &Cx,
        )
        .unwrap();
        assert!(!f.ticked());
        assert_eq!(f.returns, [("result".to_string(), Type::List(Box::new(Type::Float)))]);
        assert_eq!(f.params[2].role, Role::Input(Some(Type::Str)));
        assert!(matches!(&f.params[3].role, Role::Input(Some(Type::Record(r))) if r.name == "point"));
    }

    #[test]
    fn errors() {
        let err = |s: &str| read(s, &Cx).unwrap_err();
        assert!(err("static void f(void) {}").contains("isn't `static`"));
        assert!(err("void f(void) {}\nvoid g(void) {}").contains("f, g aren't `static`"));
        assert!(err("void f(int x) {}").contains("int64_t and double"));
        assert!(err("void f(foo x) {}").contains("`foo` isn't a type"));
        assert!(err("static list_int seen;\nvoid f(void) {}").contains("int64_t, double or bool"));
        assert!(err("void f(int64_t x {}").contains("line 1"));
    }
}

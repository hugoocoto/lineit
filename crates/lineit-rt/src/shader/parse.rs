//! From the text of a shader to a [`Program`]: the text goes through the
//! preprocessor (`prep.rs`), the tree-sitter grammar reads it, and this
//! turns the tree into expressions and statements where every name is a
//! slot, so running it looks up nothing.

use std::collections::HashMap;

use tree_sitter::{Node, Parser};

use super::ast::*;
use super::builtins;
use super::eval::BUILTINS;
use super::prep;
use super::val::*;

pub fn parse(src: &str) -> R<Program> {
    let text = prep::run(src)?;
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_gdshader::LANGUAGE.into())
        .map_err(|e| format!("the shader grammar doesn't load: {e}"))?;
    let tree = parser.parse(&text, None).ok_or("the shader couldn't be read")?;
    let root = tree.root_node();
    if root.has_error() {
        let e = find_error(root);
        let line = e.start_position().row;
        let near = text.lines().nth(line).unwrap_or("").trim();
        return Err(format!("line {}: can't read this ({near})", line + 1));
    }
    let mut l = Lower::new(&text);
    l.program(root)
}

/// The number in `[3]`: the grammar puts the brackets under the same field.
fn size_node(n: Node) -> Option<Node> {
    let mut c = n.walk();
    n.children_by_field_name("size", &mut c).find(|x| x.is_named())
}

fn find_error(n: Node) -> Node {
    if n.is_error() || n.is_missing() {
        return n;
    }
    let mut c = n.walk();
    for ch in n.children(&mut c) {
        if ch.has_error() {
            return find_error(ch);
        }
    }
    n
}

struct FnSig {
    name: String,
    index: usize,
}

struct Lower<'s> {
    src: &'s str,
    structs: Vec<StructDef>,
    struct_names: HashMap<String, usize>,
    funcs: Vec<Func>,
    sigs: Vec<FnSig>,
    globals: HashMap<String, usize>,
    nglobals: usize,
    const_ints: HashMap<String, i64>,
    scopes: Vec<HashMap<String, usize>>,
    slots: usize,
}

impl<'s> Lower<'s> {
    fn new(src: &'s str) -> Lower<'s> {
        let globals = BUILTINS.iter().enumerate().map(|(i, n)| (n.to_string(), i)).collect();
        Lower {
            src,
            structs: Vec::new(),
            struct_names: HashMap::new(),
            funcs: Vec::new(),
            sigs: Vec::new(),
            globals,
            nglobals: BUILTINS.len(),
            const_ints: HashMap::new(),
            scopes: Vec::new(),
            slots: 0,
        }
    }

    fn text(&self, n: Node) -> &'s str {
        n.utf8_text(self.src.as_bytes()).unwrap_or("")
    }

    fn err<T>(&self, n: Node, msg: impl std::fmt::Display) -> R<T> {
        Err(format!("line {}: {msg}", n.start_position().row + 1))
    }

    fn program(&mut self, root: Node) -> R<Program> {
        let mut c = root.walk();
        let items: Vec<Node> = root.named_children(&mut c).collect();

        // structs, then the names of everything global
        let mut globals = Vec::new();
        let mut mutable = Vec::new();
        let mut uniforms = Vec::new();
        let mut fn_nodes = Vec::new();
        for n in &items {
            match n.kind() {
                "shader_type_statement" => {
                    let t = self.text(*n);
                    if !t.contains("canvas_item") {
                        return self.err(*n, format!("only canvas_item shaders are supported ({})", t.trim()));
                    }
                }
                "struct_definition" => self.struct_def(*n)?,
                _ => {}
            }
        }
        for n in &items {
            match n.kind() {
                "const_declaration" | "variable_declaration" | "varying_declaration" => {
                    let ty = self.ty(n.child_by_field_name("type").ok_or("a declaration without a type")?)?;
                    let mut cur = n.walk();
                    for d in n.children_by_field_name("declarator", &mut cur) {
                        let (name, ty, init) = self.declarator(d, &ty)?;
                        if n.kind() == "const_declaration" && matches!(ty, Ty::Vec(Sc::Int, 1)) {
                            if let Some(i) = init {
                                if let Ok(v) = self.const_int(i) {
                                    self.const_ints.insert(name.clone(), v as i64);
                                }
                            }
                        }
                        let slot = self.global(&name);
                        if n.kind() != "const_declaration" {
                            mutable.push(slot);
                        }
                        globals.push((slot, ty, init));
                    }
                }
                "uniform_declaration" => {
                    let ty = self.ty(n.child_by_field_name("type").ok_or("a uniform without a type")?)?;
                    let d = n.child_by_field_name("declarator").ok_or("a uniform without a name")?;
                    if d.kind() != "identifier" {
                        return self.err(d, "uniform arrays are not supported");
                    }
                    let name = self.text(d).to_string();
                    let slot = self.global(&name);
                    uniforms.push((*n, name, slot, ty));
                }
                "function_definition" => {
                    let node = *n;
                    let idx = self.signature(node)?;
                    fn_nodes.push((idx, node));
                }
                _ => {}
            }
        }

        // now that everything has a name: the values and the bodies
        let mut init = Vec::new();
        for (slot, ty, node) in globals {
            let e = match node {
                Some(v) => Some(self.expr(v)?),
                None => None,
            };
            init.push((slot, ty, e));
        }
        let mut decls = Vec::new();
        for (n, name, slot, ty) in uniforms {
            let value = match n.child_by_field_name("value") {
                Some(v) => Some(self.expr(v)?),
                None => None,
            };
            let mut u = UniformDecl {
                name,
                slot,
                ty,
                init: value,
                color: false,
                filter_nearest: false,
                repeat: false,
                default_white: false,
            };
            if let Some(h) = n.child_by_field_name("hint") {
                let mut cur = h.walk();
                for x in h.named_children(&mut cur) {
                    match self.text(x).trim() {
                        "source_color" => u.color = true,
                        "filter_nearest" | "filter_nearest_mipmap" | "filter_nearest_mipmap_anisotropic" => {
                            u.filter_nearest = true
                        }
                        "repeat_enable" => u.repeat = true,
                        "hint_default_white" => u.default_white = true,
                        _ => {}
                    }
                }
            }
            decls.push(u);
        }
        let mut fragment = None;
        for (idx, node) in fn_nodes {
            let name = self.funcs[idx].name.clone();
            // only `fragment` is run; the others (vertex, light...) are
            // for what a GPU does with meshes and lights
            if matches!(name.as_str(), "vertex" | "light" | "start" | "process" | "sky" | "fog") {
                continue;
            }
            self.func_body(idx, node)?;
            if name == "fragment" {
                fragment = Some(idx);
            }
        }
        Ok(Program {
            structs: std::mem::take(&mut self.structs),
            funcs: std::mem::take(&mut self.funcs),
            fragment,
            globals: self.nglobals,
            init,
            mutable,
            uniforms: decls,
        })
    }

    fn global(&mut self, name: &str) -> usize {
        let slot = self.nglobals;
        self.nglobals += 1;
        self.globals.insert(name.to_string(), slot);
        slot
    }

    fn struct_def(&mut self, n: Node) -> R<()> {
        let name = self.text(n.child_by_field_name("name").ok_or("a struct without a name")?).to_string();
        let mut fields = Vec::new();
        if let Some(fs) = n.child_by_field_name("fields") {
            let mut c = fs.walk();
            for f in fs.named_children(&mut c) {
                if f.kind() != "field_definition" {
                    continue;
                }
                let base = self.ty(f.child_by_field_name("type").ok_or("a field without a type")?)?;
                let mut cur = f.walk();
                for d in f.children_by_field_name("declarator", &mut cur) {
                    let (fname, ty, _) = self.declarator(d, &base)?;
                    fields.push((fname, ty));
                }
            }
        }
        self.struct_names.insert(name.clone(), self.structs.len());
        self.structs.push(StructDef { name, fields });
        Ok(())
    }

    /// The type a node says.
    fn ty(&self, n: Node) -> R<Ty> {
        match n.kind() {
            "type_specifier" => {
                let inner = n.child_by_field_name("type").ok_or("a type without its name")?;
                let t = self.ty(inner)?;
                match size_node(n) {
                    Some(s) => Ok(Ty::Array(Box::new(t), self.const_int(s)?)),
                    None => Ok(t),
                }
            }
            "primitive_type" => {
                let t = self.text(n);
                Ok(match t {
                    "void" => Ty::Void,
                    "bool" => Ty::Vec(Sc::Bool, 1),
                    "int" => Ty::Vec(Sc::Int, 1),
                    "uint" => Ty::Vec(Sc::UInt, 1),
                    "float" => Ty::Vec(Sc::Float, 1),
                    "sampler2D" | "isampler2D" | "usampler2D" => Ty::Sampler,
                    _ => {
                        let (prefix, size) = t.split_at(t.len().saturating_sub(1));
                        let n_ = size.parse::<u8>().unwrap_or(0);
                        match (prefix, n_) {
                            ("bvec", 2..=4) => Ty::Vec(Sc::Bool, n_),
                            ("ivec", 2..=4) => Ty::Vec(Sc::Int, n_),
                            ("uvec", 2..=4) => Ty::Vec(Sc::UInt, n_),
                            ("vec", 2..=4) => Ty::Vec(Sc::Float, n_),
                            ("mat", 2..=4) => Ty::Mat(n_),
                            _ => return self.err(n, format!("the type {t} is not supported")),
                        }
                    }
                })
            }
            "type_identifier" => match self.struct_names.get(self.text(n)) {
                Some(i) => Ok(Ty::Struct(*i)),
                None => self.err(n, format!("unknown type {}", self.text(n))),
            },
            k => self.err(n, format!("can't read a type from a {k}")),
        }
    }

    /// A number written in the code that has to be known now: an array size.
    fn const_int(&self, n: Node) -> R<usize> {
        match n.kind() {
            "signed_integer" | "unsigned_integer" => match self.int_literal(n) {
                Ok(v) if v >= 0 => Ok(v as usize),
                _ => self.err(n, "a size has to be a positive number"),
            },
            "identifier" => match self.const_ints.get(self.text(n)) {
                Some(v) if *v >= 0 => Ok(*v as usize),
                _ => self.err(n, format!("{} is not a constant number", self.text(n))),
            },
            "parenthical_expression" => match n.named_child(0) {
                Some(c) => self.const_int(c),
                None => self.err(n, "an empty ()"),
            },
            _ => self.err(n, "a size has to be a number"),
        }
    }

    /// A declarator (`a`, `a = 1.0`, `a[3]`, `a[3] = ...`) of a declaration
    /// of type BASE: its name, its type and its value, if any.
    fn declarator<'t>(&self, d: Node<'t>, base: &Ty) -> R<(String, Ty, Option<Node<'t>>)> {
        match d.kind() {
            "identifier" => Ok((self.text(d).to_string(), base.clone(), None)),
            "array_declarator" => {
                let name = d.child_by_field_name("declarator").ok_or("an array without a name")?;
                let size = match size_node(d) {
                    Some(s) => self.const_int(s)?,
                    None => 0,
                };
                Ok((self.text(name).to_string(), Ty::Array(Box::new(base.clone()), size), None))
            }
            "init_declarator" => {
                let inner = d.child_by_field_name("declarator").ok_or("a declaration without a name")?;
                let (name, ty, _) = self.declarator(inner, base)?;
                Ok((name, ty, d.child_by_field_name("value")))
            }
            k => self.err(d, format!("can't read a declaration from a {k}")),
        }
    }

    fn signature(&mut self, n: Node) -> R<usize> {
        let name = self.text(n.child_by_field_name("declarator").ok_or("a function without a name")?).to_string();
        let ret = self.ty(n.child_by_field_name("type").ok_or("a function without a type")?)?;
        let mut params = Vec::new();
        if let Some(pl) = n.child_by_field_name("parameters") {
            let mut c = pl.walk();
            for p in pl.named_children(&mut c) {
                if p.kind() != "parameter_declaration" {
                    continue;
                }
                let base = self.ty(p.child_by_field_name("type").ok_or("a parameter without a type")?)?;
                let d = p.child_by_field_name("declarator").ok_or("a parameter without a name")?;
                let (_, ty, _) = self.declarator(d, &base)?;
                let mut dir = Dir::In;
                let mut cur = p.walk();
                for q in p.children_by_field_name("parameter_qualifier", &mut cur) {
                    match self.text(q).trim() {
                        "out" => dir = Dir::Out,
                        "inout" => dir = Dir::InOut,
                        _ => {}
                    }
                }
                params.push(Param { slot: params.len(), ty, dir });
            }
        }
        let index = self.funcs.len();
        self.funcs.push(Func { name: name.clone(), params, ret, body: Vec::new(), slots: 0 });
        self.sigs.push(FnSig { name, index });
        Ok(index)
    }

    fn func_body(&mut self, idx: usize, n: Node) -> R<()> {
        let names: Vec<String> = {
            let pl = n.child_by_field_name("parameters");
            let mut v = Vec::new();
            if let Some(pl) = pl {
                let mut c = pl.walk();
                for p in pl.named_children(&mut c) {
                    if p.kind() == "parameter_declaration" {
                        let d = p.child_by_field_name("declarator").ok_or("a parameter without a name")?;
                        let name = if d.kind() == "array_declarator" {
                            d.child_by_field_name("declarator").map(|x| self.text(x)).unwrap_or("")
                        } else {
                            self.text(d)
                        };
                        v.push(name.to_string());
                    }
                }
            }
            v
        };
        self.scopes = vec![names.iter().enumerate().map(|(i, n)| (n.clone(), i)).collect()];
        self.slots = names.len();
        let block = n.child_by_field_name("block").ok_or("a function without a body")?;
        let body = self.body(block)?;
        let f = &mut self.funcs[idx];
        f.body = body;
        f.slots = self.slots;
        Ok(())
    }

    // -- statements

    /// The statements of N (a block, or one statement), in a scope of
    /// their own.
    fn body(&mut self, n: Node) -> R<Vec<S>> {
        self.scopes.push(HashMap::new());
        let r = self.body_in(n);
        self.scopes.pop();
        r
    }

    fn body_in(&mut self, n: Node) -> R<Vec<S>> {
        if n.kind() == "compound_statement" {
            let mut c = n.walk();
            let nodes: Vec<Node> = n.named_children(&mut c).collect();
            self.stmt_list(&nodes)
        } else {
            self.stmt_list(&[n])
        }
    }

    /// Statements in a row. This grammar ends a `for` at its `)`: the
    /// statement after it is its body.
    fn stmt_list(&mut self, nodes: &[Node]) -> R<Vec<S>> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < nodes.len() {
            let n = nodes[i];
            i += 1;
            if n.kind() == "comment" {
                continue;
            }
            if n.kind() != "for_statement" {
                out.push(self.stmt(n)?);
                continue;
            }
            while i < nodes.len() && nodes[i].kind() == "comment" {
                i += 1;
            }
            let Some(body) = nodes.get(i) else { return self.err(n, "a for without a body") };
            i += 1;
            out.push(self.for_stmt(n, *body)?);
        }
        Ok(out)
    }

    fn for_stmt(&mut self, n: Node, body: Node) -> R<S> {
        self.scopes.push(HashMap::new());
        let r = (|| -> R<S> {
            let init = match n.child_by_field_name("initializer") {
                Some(i) => vec![self.stmt(i)?],
                None => Vec::new(),
            };
            let cond = match n.child_by_field_name("condition") {
                Some(c) => Some(self.expr(c)?),
                None => None,
            };
            let update = match n.child_by_field_name("update") {
                Some(u) => Some(self.expr(u)?),
                None => None,
            };
            let body = self.body(body)?;
            Ok(S::For(init, cond, update, body))
        })();
        self.scopes.pop();
        r
    }

    fn local(&mut self, name: &str) -> usize {
        let slot = self.slots;
        self.slots += 1;
        if let Some(s) = self.scopes.last_mut() {
            s.insert(name.to_string(), slot);
        }
        slot
    }

    fn stmt(&mut self, n: Node) -> R<S> {
        match n.kind() {
            "compound_statement" => Ok(S::Block(self.body(n)?)),
            "expression_statement" => match n.named_child(0) {
                Some(e) => Ok(S::Expr(self.expr(e)?)),
                None => Ok(S::Block(Vec::new())),
            },
            "variable_declaration" | "const_declaration" => {
                let base = self.ty(n.child_by_field_name("type").ok_or("a declaration without a type")?)?;
                let mut out = Vec::new();
                let mut cur = n.walk();
                for d in n.children_by_field_name("declarator", &mut cur) {
                    let (name, ty, init) = self.declarator(d, &base)?;
                    // the value can't see the name it declares
                    let init = match init {
                        Some(v) => Some(self.expr(v)?),
                        None => None,
                    };
                    let slot = self.local(&name);
                    out.push(S::Decl(slot, ty, init));
                }
                Ok(S::Block(out))
            }
            "if_statement" => {
                let c = self.cond(n)?;
                let a = self.body(n.child_by_field_name("consequence").ok_or("an if without a body")?)?;
                let b = match n.child_by_field_name("alternative") {
                    Some(alt) => {
                        // an else clause holds the statement
                        let inner = if alt.kind() == "else_clause" { alt.named_child(0) } else { Some(alt) };
                        match inner {
                            Some(i) => self.body(i)?,
                            None => Vec::new(),
                        }
                    }
                    None => Vec::new(),
                };
                Ok(S::If(c, a, b))
            }
            "for_statement" => self.err(n, "a for here needs its body in { }"),
            "while_statement" => {
                let c = self.cond(n)?;
                Ok(S::While(c, self.body(n.child_by_field_name("consequence").ok_or("a while without a body")?)?))
            }
            "do_statement" => {
                let body = self.body(n.child_by_field_name("consequence").ok_or("a do without a body")?)?;
                Ok(S::Do(body, self.cond(n)?))
            }
            "switch_statement" => {
                let c = self.cond(n)?;
                let block = n.child_by_field_name("block").ok_or("a switch without a body")?;
                let mut cases = Vec::new();
                self.scopes.push(HashMap::new());
                let mut cur = block.walk();
                for cs in block.named_children(&mut cur) {
                    if cs.kind() != "case_statement" {
                        continue;
                    }
                    let value = match cs.child_by_field_name("value") {
                        Some(v) => Some(self.expr(v)?),
                        None => None,
                    };
                    let mut c2 = cs.walk();
                    let nodes: Vec<Node> =
                        cs.named_children(&mut c2).filter(|s| cs.child_by_field_name("value") != Some(*s)).collect();
                    let body = self.stmt_list(&nodes)?;
                    cases.push(Case { value, body });
                }
                self.scopes.pop();
                Ok(S::Switch(c, cases))
            }
            "break_statement" => Ok(S::Break),
            "continue_statement" => Ok(S::Continue),
            "discard_statement" => Ok(S::Discard),
            "return_statement" => match n.child_by_field_name("value") {
                Some(v) => Ok(S::Return(Some(self.expr(v)?))),
                None => Ok(S::Return(None)),
            },
            "comment" => Ok(S::Block(Vec::new())),
            k => self.err(n, format!("this statement ({k}) is not supported")),
        }
    }

    /// The condition of an if, while or do.
    fn cond(&mut self, n: Node) -> R<E> {
        let c = n.child_by_field_name("condition").ok_or("a statement without a condition")?;
        self.expr(c)
    }

    // -- expressions

    fn lookup(&self, name: &str) -> Option<E> {
        for s in self.scopes.iter().rev() {
            if let Some(i) = s.get(name) {
                return Some(E::Local(*i));
            }
        }
        self.globals.get(name).map(|i| E::Global(*i))
    }

    fn int_literal(&self, n: Node) -> R<i64> {
        let t = self.text(n).trim();
        let (neg, t) = match t.strip_prefix('-') {
            Some(r) => (true, r),
            None => (false, t.strip_prefix('+').unwrap_or(t)),
        };
        let t = t.trim_end_matches(['u', 'U']);
        let v = if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
            i64::from_str_radix(h, 16)
        } else {
            t.parse::<i64>()
        };
        match v {
            Ok(v) => Ok(if neg { -v } else { v }),
            Err(_) => self.err(n, format!("can't read the number {t}")),
        }
    }

    fn expr(&mut self, n: Node) -> R<E> {
        match n.kind() {
            "float" => {
                let t = self.text(n).trim().trim_end_matches(['f', 'F']);
                match t.parse::<f32>() {
                    Ok(v) => Ok(E::Lit(V::float(v))),
                    Err(_) => self.err(n, format!("can't read the number {t}")),
                }
            }
            "signed_integer" => Ok(E::Lit(V::int(self.int_literal(n)? as i32))),
            "unsigned_integer" => Ok(E::Lit(V::uint(self.int_literal(n)? as u32))),
            "boolean" => Ok(E::Lit(V::boolean(self.text(n).trim() == "true"))),
            "identifier" => {
                let name = self.text(n);
                match self.lookup(name) {
                    Some(e) => Ok(e),
                    None => self.err(n, format!("unknown name {name}")),
                }
            }
            "parenthical_expression" => match n.named_child(0) {
                Some(c) => self.expr(c),
                None => self.err(n, "an empty ()"),
            },
            "unary_expression" => {
                let op = match self.text(n.child_by_field_name("operator").ok_or("an operator is missing")?).trim() {
                    "-" => Un::Neg,
                    "+" => Un::Plus,
                    "!" => Un::Not,
                    "~" => Un::BitNot,
                    o => return self.err(n, format!("the operator {o} is not supported")),
                };
                let a = self.expr(n.child_by_field_name("argument").ok_or("an operand is missing")?)?;
                if let E::Lit(v) = &a {
                    if let Ok(r) = unary(op, v) {
                        return Ok(E::Lit(r));
                    }
                }
                Ok(E::Un(op, Box::new(a)))
            }
            "binary_expression" => {
                let op = self.bin_op(n)?;
                let a = self.expr(n.child_by_field_name("left").ok_or("an operand is missing")?)?;
                let b = self.expr(n.child_by_field_name("right").ok_or("an operand is missing")?)?;
                if let (E::Lit(x), E::Lit(y)) = (&a, &b) {
                    if !matches!(op, Bin::And | Bin::Or | Bin::Eq | Bin::Ne) {
                        if let Ok(r) = binary(op, x, y) {
                            return Ok(E::Lit(r));
                        }
                    }
                }
                Ok(E::Bin(op, Box::new(a), Box::new(b)))
            }
            "ternary_expression" => {
                let c = self.expr(n.child_by_field_name("condition").ok_or("a condition is missing")?)?;
                let a = self.expr(n.child_by_field_name("consequence").ok_or("a value is missing")?)?;
                let b = self.expr(n.child_by_field_name("alternative").ok_or("a value is missing")?)?;
                Ok(E::Cond(Box::new(c), Box::new(a), Box::new(b)))
            }
            "assignment_expression" => {
                let op = match self.text(n.child_by_field_name("operator").ok_or("an operator is missing")?).trim() {
                    "=" => None,
                    "+=" => Some(Bin::Add),
                    "-=" => Some(Bin::Sub),
                    "*=" => Some(Bin::Mul),
                    "/=" => Some(Bin::Div),
                    "%=" => Some(Bin::Rem),
                    "<<=" => Some(Bin::Shl),
                    ">>=" => Some(Bin::Shr),
                    "&=" => Some(Bin::BitAnd),
                    "|=" => Some(Bin::BitOr),
                    "^=" => Some(Bin::BitXor),
                    o => return self.err(n, format!("the operator {o} is not supported")),
                };
                let l = self.expr(n.child_by_field_name("left").ok_or("an operand is missing")?)?;
                let r = self.expr(n.child_by_field_name("right").ok_or("an operand is missing")?)?;
                Ok(E::Assign(Box::new(l), op, Box::new(r)))
            }
            "update_expression" => {
                let op = n.child_by_field_name("operator").ok_or("an operator is missing")?;
                let arg = n.child_by_field_name("argument").ok_or("an operand is missing")?;
                let delta = if self.text(op).trim() == "++" { 1.0 } else { -1.0 };
                let post = arg.start_byte() < op.start_byte();
                Ok(E::IncDec(Box::new(self.expr(arg)?), delta, post))
            }
            "comma_expression" => {
                let mut c = n.walk();
                let list = n.named_children(&mut c).map(|x| self.expr(x)).collect::<R<Vec<_>>>()?;
                Ok(E::Comma(list))
            }
            "field_expression" => {
                let base = self.expr(n.child_by_field_name("argument").ok_or("a field of nothing")?)?;
                let name = self.text(n.child_by_field_name("field").ok_or("a field without a name")?).to_string();
                let swz = swizzle(&name);
                Ok(E::Field(Box::new(base), Field { name, swz }))
            }
            "subscript_expression" => {
                let b = self.expr(n.child_by_field_name("argument").ok_or("an index of nothing")?)?;
                let i = self.expr(n.child_by_field_name("index").ok_or("an index is missing")?)?;
                Ok(E::Index(Box::new(b), Box::new(i)))
            }
            "method_expression" => {
                // only `array.length()`
                let m = self.text(n.child_by_field_name("method").ok_or("a method without a name")?);
                if m != "length" {
                    return self.err(n, format!("the method {m} is not supported"));
                }
                let a = self.expr(n.child_by_field_name("argument").ok_or("a method of nothing")?)?;
                Ok(E::Call(Callee::Builtin(builtins::array_length), vec![a]))
            }
            "call_expression" => self.call(n),
            k => self.err(n, format!("this ({k}) is not supported")),
        }
    }

    fn bin_op(&self, n: Node) -> R<Bin> {
        let op = self.text(n.child_by_field_name("operator").ok_or("an operator is missing")?).trim();
        Ok(match op {
            "+" => Bin::Add,
            "-" => Bin::Sub,
            "*" => Bin::Mul,
            "/" => Bin::Div,
            "%" => Bin::Rem,
            "<<" => Bin::Shl,
            ">>" => Bin::Shr,
            "<" => Bin::Lt,
            "<=" => Bin::Le,
            ">" => Bin::Gt,
            ">=" => Bin::Ge,
            "==" => Bin::Eq,
            "!=" => Bin::Ne,
            "&" => Bin::BitAnd,
            "|" => Bin::BitOr,
            "^" => Bin::BitXor,
            "&&" => Bin::And,
            "||" => Bin::Or,
            "^^" => Bin::Xor,
            o => return self.err(n, format!("the operator {o} is not supported")),
        })
    }

    fn call(&mut self, n: Node) -> R<E> {
        let f = n.child_by_field_name("function").ok_or("a call without a function")?;
        let mut args = Vec::new();
        if let Some(al) = n.child_by_field_name("arguments") {
            let mut c = al.walk();
            for a in al.named_children(&mut c) {
                if a.kind() != "comment" {
                    args.push(self.expr(a)?);
                }
            }
        }
        let name = self.text(f).trim();
        match f.kind() {
            "primitive_type" | "type_identifier" => {
                let t = self.ty(f)?;
                // `float[3](...)` and `float[](...)` are arrays
                let mut c = n.walk();
                let is_array = n.children(&mut c).any(|x| !x.is_named() && self.text(x) == "[");
                let ty = if is_array {
                    let size = match size_node(n) {
                        Some(s) => self.const_int(s)?,
                        None => 0,
                    };
                    Ty::Array(Box::new(t), size)
                } else {
                    t
                };
                Ok(E::Call(Callee::Ctor(ty), args))
            }
            "identifier" => {
                if let Some(s) =
                    self.sigs.iter().find(|s| s.name == name && self.funcs[s.index].params.len() == args.len())
                {
                    return Ok(E::Call(Callee::User(s.index), args));
                }
                if let Some(i) = self.struct_names.get(name) {
                    return Ok(E::Call(Callee::Ctor(Ty::Struct(*i)), args));
                }
                match builtins::lookup(name) {
                    Some(b) => Ok(E::Call(Callee::Builtin(b), args)),
                    None => self.err(n, format!("unknown function {name}")),
                }
            }
            k => self.err(n, format!("can't call a {k}")),
        }
    }
}

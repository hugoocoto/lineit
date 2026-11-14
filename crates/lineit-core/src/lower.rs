//! From the canvas to the IR, in four steps:
//!
//! 1. **Flatten**: subgraph squares are replaced by the squares inside them
//!    (each copy in a scope of its own, for its variables), and wires
//!    through `graph/input` and `graph/output` ports are joined up.
//! 2. **Signatures**: every square's pins, from its params (and the types of
//!    the variables it names).
//! 3. **Types** ([`crate::types`]): what flows on every wire.
//! 4. **Lowering**: each module turns its square into IR through [`Lx`].
//!    Data inputs become the expressions of what's wired to them; each tick
//!    input something fires becomes a handler.
//!
//! Anything wrong is a [`Diag`] on the square of the canvas it's in (the
//! subgraph square, for a square inside one), and there's no program.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use lineit_rt::{RecordType, Type, Value};

use crate::ir::*;
use crate::model::{Canvas, Link, Square, SquareId};
use crate::registry::{Module, Params, PinTy, Registry, SigCx, Signature};

#[derive(Clone, Debug, PartialEq)]
pub struct Diag {
    /// The square of the canvas it's about, if any.
    pub square: Option<SquareId>,
    pub message: String,
}

/// About how tall a square is drawn, in canvas units (the editor has the
/// exact numbers): for laying out canvases written by hand.
pub fn square_height(info: &SquareInfo, params: usize) -> f32 {
    let rows = info.sig.inputs.len().max(info.sig.outputs.len()) + params;
    let preview = if info.out_types.iter().any(|t| t.as_ref() == Some(&Type::Image)) { 170.0 } else { 0.0 };
    26.0 + 22.0 * rows as f32 + 10.0 + preview
}

/// Lays out CANVAS with its squares' real heights.
pub fn arrange(canvas: &mut Canvas, reg: &Registry) {
    let l = lower(canvas, reg);
    // a code square's code, as lowering read it (from its file, too): it's
    // drawn below its other params, a line each, as wide as its widest
    let code = |sq: &Square| -> Option<String> {
        let info = l.squares.get(&sq.id)?;
        info.sig.code.as_ref().map(|f| f.source.trim_end().to_string())
    };
    canvas.arrange(
        &|sq| {
            let mut params = reg.get(&sq.module).map(|m| m.params().len()).unwrap_or(0);
            // as the editor draws it: its code and tests aren't rows of
            // params; its code is, a line each (14), with its line of tests
            // below (from a file: a line that says so, and a blank one)
            let extra = code(sq).map_or(0.0, |c| {
                params = params.saturating_sub(2);
                let from_file = sq.params.get("file").is_some_and(|f| !f.trim().is_empty());
                let lines = c.lines().count() + if from_file { 2 } else { 0 };
                lines.max(2) as f32 * 14.0 + 12.0 + 22.0
            });
            l.squares.get(&sq.id).map(|i| square_height(i, params) + extra)
        },
        &|sq| {
            code(sq).map(|c| (c.lines().map(|l| l.chars().count()).max().unwrap_or(0) as f32 * 7.3 + 24.0).min(640.0))
        },
    );
}

/// What the editor shows of a square: its pins and their types.
#[derive(Clone, Debug)]
pub struct SquareInfo {
    pub sig: Signature,
    pub in_types: Vec<Option<Type>>,
    pub out_types: Vec<Option<Type>>,
}

pub struct Lowered {
    /// None if there are diagnostics.
    pub program: Option<Program>,
    pub diags: Vec<Diag>,
    /// The wires of the canvas that go into a data input with more than
    /// one: the editor marks them.
    pub double: Vec<Link>,
    pub squares: HashMap<SquareId, SquareInfo>,
    /// The expressions of the images that pure squares on the canvas make,
    /// for the editor to show without running anything.
    pub previews: Vec<(SquareId, String, Expr)>,
    /// The files the signatures were worked out from, and when each was
    /// changed (None: it wasn't there).
    pub deps: Vec<(std::path::PathBuf, Option<std::time::SystemTime>)>,
}

/// Where a data input gets its value.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// Nothing: the type's default.
    None,
    /// Output (node, output).
    Wire(NodeId, usize),
    /// Typed in a field: the text, and the type of the subgraph port it
    /// came in through, if it did.
    Field(String, Option<Type>),
}

pub const GLOBAL_SCOPE: usize = usize::MAX;

/// A square of the flattened canvas.
pub struct FNode {
    pub module: Arc<dyn Module>,
    pub square: SquareId,
    /// Whether it's on the canvas itself, not inside a subgraph.
    pub top: bool,
    pub params: BTreeMap<String, String>,
    pub scope: usize,
    pub sig: Signature,
    pub inputs: Vec<Source>,
    /// Per tick output, the (node, tick input)s it fires.
    pub targets: Vec<Vec<(NodeId, usize)>>,
    pub in_types: Vec<Option<Type>>,
    pub out_types: Vec<Option<Type>>,
}

#[derive(Clone, Debug)]
pub struct VarDecl {
    pub ty: Type,
    pub init: String,
}

// ---------------------------------------------------------------- flatten

enum GKind {
    Real(Arc<dyn Module>),
    /// A `graph/input` or `graph/output`: what comes into its `value` goes
    /// out of its `value`.
    Pass,
}

struct GNode {
    kind: GKind,
    /// For a port: its type, if it's data.
    port_ty: Option<Type>,
    /// For a port: its name.
    port: String,
    square: SquareId,
    top: bool,
    scope: usize,
    params: BTreeMap<String, String>,
    fields: BTreeMap<String, String>,
    /// (input, source node, source output, the wire's place in the main
    /// canvas's links, if it's one of them)
    incoming: Vec<(String, usize, String, Option<usize>)>,
}

enum Mapping {
    Real(usize),
    Sub { inputs: HashMap<String, usize>, outputs: HashMap<String, usize> },
}

struct Flattener<'a> {
    reg: &'a Registry,
    g: Vec<GNode>,
    /// (subgraph square on the canvas, output, its graph/output port)
    top_subs: Vec<(SquareId, String, usize)>,
    scopes: usize,
    diags: Vec<Diag>,
}

struct Ports {
    inputs: HashMap<String, usize>,
    outputs: HashMap<String, usize>,
}

impl Flattener<'_> {
    /// Adds the squares of CANVAS. TOP is the square of the main canvas
    /// it's inside of, if it's a subgraph's; STACK the subgraphs it's in.
    fn expand(&mut self, canvas: &Canvas, scope: usize, top: Option<SquareId>, stack: &mut Vec<String>) -> Ports {
        let mut map: HashMap<SquareId, Mapping> = HashMap::new();
        let mut ports = Ports { inputs: HashMap::new(), outputs: HashMap::new() };
        for sq in &canvas.squares {
            let at = top.unwrap_or(sq.id);
            let Some(m) = self.reg.get(&sq.module) else {
                self.diags.push(Diag { square: Some(at), message: format!("there's no module `{}`", sq.module) });
                continue;
            };
            let gnode = |kind| GNode {
                kind,
                port_ty: None,
                port: String::new(),
                square: at,
                top: top.is_none(),
                scope,
                params: sq.params.clone(),
                fields: sq.fields.clone(),
                incoming: Vec::new(),
            };
            if sq.module == "graph/input" || sq.module == "graph/output" {
                let p = Params { defs: m.params(), values: &sq.params };
                let mut n = gnode(GKind::Pass);
                n.fields = BTreeMap::new();
                n.port_ty = Type::parse(&p.str("type"), &|r| self.reg.records.get(r).cloned());
                n.port = p.str("name");
                if sq.module == "graph/input" {
                    n.fields.insert("value".into(), p.str("default"));
                    ports.inputs.insert(p.str("name"), self.g.len());
                } else {
                    ports.outputs.insert(p.str("name"), self.g.len());
                }
                map.insert(sq.id, Mapping::Real(self.g.len()));
                self.g.push(n);
            } else if let Some(sub) = m.subgraph() {
                if stack.contains(&sq.module) {
                    self.diags.push(Diag {
                        square: Some(at),
                        message: format!("`{}` has itself inside: {} -> {}", sq.module, stack.join(" -> "), sq.module),
                    });
                    continue;
                }
                self.scopes += 1;
                stack.push(sq.module.clone());
                let inner = self.expand(&sub.canvas, self.scopes, Some(at), stack);
                stack.pop();
                for (name, g) in &inner.inputs {
                    if let Some(v) = sq.fields.get(name) {
                        self.g[*g].fields.insert("value".into(), v.clone());
                    }
                }
                if top.is_none() {
                    for (name, g) in &inner.outputs {
                        self.top_subs.push((sq.id, name.clone(), *g));
                    }
                }
                map.insert(sq.id, Mapping::Sub { inputs: inner.inputs, outputs: inner.outputs });
            } else {
                map.insert(sq.id, Mapping::Real(self.g.len()));
                self.g.push(gnode(GKind::Real(m.clone())));
            }
        }
        for (li, l) in canvas.links.iter().enumerate() {
            let src = match map.get(&l.from) {
                Some(Mapping::Real(g)) => Some((*g, l.output.clone())),
                Some(Mapping::Sub { outputs, .. }) => outputs.get(&l.output).map(|g| (*g, "value".to_string())),
                None => None,
            };
            let dst = match map.get(&l.to) {
                Some(Mapping::Real(g)) => Some((*g, l.input.clone())),
                Some(Mapping::Sub { inputs, .. }) => inputs.get(&l.input).map(|g| (*g, "value".to_string())),
                None => None,
            };
            // wires to pins that aren't there (any more) are left out
            if let (Some((sg, so)), Some((dg, di))) = (src, dst) {
                self.g[dg].incoming.push((di, sg, so, top.is_none().then_some(li)));
            }
        }
        ports
    }
}

struct VarCx<'a> {
    vars: &'a HashMap<(usize, String), VarDecl>,
    scope: usize,
    reg: &'a Registry,
    dir: Option<&'a std::path::Path>,
    deps: &'a std::cell::RefCell<Vec<(std::path::PathBuf, Option<std::time::SystemTime>)>>,
}

pub fn var_scope(name: &str, scope: usize) -> usize {
    if name.starts_with("::") { GLOBAL_SCOPE } else { scope }
}

impl SigCx for VarCx<'_> {
    fn var(&self, name: &str) -> Option<Type> {
        self.vars.get(&(var_scope(name, self.scope), name.to_string())).map(|d| d.ty.clone())
    }
    fn vars(&self) -> Vec<(String, Type)> {
        let mut v: Vec<(String, Type)> = self
            .vars
            .iter()
            .filter(|((scope, _), _)| *scope == self.scope || *scope == GLOBAL_SCOPE)
            .map(|((_, n), d)| (n.clone(), d.ty.clone()))
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    }
    fn record(&self, name: &str) -> Option<Arc<RecordType>> {
        self.reg.records.get(name).cloned()
    }
    fn read(&self, path: &str) -> Result<String, String> {
        let p = std::path::Path::new(path);
        let full = match self.dir {
            Some(d) if p.is_relative() => d.join(p),
            _ => p.to_path_buf(),
        };
        let stamp = std::fs::metadata(&full).and_then(|m| m.modified()).ok();
        self.deps.borrow_mut().push((full.clone(), stamp));
        std::fs::read_to_string(&full).map_err(|e| format!("{}: {e}", full.display()))
    }
}

/// The flattened canvas, with every square's signature and sources.
pub struct Flat {
    pub nodes: Vec<FNode>,
    /// The outputs of the subgraph squares on the canvas, and where each
    /// gets its value from.
    pub sub_outputs: Vec<(SquareId, String, Source)>,
    pub vars: HashMap<(usize, String), VarDecl>,
    pub diags: Vec<Diag>,
    /// The wires of the canvas that go into a data input with more than one.
    pub double: Vec<Link>,
    /// The files the signatures came from (see [`Lowered::deps`]).
    pub deps: Vec<(std::path::PathBuf, Option<std::time::SystemTime>)>,
}

pub fn flatten(canvas: &Canvas, reg: &Registry) -> Flat {
    let mut f = Flattener { reg, g: Vec::new(), top_subs: Vec::new(), scopes: 0, diags: Vec::new() };
    f.expand(canvas, 0, None, &mut Vec::new());
    let Flattener { g, mut diags, top_subs, .. } = f;
    let deps = std::cell::RefCell::new(Vec::new());
    let dir = canvas.dir.as_deref();

    // variables
    let mut vars: HashMap<(usize, String), VarDecl> = HashMap::new();
    for n in &g {
        let GKind::Real(m) = &n.kind else { continue };
        if m.path() != "var/var" {
            continue;
        }
        let p = Params { defs: m.params(), values: &n.params };
        let name = p.str("name");
        let cx = VarCx { vars: &vars, scope: n.scope, reg, dir, deps: &deps };
        let ty = match (&cx as &dyn SigCx).parse_type(&p.str("type")) {
            Ok(t) => t,
            Err(e) => {
                diags.push(Diag { square: Some(n.square), message: e });
                continue;
            }
        };
        let key = (var_scope(&name, n.scope), name.clone());
        match vars.get(&key) {
            Some(d) if d.ty != ty => diags.push(Diag {
                square: Some(n.square),
                message: format!("`{name}` is declared {} elsewhere, and {ty} here", d.ty),
            }),
            Some(_) => {}
            None => {
                vars.insert(key, VarDecl { ty, init: p.str("initial") });
            }
        }
    }

    // signatures
    let mut flat_of: Vec<Option<NodeId>> = vec![None; g.len()];
    let mut nodes: Vec<FNode> = Vec::new();
    for (i, n) in g.iter().enumerate() {
        let GKind::Real(m) = &n.kind else { continue };
        let cx = VarCx { vars: &vars, scope: n.scope, reg, dir, deps: &deps };
        match m.signature(&Params { defs: m.params(), values: &n.params }, &cx) {
            Ok(sig) => {
                flat_of[i] = Some(nodes.len() as NodeId);
                nodes.push(FNode {
                    module: m.clone(),
                    square: n.square,
                    top: n.top,
                    params: n.params.clone(),
                    scope: n.scope,
                    in_types: vec![None; sig.inputs.len()],
                    out_types: vec![None; sig.outputs.len()],
                    inputs: Vec::new(),
                    targets: Vec::new(),
                    sig,
                });
            }
            Err(e) => diags.push(Diag { square: Some(n.square), message: e }),
        }
    }

    // sources, back through the ports
    fn source(g: &[GNode], flat_of: &[Option<NodeId>], nodes: &[FNode], at: usize, out: &str, depth: u32) -> Source {
        if depth > 64 {
            return Source::None;
        }
        match &g[at].kind {
            GKind::Real(_) => match flat_of[at] {
                Some(f) => match nodes[f as usize].sig.output_index(out) {
                    Some(o) => Source::Wire(f, o),
                    None => Source::None,
                },
                None => Source::None,
            },
            GKind::Pass => match g[at].incoming.iter().rev().find(|(i, _, _, _)| i == "value") {
                Some((_, sg, so, _)) => source(g, flat_of, nodes, *sg, so, depth + 1),
                None => g[at]
                    .fields
                    .get("value")
                    .map(|v| Source::Field(v.clone(), g[at].port_ty.clone()))
                    .unwrap_or(Source::None),
            },
        }
    }
    let mut outgoing: HashMap<(usize, String), Vec<(usize, String)>> = HashMap::new();
    for (d, n) in g.iter().enumerate() {
        for (input, s, out, _) in &n.incoming {
            outgoing.entry((*s, out.clone())).or_default().push((d, input.clone()));
        }
    }
    fn targets(
        g: &[GNode],
        flat_of: &[Option<NodeId>],
        nodes: &[FNode],
        outgoing: &HashMap<(usize, String), Vec<(usize, String)>>,
        at: usize,
        out: &str,
        depth: u32,
        into: &mut Vec<(NodeId, usize)>,
    ) {
        if depth > 64 {
            return;
        }
        for (d, input) in outgoing.get(&(at, out.to_string())).into_iter().flatten() {
            match &g[*d].kind {
                GKind::Real(_) => {
                    if let Some(f) = flat_of[*d]
                        && let Some(i) = nodes[f as usize].sig.input_index(input)
                    {
                        into.push((f, i));
                    }
                }
                GKind::Pass => targets(g, flat_of, nodes, outgoing, *d, "value", depth + 1, into),
            }
        }
    }
    for (i, n) in g.iter().enumerate() {
        let Some(f) = flat_of[i] else { continue };
        let sig = nodes[f as usize].sig.clone();
        let mut inputs = Vec::new();
        for pin in &sig.inputs {
            let wire = n.incoming.iter().rev().find(|(inp, _, _, _)| *inp == pin.name);
            inputs.push(match (&pin.ty, wire) {
                (PinTy::Tick, _) => Source::None,
                (_, Some((_, sg, so, _))) => source(&g, &flat_of, &nodes, *sg, so, 0),
                (_, None) => match (&pin.field, n.fields.get(&pin.name)) {
                    (Some(_), Some(v)) => Source::Field(v.clone(), None),
                    (Some(d), None) => Source::Field(d.clone(), None),
                    (None, _) => Source::None,
                },
            });
        }
        let mut tgts = Vec::new();
        for pin in &sig.outputs {
            let mut t = Vec::new();
            if pin.ty == PinTy::Tick {
                targets(&g, &flat_of, &nodes, &outgoing, i, &pin.name, 0, &mut t);
            }
            tgts.push(t);
        }
        nodes[f as usize].inputs = inputs;
        nodes[f as usize].targets = tgts;
    }

    // tick wires into data inputs and the other way round
    let mut bad: Vec<(SquareId, String)> = Vec::new();
    // and more than one wire into a data input (a tick input takes any number):
    // a real square's, or a port's (a subgraph's input, or a graph/output)
    let mut double = Vec::new();
    for (i, n) in g.iter().enumerate() {
        let pins: Vec<(String, String)> = match &n.kind {
            GKind::Real(_) => match flat_of[i] {
                Some(f) => nodes[f as usize]
                    .sig
                    .inputs
                    .iter()
                    .filter(|p| p.ty != PinTy::Tick)
                    .map(|p| (p.name.clone(), p.name.clone()))
                    .collect(),
                None => continue,
            },
            GKind::Pass if n.port_ty.is_some() => vec![("value".into(), n.port.clone())],
            GKind::Pass => continue,
        };
        for (pin, name) in pins {
            let wires: Vec<_> = n.incoming.iter().filter(|w| w.0 == pin).collect();
            if wires.len() > 1 {
                bad.push((n.square, format!("`{name}` takes one wire, and has {}", wires.len())));
                double.extend(wires.iter().filter_map(|w| w.3).map(|li| canvas.links[li].clone()));
            }
        }
    }
    for (f, n) in nodes.iter().enumerate() {
        for (i, s) in n.inputs.iter().enumerate() {
            if let Source::Wire(sn, so) = s
                && nodes[*sn as usize].sig.outputs[*so].ty == PinTy::Tick
            {
                bad.push((nodes[f].square, format!("a tick goes into `{}`, which takes data", n.sig.inputs[i].name)));
            }
        }
        for (o, ts) in n.targets.iter().enumerate() {
            for (tn, ti) in ts {
                if nodes[*tn as usize].sig.inputs[*ti].ty != PinTy::Tick {
                    bad.push((
                        nodes[*tn as usize].square,
                        format!("`{}` takes data, and a tick goes into it", nodes[*tn as usize].sig.inputs[*ti].name),
                    ));
                }
                let _ = o;
            }
        }
    }
    // a subgraph used twice reports once
    bad.sort();
    bad.dedup();
    for (square, message) in bad {
        diags.push(Diag { square: Some(square), message });
    }
    let sub_outputs = top_subs
        .into_iter()
        .map(|(sq, name, port)| (sq, name, source(&g, &flat_of, &nodes, port, "value", 0)))
        .collect();
    Flat { nodes, sub_outputs, vars, diags, double, deps: deps.into_inner() }
}

// ---------------------------------------------------------------- lowering

pub struct Lowerer<'a> {
    flat: &'a Flat,
    reg: &'a Registry,
    prog: Program,
    out_slots: HashMap<(NodeId, usize), SlotId>,
    var_slots: HashMap<(usize, String), SlotId>,
    handlers: HashMap<(NodeId, usize), HandlerId>,
    work: Vec<(HandlerId, NodeId, usize)>,
    memo: HashMap<(NodeId, usize), Expr>,
    evaluating: Vec<NodeId>,
    locals: Vec<Type>,
    exts: HashMap<String, usize>,
    ext_nodes: HashMap<NodeId, usize>,
    diags: Vec<Diag>,
    /// Lowering for a test: the `test/case` square it starts from (and
    /// nothing else starts the program).
    test_case: Option<SquareId>,
}

/// What a module lowers its square with.
pub struct Lx<'l, 'a> {
    lw: &'l mut Lowerer<'a>,
    node: NodeId,
}

/// The default of TY, as an expression.
pub fn default_expr(ty: &Type) -> Expr {
    if ty.is_scalar() { Expr::Const(Value::default_of(ty)) } else { Expr::Default(ty.clone()) }
}

fn ident(s: &str) -> String {
    let mut out: String =
        s.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' }).collect();
    out.truncate(40);
    out
}

impl<'a> Lx<'_, 'a> {
    fn n(&self) -> &'a FNode {
        &self.lw.flat.nodes[self.node as usize]
    }

    pub fn node(&self) -> NodeId {
        self.node
    }

    pub fn params(&self) -> Params<'a> {
        let n = self.n();
        Params { defs: n.module.params(), values: &n.params }
    }

    pub fn sig(&self) -> &'a Signature {
        &self.n().sig
    }

    pub fn in_ty(&self, i: usize) -> Type {
        self.n().in_types[i].clone().unwrap_or(Type::Int)
    }

    pub fn out_ty(&self, o: usize) -> Type {
        self.n().out_types[o].clone().unwrap_or(Type::Int)
    }

    /// The square of the canvas it is, or is inside of.
    pub fn square(&self) -> SquareId {
        self.n().square
    }

    /// The `test/case` square the program is lowered for, if it's for one.
    pub fn test_case(&self) -> Option<SquareId> {
        self.lw.test_case
    }

    /// What's typed in data input I's field, if it has nothing wired.
    pub fn field_text(&self, i: usize) -> Option<String> {
        match &self.n().inputs[i] {
            Source::Field(text, _) => Some(text.clone()),
            _ => None,
        }
    }

    /// The type of slot S.
    pub fn slot_ty(&self, s: SlotId) -> Type {
        self.lw.prog.slots[s as usize].ty.clone()
    }

    pub fn record(&self, name: &str) -> Option<Arc<RecordType>> {
        self.lw.reg.records.get(name).cloned()
    }

    /// The value of data input I: what's wired to it, or its field.
    pub fn input(&mut self, i: usize) -> Result<Expr, String> {
        let ty = self.in_ty(i);
        match self.n().inputs[i].clone() {
            Source::Wire(src, o) => {
                let e = self.lw.pull(src, o)?;
                let from = self.lw.flat.nodes[src as usize].out_types[o].clone().unwrap_or(Type::Int);
                Ok(if from == Type::Int && ty == Type::Float { Expr::un(Un::Widen, e) } else { e })
            }
            Source::Field(text, port) => {
                // through a port, it's of the port's type, then widened if need be
                let from = port.unwrap_or_else(|| ty.clone());
                Ok(match Value::parse(&text, &from) {
                    Some(Value::Int(i)) if ty == Type::Float => Expr::Const(Value::Float(i as f64)),
                    Some(v) => Expr::Const(v),
                    None => default_expr(&ty),
                })
            }
            Source::None => Ok(default_expr(&ty)),
        }
    }

    /// Whether data input I has a wire.
    pub fn wired(&self, i: usize) -> bool {
        matches!(self.n().inputs[i], Source::Wire(..))
    }

    /// The slot where output O keeps its value.
    pub fn out_slot(&mut self, o: usize) -> SlotId {
        if let Some(s) = self.lw.out_slots.get(&(self.node, o)) {
            return *s;
        }
        let n = self.n();
        let ty = self.out_ty(o);
        let name = format!(
            "{}_{}_{}",
            ident(n.module.path().rsplit('/').next().unwrap_or("")),
            self.node,
            ident(&n.sig.outputs[o].name)
        );
        let shows = n.top.then(|| (n.square, n.sig.outputs[o].name.clone()));
        let s = self.lw.new_slot(ty.clone(), Value::default_of(&ty), name, shows);
        self.lw.out_slots.insert((self.node, o), s);
        s
    }

    /// A slot of its own, for state that's in no output.
    pub fn private_slot(&mut self, what: &str, ty: Type, init: Value) -> SlotId {
        let name =
            format!("{}_{}_{}", ident(self.n().module.path().rsplit('/').next().unwrap_or("")), self.node, ident(what));
        self.lw.new_slot(ty, init, name, None)
    }

    /// The slot of variable NAME, as seen from here.
    pub fn var(&mut self, name: &str) -> Result<SlotId, String> {
        let key = (var_scope(name, self.n().scope), name.to_string());
        if let Some(s) = self.lw.var_slots.get(&key) {
            return Ok(*s);
        }
        let d = self.lw.flat.vars.get(&key).ok_or_else(|| format!("there's no variable `{name}` here"))?;
        let init = if !d.ty.is_scalar() || (d.init.trim().is_empty() && d.ty != Type::Str) {
            Value::default_of(&d.ty)
        } else {
            Value::parse(&d.init, &d.ty)
                .ok_or_else(|| format!("`{}` isn't a {} (the start of `{name}`)", d.init, d.ty))?
        };
        let s = self.lw.new_slot(d.ty.clone(), init, format!("var_{}", ident(name.trim_start_matches(':'))), None);
        self.lw.var_slots.insert(key, s);
        Ok(s)
    }

    /// A local of the handler being lowered.
    pub fn local(&mut self, ty: Type) -> LocalId {
        self.lw.locals.push(ty);
        (self.lw.locals.len() - 1) as LocalId
    }

    /// Fires tick output O.
    pub fn fire(&mut self, o: usize) -> Stmt {
        Stmt::Fire(self.fire_of(o))
    }

    pub fn fire_of(&mut self, o: usize) -> Fire {
        let targets = self.n().targets[o].clone();
        Fire { targets: targets.into_iter().map(|(n, i)| self.lw.handler(n, i)).collect(), meets: Vec::new() }
    }

    /// The Lua or Rust module at PATH, and the instance of it for this
    /// square, with PARAMS.
    pub fn ext_instance(&mut self, path: &str, kind: &ExtKind, params: Vec<(String, Value)>) -> usize {
        if let Some(i) = self.lw.ext_nodes.get(&self.node) {
            return *i;
        }
        let module = *self.lw.exts.entry(path.to_string()).or_insert_with(|| {
            self.lw.prog.exts.push(ExtDef { path: path.to_string(), kind: kind.clone() });
            self.lw.prog.exts.len() - 1
        });
        let square = self.n().square;
        self.lw.prog.ext_instances.push(ExtInstance { module, params, square });
        let i = self.lw.prog.ext_instances.len() - 1;
        self.lw.ext_nodes.insert(self.node, i);
        i
    }

    /// Records the program uses, for the generated code.
    pub fn use_record(&mut self, r: &Arc<RecordType>) {
        if !self.lw.prog.records.iter().any(|x| x.name == r.name) {
            for (_, t) in &r.fields {
                self.use_type(&t.clone());
            }
            self.lw.prog.records.push(r.clone());
        }
    }

    fn use_type(&mut self, t: &Type) {
        match t {
            Type::Record(r) => self.use_record(r),
            Type::List(t) | Type::Map(t) => self.use_type(t),
            _ => {}
        }
    }
}

impl<'a> Lowerer<'a> {
    fn new_slot(&mut self, ty: Type, init: Value, name: String, shows: Option<(SquareId, String)>) -> SlotId {
        self.prog.slots.push(SlotDef { ty, init, name, shows });
        (self.prog.slots.len() - 1) as SlotId
    }

    fn cx(&mut self, node: NodeId) -> Lx<'_, 'a> {
        Lx { lw: self, node }
    }

    fn pull(&mut self, node: NodeId, o: usize) -> Result<Expr, String> {
        if let Some(e) = self.memo.get(&(node, o)) {
            return Ok(e.clone());
        }
        if self.evaluating.contains(&node) {
            return Err("its inputs come from its own outputs, with no tick in between".into());
        }
        self.evaluating.push(node);
        let m = self.flat.nodes[node as usize].module.clone();
        let r = m.eval(&mut self.cx(node), o);
        self.evaluating.pop();
        let e = r.map_err(|e| {
            let n = &self.flat.nodes[node as usize];
            format!("{} ({}): {e}", n.module.path(), n.sig.outputs[o].name)
        })?;
        self.memo.insert((node, o), e.clone());
        Ok(e)
    }

    fn handler(&mut self, node: NodeId, input: usize) -> HandlerId {
        if let Some(h) = self.handlers.get(&(node, input)) {
            return *h;
        }
        let n = &self.flat.nodes[node as usize];
        let h = self.prog.handlers.len() as HandlerId;
        self.prog.handlers.push(Handler {
            node,
            name: format!("{}_{}_{}", ident(n.module.path()), node, ident(&n.sig.inputs[input].name)),
            locals: Vec::new(),
            body: Vec::new(),
        });
        self.handlers.insert((node, input), h);
        self.work.push((h, node, input));
        h
    }

    /// Lowers BODY into handler H, with its own locals.
    fn fill(&mut self, h: HandlerId, node: NodeId, body: impl FnOnce(&mut Lx) -> Result<Block, String>) {
        let saved = std::mem::take(&mut self.locals);
        let r = body(&mut self.cx(node));
        let locals = std::mem::replace(&mut self.locals, saved);
        match r {
            Ok(b) => {
                let handler = &mut self.prog.handlers[h as usize];
                handler.body = std::iter::once(Stmt::Enter(node)).chain(b).collect();
                handler.locals = locals;
            }
            Err(e) => self.diags.push(Diag { square: Some(self.flat.nodes[node as usize].square), message: e }),
        }
    }

    fn with_locals<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> (R, Vec<Type>) {
        let saved = std::mem::take(&mut self.locals);
        let r = f(self);
        (r, std::mem::replace(&mut self.locals, saved))
    }

    fn add_handler(&mut self, node: NodeId, name: String, body: Block, locals: Vec<Type>) -> HandlerId {
        let body = std::iter::once(Stmt::Enter(node)).chain(body).collect();
        self.prog.handlers.push(Handler { node, name, locals, body });
        (self.prog.handlers.len() - 1) as HandlerId
    }

    fn drain(&mut self) {
        while let Some((h, node, input)) = self.work.pop() {
            let m = self.flat.nodes[node as usize].module.clone();
            self.fill(h, node, |cx| m.tick(cx, input));
        }
    }
}

pub fn lower(canvas: &Canvas, reg: &Registry) -> Lowered {
    lower_for(canvas, reg, None)
}

/// CANVAS lowered to run test CASE (a `test/case` square of it): it starts
/// there, with the values it gives, and nowhere else.
pub fn lower_test(canvas: &Canvas, reg: &Registry, case: SquareId) -> Lowered {
    lower_for(canvas, reg, Some(case))
}

fn lower_for(canvas: &Canvas, reg: &Registry, test_case: Option<SquareId>) -> Lowered {
    let mut flat = flatten(canvas, reg);
    let mut diags = std::mem::take(&mut flat.diags);
    crate::types::infer(&mut flat.nodes, &mut diags);

    let mut squares = HashMap::new();
    for n in flat.nodes.iter().filter(|n| n.top) {
        squares.insert(
            n.square,
            SquareInfo { sig: n.sig.clone(), in_types: n.in_types.clone(), out_types: n.out_types.clone() },
        );
    }
    // subgraph squares: their ports' types are in their signature
    for sq in &canvas.squares {
        if let Some(m) = reg.get(&sq.module).filter(|m| m.subgraph().is_some()) {
            let deps = std::cell::RefCell::new(Vec::new());
            let cx = VarCx { vars: &flat.vars, scope: 0, reg, dir: canvas.dir.as_deref(), deps: &deps };
            if let Ok(sig) = m.signature(&Params { defs: m.params(), values: &sq.params }, &cx) {
                let concrete = |p: &crate::registry::PinDef| match &p.ty {
                    PinTy::Data(crate::registry::TExpr::T(t)) => Some(t.clone()),
                    _ => None,
                };
                let in_types = sig.inputs.iter().map(concrete).collect();
                let out_types = sig.outputs.iter().map(concrete).collect();
                squares.insert(sq.id, SquareInfo { sig, in_types, out_types });
            }
        }
    }
    if !diags.is_empty() {
        return Lowered { program: None, diags, double: flat.double, squares, previews: Vec::new(), deps: flat.deps };
    }

    let mut lw = Lowerer {
        flat: &flat,
        reg,
        prog: Program::default(),
        out_slots: HashMap::new(),
        var_slots: HashMap::new(),
        handlers: HashMap::new(),
        work: Vec::new(),
        memo: HashMap::new(),
        evaluating: Vec::new(),
        locals: Vec::new(),
        exts: HashMap::new(),
        ext_nodes: HashMap::new(),
        diags: Vec::new(),
        test_case,
    };
    lw.prog.nodes =
        flat.nodes.iter().map(|n| NodeInfo { square: n.square, module: n.module.path().to_string() }).collect();
    for (i, n) in flat.nodes.iter().enumerate() {
        let node = i as NodeId;
        for t in n.in_types.iter().chain(&n.out_types).flatten() {
            lw.cx(node).use_type(t);
        }
        let m = n.module.clone();
        let (start, locals) = lw.with_locals(|lw| m.start(&mut lw.cx(node)));
        // a test starts at its case only
        let start = match test_case {
            Some(case) if !(n.top && n.square == case) => Ok(None),
            _ => start,
        };
        match start {
            Ok(Some(b)) => {
                let h = lw.add_handler(node, format!("start_{node}"), b, locals);
                lw.prog.starts.targets.push(h);
            }
            Ok(None) => {}
            Err(e) => lw.diags.push(Diag { square: Some(n.square), message: e }),
        }
        let (clock, locals) = lw.with_locals(|lw| m.clock(&mut lw.cx(node)));
        match clock {
            Ok(Some((interval, b))) => {
                let h = lw.add_handler(node, format!("clock_{node}"), b, locals);
                lw.prog.clocks.push(Clock { interval, handler: h });
            }
            Ok(None) => {}
            Err(e) => lw.diags.push(Diag { square: Some(n.square), message: e }),
        }
        lw.drain();
    }
    // the variables to show: the canvas's and the global ones
    let mut vars: Vec<(String, SlotId)> = lw
        .var_slots
        .iter()
        .filter(|((scope, _), _)| *scope == 0 || *scope == GLOBAL_SCOPE)
        .map(|((_, n), s)| (n.clone(), *s))
        .collect();
    vars.sort();
    lw.prog.vars = vars;
    // pure squares that make images, for previews
    let mut previews = Vec::new();
    for (i, n) in flat.nodes.iter().enumerate() {
        if !n.top || n.sig.has_tick_input() || n.module.path().starts_with("graph/") {
            continue;
        }
        for (o, t) in n.out_types.iter().enumerate() {
            if t.as_ref() == Some(&Type::Image)
                && let Ok(e) = lw.pull(i as NodeId, o)
            {
                previews.push((n.square, n.sig.outputs[o].name.clone(), e));
            }
        }
    }
    // and the images that subgraph squares make, from what's inside them
    for (sq, name, src) in &flat.sub_outputs {
        if let Source::Wire(node, o) = src {
            let n = &flat.nodes[*node as usize];
            if n.out_types[*o].as_ref() == Some(&Type::Image)
                && !n.sig.has_tick_input()
                && let Ok(e) = lw.pull(*node, *o)
            {
                previews.push((*sq, name.clone(), e));
            }
        }
    }
    let mut prog = std::mem::take(&mut lw.prog);
    let mut diags = std::mem::take(&mut lw.diags);
    diags.dedup();
    if !diags.is_empty() {
        return Lowered {
            program: None,
            diags,
            double: flat.double.clone(),
            squares,
            previews: Vec::new(),
            deps: flat.deps.clone(),
        };
    }
    prog.find_meets();
    Lowered { program: Some(prog), diags, double: flat.double.clone(), squares, previews, deps: flat.deps.clone() }
}

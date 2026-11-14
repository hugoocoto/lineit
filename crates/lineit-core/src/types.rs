//! Type inference over the wires.
//!
//! Each square's type variables (see [`crate::registry::TExpr`]) get the
//! join of the types that flow into them, from wires and from the literals
//! typed in fields (`2` is an int, `2.5` a float). An int and a float join
//! as a float: the int widens on its wire. A variable nothing flows into
//! takes the type of what it flows to, and failing that a default (int for
//! numbers, str for anything). Then every wire and every field is checked.

use lineit_rt::{Type, Value};

use crate::lower::{Diag, FNode, Source};
use crate::registry::{Class, PinTy, TExpr};

fn join(a: &Type, b: &Type) -> Option<Type> {
    match (a, b) {
        _ if a == b => Some(a.clone()),
        (Type::Int, Type::Float) | (Type::Float, Type::Int) => Some(Type::Float),
        _ => None,
    }
}

/// Whether a wire can carry FROM into an input of type TO.
pub fn fits(from: &Type, to: &Type) -> bool {
    from == to || (*from == Type::Int && *to == Type::Float)
}

struct State {
    base: Vec<usize>,
    class: Vec<Class>,
    ty: Vec<Option<Type>>,
}

impl State {
    fn resolve(&self, node: usize, t: &TExpr) -> Option<Type> {
        let b = self.base[node];
        match t {
            TExpr::T(t) => Some(t.clone()),
            TExpr::Var(v) => self.ty[b + v].clone(),
            TExpr::ListOf(v) => self.ty[b + v].clone().map(|t| Type::List(Box::new(t))),
            TExpr::MapOf(v) => self.ty[b + v].clone().map(|t| Type::Map(Box::new(t))),
        }
    }

    /// TY flows into T, on NODE. Whether that changed anything.
    fn flow(&mut self, node: usize, t: &TExpr, ty: &Type) -> Result<bool, String> {
        let b = self.base[node];
        let (v, inner) = match (t, ty) {
            (TExpr::T(_), _) => return Ok(false),
            (TExpr::Var(v), ty) => (b + v, ty.clone()),
            (TExpr::ListOf(v), Type::List(e)) => (b + v, (**e).clone()),
            (TExpr::MapOf(v), Type::Map(e)) => (b + v, (**e).clone()),
            (TExpr::ListOf(_), _) => return Err(format!("it takes a list, not a {ty}")),
            (TExpr::MapOf(_), _) => return Err(format!("it takes a map, not a {ty}")),
        };
        match &self.ty[v] {
            None => {
                self.ty[v] = Some(inner);
                Ok(true)
            }
            Some(old) => match join(old, &inner) {
                Some(j) if j == *old => Ok(false),
                Some(j) => {
                    self.ty[v] = Some(j);
                    Ok(true)
                }
                None => Err(format!("its inputs should have one type, and they get {old} and {inner}")),
            },
        }
    }
}

fn data(p: &PinTy) -> Option<&TExpr> {
    match p {
        PinTy::Data(t) => Some(t),
        PinTy::Tick => None,
    }
}

pub fn infer(nodes: &mut [FNode], diags: &mut Vec<Diag>) {
    let mut base = Vec::with_capacity(nodes.len());
    let mut class = Vec::new();
    for n in nodes.iter() {
        base.push(class.len());
        class.extend_from_slice(&n.sig.vars);
    }
    let mut st = State { base, ty: vec![None; class.len()], class };
    let mut errors: Vec<(usize, String)> = Vec::new();

    // what flows from where, as (node, input) <- source
    let flows = |st: &mut State, errors: &mut Vec<(usize, String)>, nodes: &[FNode]| {
        for _ in 0..64 {
            let mut changed = false;
            for (d, n) in nodes.iter().enumerate() {
                for (i, src) in n.inputs.iter().enumerate() {
                    let Some(dt) = data(&n.sig.inputs[i].ty) else { continue };
                    let ty = match src {
                        Source::Wire(s, o) => {
                            data(&nodes[*s as usize].sig.outputs[*o].ty).and_then(|t| st.resolve(*s as usize, t))
                        }
                        Source::Field(_, Some(port)) => Some(port.clone()),
                        Source::Field(text, None) if matches!(dt, TExpr::Var(_)) => Some(Value::literal_type(text)),
                        _ => None,
                    };
                    if let Some(ty) = ty {
                        match st.flow(d, dt, &ty) {
                            Ok(c) => changed |= c,
                            Err(e) => {
                                if !errors.iter().any(|(x, _)| *x == d) {
                                    errors.push((d, e));
                                }
                            }
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
    };
    flows(&mut st, &mut errors, nodes);

    // nothing flows in: the type of what it flows to
    for _ in 0..8 {
        let mut changed = false;
        for (d, n) in nodes.iter().enumerate() {
            for (i, src) in n.inputs.iter().enumerate() {
                let Source::Wire(s, o) = src else { continue };
                let (Some(dt), Some(st_)) = (data(&n.sig.inputs[i].ty), data(&nodes[*s as usize].sig.outputs[*o].ty))
                else {
                    continue;
                };
                if st.resolve(*s as usize, st_).is_none()
                    && let Some(to) = st.resolve(d, dt)
                {
                    changed |= st.flow(*s as usize, st_, &to).unwrap_or(false);
                }
            }
        }
        flows(&mut st, &mut errors, nodes);
        if !changed {
            break;
        }
    }

    // and failing that, a default
    for v in 0..st.ty.len() {
        if st.ty[v].is_none() {
            st.ty[v] = Some(match st.class[v] {
                Class::Num | Class::Ord => Type::Int,
                Class::Any => Type::Str,
            });
        }
    }
    flows(&mut st, &mut errors, nodes);

    for (d, n) in nodes.iter().enumerate() {
        for (v, c) in n.sig.vars.iter().enumerate() {
            let t = st.ty[st.base[d] + v].clone().unwrap_or(Type::Int);
            let ok = match c {
                Class::Any => true,
                Class::Num => t.is_number(),
                Class::Ord => t.is_ordered(),
            };
            if !ok {
                errors.push((
                    d,
                    format!(
                        "it takes {}, not {t}",
                        if *c == Class::Num { "numbers" } else { "numbers, strings or bools" }
                    ),
                ));
            }
        }
    }

    for d in 0..nodes.len() {
        let n = &nodes[d];
        let ins: Vec<Option<Type>> = n.sig.inputs.iter().map(|p| data(&p.ty).and_then(|t| st.resolve(d, t))).collect();
        let outs: Vec<Option<Type>> =
            n.sig.outputs.iter().map(|p| data(&p.ty).and_then(|t| st.resolve(d, t))).collect();
        nodes[d].in_types = ins;
        nodes[d].out_types = outs;
    }

    for (d, n) in nodes.iter().enumerate() {
        for (i, src) in n.inputs.iter().enumerate() {
            let Some(to) = &n.in_types[i] else { continue };
            match src {
                Source::Wire(s, o) => {
                    if let Some(from) = &nodes[*s as usize].out_types[*o]
                        && !fits(from, to)
                    {
                        errors.push((d, format!("`{}` takes {to}, and gets {from}", n.sig.inputs[i].name)));
                    }
                }
                Source::Field(_, Some(port)) => {
                    if !fits(port, to) {
                        errors.push((d, format!("`{}` takes {to}, and gets {port}", n.sig.inputs[i].name)));
                    }
                }
                Source::Field(text, None) => {
                    if to.is_scalar()
                        && Value::parse(text, to).is_none()
                        && !(text.trim().is_empty() && *to != Type::Str)
                    {
                        errors.push((d, format!("`{text}` isn't a {to} (in `{}`)", n.sig.inputs[i].name)));
                    }
                }
                Source::None => {}
            }
        }
    }

    // what a function gives has to be what its output says
    for (d, n) in nodes.iter().enumerate() {
        for (src, to, name) in &n.fn_outs {
            if let Source::Wire(s, o) = src
                && let Some(from) = &nodes[*s as usize].out_types[*o]
                && !fits(from, to)
            {
                errors.push((d, format!("the function's output `{name}` is {to}, and it gets {from}")));
            }
        }
    }

    errors.sort_by_key(|(d, _)| *d);
    errors.dedup();
    for (d, e) in errors {
        diags.push(Diag { square: Some(nodes[d].square), message: format!("{}: {e}", nodes[d].module.path()) });
    }
}

//! The modules that come with lineit. None of them runs code of its own:
//! each says how its square lowers to IR, so Run interprets plain IR and
//! Compile makes plain Rust of it.

use std::sync::Arc;

use lineit_rt::{Type, Value};

use super::{Class, FnPorts, Kind, Module, ParamDef, ParamKind, Params, SigCx, Signature, TExpr};
use crate::ir::*;
use crate::lower::Lx;

type SigFn = Box<dyn Fn(&Params, &dyn SigCx) -> Result<Signature, String> + Send + Sync>;
type EvalFn = Box<dyn Fn(&mut Lx, usize) -> Result<Expr, String> + Send + Sync>;
type TickFn = Box<dyn Fn(&mut Lx, usize) -> Result<Block, String> + Send + Sync>;
type StartFn = Box<dyn Fn(&mut Lx) -> Result<Block, String> + Send + Sync>;
type ClockFn = Box<dyn Fn(&mut Lx) -> Result<(Expr, Block), String> + Send + Sync>;

pub struct Builtin {
    path: String,
    doc: String,
    params: Vec<ParamDef>,
    sig: SigFn,
    eval: Option<EvalFn>,
    tick: Option<TickFn>,
    start: Option<StartFn>,
    clock: Option<ClockFn>,
}

impl Module for Builtin {
    fn path(&self) -> &str {
        &self.path
    }
    fn doc(&self) -> &str {
        &self.doc
    }
    fn kind(&self) -> Kind {
        Kind::Builtin
    }
    fn params(&self) -> &[ParamDef] {
        &self.params
    }
    fn signature(&self, params: &Params, cx: &dyn SigCx) -> Result<Signature, String> {
        (self.sig)(params, cx)
    }
    fn eval(&self, cx: &mut Lx, output: usize) -> Result<Expr, String> {
        match &self.eval {
            Some(f) => f(cx, output),
            None => Ok(Expr::Slot(cx.out_slot(output))),
        }
    }
    fn tick(&self, cx: &mut Lx, input: usize) -> Result<Block, String> {
        match &self.tick {
            Some(f) => f(cx, input),
            None => Err("it has no tick inputs".into()),
        }
    }
    fn start(&self, cx: &mut Lx) -> Result<Option<Block>, String> {
        // nothing to do (a test/case in a run that isn't its): no start
        Ok(self.start.as_ref().map(|f| f(cx)).transpose()?.filter(|b| !b.is_empty()))
    }
    fn clock(&self, cx: &mut Lx) -> Result<Option<(Expr, Block)>, String> {
        self.clock.as_ref().map(|f| f(cx)).transpose()
    }
}

struct B(Builtin);

fn b(path: &str, doc: &str) -> B {
    B(Builtin {
        path: path.into(),
        doc: doc.into(),
        params: Vec::new(),
        sig: Box::new(|_, _| Ok(Signature::new())),
        eval: None,
        tick: None,
        start: None,
        clock: None,
    })
}

impl B {
    fn param(mut self, name: &str, kind: ParamKind, default: &str) -> B {
        self.0.params.push(ParamDef::new(name, kind, default));
        self
    }
    fn sig(mut self, f: impl Fn(&Params, &dyn SigCx) -> Result<Signature, String> + Send + Sync + 'static) -> B {
        self.0.sig = Box::new(f);
        self
    }
    fn fixed(self, s: Signature) -> B {
        self.sig(move |_, _| Ok(s.clone()))
    }
    fn eval(mut self, f: impl Fn(&mut Lx, usize) -> Result<Expr, String> + Send + Sync + 'static) -> B {
        self.0.eval = Some(Box::new(f));
        self
    }
    fn tick(mut self, f: impl Fn(&mut Lx, usize) -> Result<Block, String> + Send + Sync + 'static) -> B {
        self.0.tick = Some(Box::new(f));
        self
    }
    fn start(mut self, f: impl Fn(&mut Lx) -> Result<Block, String> + Send + Sync + 'static) -> B {
        self.0.start = Some(Box::new(f));
        self
    }
    fn clock(mut self, f: impl Fn(&mut Lx) -> Result<(Expr, Block), String> + Send + Sync + 'static) -> B {
        self.0.clock = Some(Box::new(f));
        self
    }
    fn done(self) -> Arc<dyn Module> {
        Arc::new(self.0)
    }
}

fn t(ty: Type) -> TExpr {
    TExpr::T(ty)
}
fn int() -> TExpr {
    t(Type::Int)
}
fn float() -> TExpr {
    t(Type::Float)
}
fn boolean() -> TExpr {
    t(Type::Bool)
}
fn string() -> TExpr {
    t(Type::Str)
}
fn image() -> TExpr {
    t(Type::Image)
}
fn v(n: usize) -> TExpr {
    TExpr::Var(n)
}

/// What a field of type TY starts with.
pub fn default_text(ty: &Type) -> &'static str {
    match ty {
        Type::Int | Type::Float => "0",
        Type::Bool => "false",
        _ => "",
    }
}

/// The input of a value of TY: with a field if it can be typed.
fn value_in(s: Signature, name: &str, ty: &Type) -> Signature {
    if ty.is_scalar() { s.input(name, t(ty.clone()), default_text(ty)) } else { s.wire_in(name, t(ty.clone())) }
}

/// E, of type FROM, as a TO.
pub fn convert(e: Expr, from: &Type, to: &Type) -> Result<Expr, String> {
    use Type::*;
    Ok(match (from, to) {
        _ if from == to => e,
        (Int, Float) => Expr::un(Un::Widen, e),
        (Float, Int) => Expr::un(Un::Trunc, e),
        (Bool, Int) => Expr::un(Un::BoolToInt, e),
        (Int, Bool) => Expr::un(Un::IntToBool, e),
        (Float, Bool) => Expr::un(Un::FloatToBool, e),
        (Bool, Float) => Expr::un(Un::BoolToFloat, e),
        (_, Str) => Expr::call(Func::ToStr(from.clone()), vec![e]),
        (Str, Int) => Expr::call(Func::StrToInt, vec![e]),
        (Str, Float) => Expr::call(Func::StrToFloat, vec![e]),
        (Str, Bool) => Expr::call(Func::StrToBool, vec![e]),
        _ => return Err(format!("a {from} can't be made a {to}")),
    })
}

fn text(e: Expr, ty: &Type) -> Expr {
    if *ty == Type::Str { e } else { Expr::call(Func::ToStr(ty.clone()), vec![e]) }
}

fn types_choice() -> ParamKind {
    ParamKind::Type { allow_tick: false }
}

pub fn all() -> Vec<Arc<dyn Module>> {
    let mut m = Vec::new();
    flow(&mut m);
    math(&mut m);
    compare(&mut m);
    logic(&mut m);
    value(&mut m);
    var(&mut m);
    list(&mut m);
    list_fns(&mut m);
    map(&mut m);
    strings(&mut m);
    io(&mut m);
    images(&mut m);
    graph(&mut m);
    tests(&mut m);
    m.push(
        b("note", "A note for whoever reads the canvas, like a post-it: it does nothing. Double click it to write")
            .param("text", ParamKind::Text, "")
            .fixed(Signature::new())
            .done(),
    );
    m
}

fn flow(m: &mut Vec<Arc<dyn Module>>) {
    m.push(
        b("flow/entry", "Where the program starts: ticks once when it does")
            .fixed(Signature::new().tick_out("then"))
            .start(|cx| Ok(vec![cx.fire(0)]))
            .done(),
    );
    m.push(
        b("flow/clock", "Ticks every `interval` seconds while the program runs, which is until it's stopped")
            .fixed(Signature::new().input("interval", float(), "1").tick_out("tick"))
            .clock(|cx| Ok((cx.input(0)?, vec![cx.fire(0)])))
            .done(),
    );
    m.push(
        b("flow/loop", "Ticks `body` for each `index` from `from` up to `to` (left out), then `done`")
            .fixed(
                Signature::new()
                    .tick_in("tick")
                    .input("from", int(), "0")
                    .input("to", int(), "10")
                    .tick_out("body")
                    .output("index", int())
                    .tick_out("done"),
            )
            .tick(|cx, _| {
                let (from, to) = (cx.input(1)?, cx.input(2)?);
                let l = cx.local(Type::Int);
                let index = cx.out_slot(1);
                let body = vec![Stmt::Set(index, Expr::Local(l)), cx.fire(0)];
                Ok(vec![Stmt::For { local: l, from, to, body }, cx.fire(2)])
            })
            .done(),
    );
    m.push(
        b("flow/while", "Ticks `body` while `condition` is true, then `done`")
            .fixed(
                Signature::new()
                    .tick_in("tick")
                    .input("condition", boolean(), "false")
                    .tick_out("body")
                    .tick_out("done"),
            )
            .tick(|cx, _| Ok(vec![Stmt::While(cx.input(1)?, vec![cx.fire(0)]), cx.fire(1)]))
            .done(),
    );
    m.push(
        b("flow/ifelse", "Ticks `then` if `condition` is true, `else` if not")
            .fixed(
                Signature::new()
                    .tick_in("tick")
                    .input("condition", boolean(), "false")
                    .tick_out("then")
                    .tick_out("else"),
            )
            .tick(|cx, _| Ok(vec![Stmt::If(cx.input(1)?, vec![cx.fire(0)], vec![cx.fire(1)])]))
            .done(),
    );
    m.push(
        b("flow/counter", "Counts its ticks: `count` goes up by one each tick, and back to 0 on `reset`")
            .fixed(Signature::new().tick_in("tick").tick_in("reset").tick_out("then").output("count", int()))
            .tick(|cx, input| {
                let c = cx.out_slot(1);
                Ok(if input == 0 {
                    vec![Stmt::Set(c, Expr::bin(Bin::Add(Num::Int), Expr::Slot(c), Expr::int(1))), cx.fire(0)]
                } else {
                    vec![Stmt::Set(c, Expr::int(0))]
                })
            })
            .done(),
    );
    m.push(
        b("flow/sequence", "Ticks its outputs one after the other, each once the one before is done")
            .param("count", ParamKind::Int, "2")
            .sig(|p, _| {
                let mut s = Signature::new().tick_in("tick");
                for i in 1..=p.int("count").clamp(1, 32) {
                    s = s.tick_out(&i.to_string());
                }
                Ok(s)
            })
            .tick(|cx, _| Ok((0..cx.sig().outputs.len()).map(|o| cx.fire(o)).collect()))
            .done(),
    );
    m.push(
        b("flow/stop", "Ends the program: every branch, and the clocks")
            .fixed(Signature::new().tick_in("tick"))
            .tick(|_, _| Ok(vec![Stmt::Stop]))
            .done(),
    );
    m.push(
        b("flow/select", "`then` if `condition` is true, `else` if not (values, not ticks)")
            .fixed(
                Signature::new()
                    .vars(&[Class::Any])
                    .input("condition", boolean(), "false")
                    .input("then", v(0), "")
                    .input("else", v(0), "")
                    .output("value", v(0)),
            )
            .eval(|cx, _| Ok(Expr::If(Box::new(cx.input(0)?), Box::new(cx.input(1)?), Box::new(cx.input(2)?))))
            .done(),
    );
}

fn math(m: &mut Vec<Arc<dyn Module>>) {
    let binops: &[(&str, &str, fn(Num) -> Bin, &str, &str)] = &[
        ("add", "a + b", Bin::Add, "0", "0"),
        ("subtract", "a - b", Bin::Sub, "0", "0"),
        ("multiply", "a × b", Bin::Mul, "1", "1"),
        ("divide", "a / b (ints divide as ints; by 0 gives 0)", Bin::Div, "1", "1"),
        ("modulo", "What's left of a / b, with the sign of b", Bin::Mod, "0", "1"),
        ("power", "a to the b", Bin::Pow, "2", "2"),
        ("min", "The smaller of a and b", Bin::Min, "0", "0"),
        ("max", "The bigger of a and b", Bin::Max, "0", "0"),
    ];
    for &(name, doc, op, da, db) in binops {
        m.push(
            b(&format!("math/{name}"), doc)
                .fixed(
                    Signature::new()
                        .vars(&[Class::Num])
                        .input("a", v(0), da)
                        .input("b", v(0), db)
                        .output("result", v(0)),
                )
                .eval(move |cx, _| Ok(Expr::bin(op(Num::of(&cx.out_ty(0))), cx.input(0)?, cx.input(1)?)))
                .done(),
        );
    }
    let unops: &[(&str, &str, fn(Num) -> Un)] =
        &[("negate", "-value", Un::Neg), ("absolute", "The value without its sign", Un::Abs)];
    for &(name, doc, op) in unops {
        m.push(
            b(&format!("math/{name}"), doc)
                .fixed(Signature::new().vars(&[Class::Num]).input("value", v(0), "0").output("result", v(0)))
                .eval(move |cx, _| Ok(Expr::un(op(Num::of(&cx.out_ty(0))), cx.input(0)?)))
                .done(),
        );
    }
    for (name, doc, op) in [
        ("floor", "The int at or below", Un::Floor),
        ("ceil", "The int at or above", Un::Ceil),
        ("round", "The closest int (halves away from 0)", Un::Round),
        ("truncate", "The int toward 0", Un::Trunc),
    ] {
        m.push(
            b(&format!("math/{name}"), doc)
                .fixed(Signature::new().input("value", float(), "0").output("result", int()))
                .eval(move |cx, _| Ok(Expr::un(op, cx.input(0)?)))
                .done(),
        );
    }
    let names: Vec<String> = MathFn::ALL.iter().map(|(n, _)| n.to_string()).collect();
    m.push(
        b("math/function", "sqrt, sin, cos, tan, asin, acos, atan, exp, ln or log10 of the value")
            .param("function", ParamKind::Choice(names), "sqrt")
            .fixed(Signature::new().input("value", float(), "0").output("result", float()))
            .eval(|cx, _| {
                let f = cx.params().str("function");
                let (_, f) = MathFn::ALL.iter().find(|(n, _)| *n == f).ok_or_else(|| format!("no function `{f}`"))?;
                Ok(Expr::un(Un::Math(*f), cx.input(0)?))
            })
            .done(),
    );
    m.push(
        b("math/clamp", "The value, kept from min to max")
            .fixed(
                Signature::new()
                    .vars(&[Class::Num])
                    .input("value", v(0), "0")
                    .input("min", v(0), "0")
                    .input("max", v(0), "1")
                    .output("result", v(0)),
            )
            .eval(|cx, _| {
                Ok(Expr::call(Func::Clamp(Num::of(&cx.out_ty(0))), vec![cx.input(0)?, cx.input(1)?, cx.input(2)?]))
            })
            .done(),
    );
    m.push(
        b(
            "math/random",
            "A random number from min to max: both in for ints, max left out for floats. A new one each read",
        )
        .fixed(
            Signature::new()
                .vars(&[Class::Num])
                .input("min", v(0), "0")
                .input("max", v(0), "100")
                .output("value", v(0)),
        )
        .eval(|cx, _| Ok(Expr::call(Func::Random(Num::of(&cx.out_ty(0))), vec![cx.input(0)?, cx.input(1)?])))
        .done(),
    );
    m.push(
        b("math/pi", "π")
            .fixed(Signature::new().output("value", float()))
            .eval(|_, _| Ok(Expr::Const(Value::Float(std::f64::consts::PI))))
            .done(),
    );
}

fn compare(m: &mut Vec<Arc<dyn Module>>) {
    let ops: &[(&str, &str, fn(Type) -> Bin, Class)] = &[
        ("equal", "a = b", Bin::Eq, Class::Any),
        ("not_equal", "a ≠ b", Bin::Ne, Class::Any),
        ("less", "a < b", Bin::Lt, Class::Ord),
        ("less_equal", "a ≤ b", Bin::Le, Class::Ord),
        ("greater", "a > b", Bin::Gt, Class::Ord),
        ("greater_equal", "a ≥ b", Bin::Ge, Class::Ord),
    ];
    for &(name, doc, op, class) in ops {
        m.push(
            b(&format!("compare/{name}"), doc)
                .fixed(
                    Signature::new()
                        .vars(&[class])
                        .input("a", v(0), "0")
                        .input("b", v(0), "0")
                        .output("result", boolean()),
                )
                .eval(move |cx, _| Ok(Expr::bin(op(cx.in_ty(0)), cx.input(0)?, cx.input(1)?)))
                .done(),
        );
    }
}

fn logic(m: &mut Vec<Arc<dyn Module>>) {
    let ops: &[(&str, &str, Bin, bool)] = &[
        ("and", "a and b", Bin::And, false),
        ("or", "a or b", Bin::Or, false),
        ("xor", "a or b, not both", Bin::Xor, false),
        ("nand", "not (a and b)", Bin::And, true),
        ("nor", "not (a or b)", Bin::Or, true),
        ("xnor", "a equals b", Bin::Xor, true),
    ];
    for (name, doc, op, negate) in ops {
        let (op, negate) = (op.clone(), *negate);
        m.push(
            b(&format!("logic/{name}"), doc)
                .fixed(
                    Signature::new()
                        .input("a", boolean(), "false")
                        .input("b", boolean(), "false")
                        .output("result", boolean()),
                )
                .eval(move |cx, _| {
                    let e = Expr::bin(op.clone(), cx.input(0)?, cx.input(1)?);
                    Ok(if negate { Expr::un(Un::Not, e) } else { e })
                })
                .done(),
        );
    }
    m.push(
        b("logic/not", "true if false, false if true")
            .fixed(Signature::new().input("value", boolean(), "false").output("result", boolean()))
            .eval(|cx, _| Ok(Expr::un(Un::Not, cx.input(0)?)))
            .done(),
    );
}

fn value(m: &mut Vec<Arc<dyn Module>>) {
    m.push(
        b("value/constant", "A value, of the type you choose")
            .param("type", types_choice(), "int")
            .param("value", ParamKind::Text, "0")
            .sig(|p, cx| Ok(Signature::new().output("value", t(cx.parse_type(&p.str("type"))?))))
            .eval(|cx, _| {
                let ty = cx.out_ty(0);
                let text = cx.params().str("value");
                if !ty.is_scalar() {
                    return Ok(Expr::Default(ty));
                }
                Value::parse(&text, &ty).map(Expr::Const).ok_or_else(|| format!("`{text}` isn't a {ty}"))
            })
            .done(),
    );
    m.push(
        b("value/convert", "The value, made the type you choose: numbers from strings, strings from anything...")
            .param("to", ParamKind::Choice(vec!["int".into(), "float".into(), "bool".into(), "str".into()]), "str")
            .sig(|p, cx| {
                let to = cx.parse_type(&p.str("to"))?;
                Ok(Signature::new().vars(&[Class::Any]).input("value", v(0), "0").output("value", t(to)))
            })
            .eval(|cx, _| convert(cx.input(0)?, &cx.in_ty(0), &cx.out_ty(0)))
            .done(),
    );
    m.push(
        b("value/time", "Seconds since the program started")
            .fixed(Signature::new().output("seconds", float()))
            .eval(|_, _| Ok(Expr::call(Func::Time, vec![])))
            .done(),
    );
}

fn var_param() -> ParamKind {
    ParamKind::Var
}

fn var(m: &mut Vec<Arc<dyn Module>>) {
    m.push(
        b(
            "var/var",
            "Declares a variable: its name, type and value when the program starts. `::name` is global; other names are \
             the canvas's own (each copy of a subgraph has its own)",
        )
        .param("name", ParamKind::Text, "x")
        .param("type", types_choice(), "int")
        .param("initial", ParamKind::Text, "0")
        .sig(|p, cx| Ok(Signature::new().output("value", t(cx.var_type(&p.str("name"))?))))
        .eval(|cx, _| {
            let name = cx.params().str("name");
            Ok(Expr::Slot(cx.var(&name)?))
        })
        .done(),
    );
    m.push(
        b("var/get", "The value of a variable")
            .param("name", var_param(), "x")
            .sig(|p, cx| Ok(Signature::new().output("value", t(cx.var_type(&p.str("name"))?))))
            .eval(|cx, _| {
                let name = cx.params().str("name");
                Ok(Expr::Slot(cx.var(&name)?))
            })
            .done(),
    );
    m.push(
        b("var/set", "Sets a variable")
            .param("name", var_param(), "x")
            .sig(|p, cx| {
                let ty = cx.var_type(&p.str("name"))?;
                Ok(value_in(Signature::new().tick_in("tick"), "value", &ty).tick_out("then"))
            })
            .tick(|cx, _| {
                let name = cx.params().str("name");
                let s = cx.var(&name)?;
                Ok(vec![Stmt::Set(s, cx.input(1)?), cx.fire(0)])
            })
            .done(),
    );
}

/// The type of the items of the list variable named by param `list`.
fn list_elem(p: &Params, cx: &dyn SigCx) -> Result<Type, String> {
    let name = p.str("list");
    match cx.var_type(&name)? {
        Type::List(e) => Ok(*e),
        t => Err(format!("`{name}` is a {t}, not a list")),
    }
}

fn map_elem(p: &Params, cx: &dyn SigCx) -> Result<Type, String> {
    let name = p.str("map");
    match cx.var_type(&name)? {
        Type::Map(e) => Ok(*e),
        t => Err(format!("`{name}` is a {t}, not a map")),
    }
}

fn list(m: &mut Vec<Arc<dyn Module>>) {
    fn lb(path: &str, doc: &str) -> B {
        b(path, doc).param("list", var_param(), "items")
    }
    fn slot(cx: &mut Lx, key: &str) -> Result<SlotId, String> {
        let name = cx.params().str(key);
        cx.var(&name)
    }
    m.push(
        lb("list/push", "Adds the value at the end of the list")
            .sig(|p, cx| Ok(value_in(Signature::new().tick_in("tick"), "value", &list_elem(p, cx)?).tick_out("then")))
            .tick(|cx, _| Ok(vec![Stmt::Do(Effect::ListPush(slot(cx, "list")?), vec![cx.input(1)?]), cx.fire(0)]))
            .done(),
    );
    m.push(
        lb("list/pop", "Takes the last value out of the list (the default if it's empty)")
            .sig(|p, cx| Ok(Signature::new().tick_in("tick").tick_out("then").output("value", t(list_elem(p, cx)?))))
            .tick(|cx, _| {
                let (list, into) = (slot(cx, "list")?, cx.out_slot(1));
                Ok(vec![Stmt::Do(Effect::ListPop { list, into }, vec![]), cx.fire(0)])
            })
            .done(),
    );
    m.push(
        lb("list/insert", "Puts the value before `index` (negative: from the end)")
            .sig(|p, cx| {
                let s = Signature::new().tick_in("tick").input("index", int(), "0");
                Ok(value_in(s, "value", &list_elem(p, cx)?).tick_out("then"))
            })
            .tick(|cx, _| {
                Ok(vec![Stmt::Do(Effect::ListInsert(slot(cx, "list")?), vec![cx.input(1)?, cx.input(2)?]), cx.fire(0)])
            })
            .done(),
    );
    m.push(
        lb("list/set", "Sets the value at `index` (negative: from the end)")
            .sig(|p, cx| {
                let s = Signature::new().tick_in("tick").input("index", int(), "0");
                Ok(value_in(s, "value", &list_elem(p, cx)?).tick_out("then"))
            })
            .tick(|cx, _| {
                Ok(vec![Stmt::Do(Effect::ListSet(slot(cx, "list")?), vec![cx.input(1)?, cx.input(2)?]), cx.fire(0)])
            })
            .done(),
    );
    m.push(
        lb("list/remove", "Takes the value at `index` out of the list")
            .sig(|_, _| Ok(Signature::new().tick_in("tick").input("index", int(), "0").tick_out("then")))
            .tick(|cx, _| Ok(vec![Stmt::Do(Effect::ListRemove(slot(cx, "list")?), vec![cx.input(1)?]), cx.fire(0)]))
            .done(),
    );
    m.push(
        lb("list/clear", "Empties the list")
            .sig(|_, _| Ok(Signature::new().tick_in("tick").tick_out("then")))
            .tick(|cx, _| Ok(vec![Stmt::Do(Effect::ListClear(slot(cx, "list")?), vec![]), cx.fire(0)]))
            .done(),
    );
    m.push(
        lb("list/get", "The value at `index` (negative: from the end; the default outside the list)")
            .sig(|p, cx| Ok(Signature::new().input("index", int(), "0").output("value", t(list_elem(p, cx)?))))
            .eval(|cx, _| Ok(Expr::ListGet(slot(cx, "list")?, Box::new(cx.input(0)?))))
            .done(),
    );
    m.push(
        lb("list/length", "How many values the list has")
            .sig(|p, cx| list_elem(p, cx).map(|_| Signature::new().output("length", int())))
            .eval(|cx, _| Ok(Expr::ListLen(slot(cx, "list")?)))
            .done(),
    );
    m.push(
        lb("list/foreach", "Ticks `body` for each value of the list, as it was when it started, then `done`")
            .sig(|p, cx| {
                let e = list_elem(p, cx)?;
                Ok(Signature::new()
                    .tick_in("tick")
                    .tick_out("body")
                    .output("value", t(e))
                    .output("index", int())
                    .tick_out("done"))
            })
            .tick(|cx, _| {
                let list = slot(cx, "list")?;
                let l = cx.local(Type::Int);
                let (value, index) = (cx.out_slot(1), cx.out_slot(2));
                let body = vec![
                    Stmt::Set(value, Expr::ListGet(list, Box::new(Expr::Local(l)))),
                    Stmt::Set(index, Expr::Local(l)),
                    cx.fire(0),
                ];
                Ok(vec![Stmt::For { local: l, from: Expr::int(0), to: Expr::ListLen(list), body }, cx.fire(3)])
            })
            .done(),
    );
    m.push(
        b("list/at", "The value at `index` of a list that comes by wire (negative: from the end)")
            .fixed(
                Signature::new()
                    .vars(&[Class::Any])
                    .wire_in("list", TExpr::ListOf(0))
                    .input("index", int(), "0")
                    .output("value", v(0)),
            )
            .eval(|cx, _| Ok(Expr::call(Func::ListAt(cx.out_ty(0)), vec![cx.input(0)?, cx.input(1)?])))
            .done(),
    );
    m.push(
        b("list/size", "How many values a list that comes by wire has")
            .fixed(Signature::new().vars(&[Class::Any]).wire_in("list", TExpr::ListOf(0)).output("length", int()))
            .eval(|cx, _| Ok(Expr::call(Func::ListSize, vec![cx.input(0)?])))
            .done(),
    );
}

fn list_fns(m: &mut Vec<Arc<dyn Module>>) {
    fn fb(path: &str, doc: &str, args: &[&str]) -> B {
        b(path, doc).param("fn", ParamKind::Function { args: args.iter().map(|a| a.to_string()).collect() }, "")
    }
    /// The pins of the function param, which has to take INPUTS values and give one.
    fn ports(p: &Params, cx: &dyn SigCx, inputs: usize) -> Result<FnPorts, String> {
        let path = p.str("fn");
        let f = cx.function(&path)?;
        if f.inputs.len() != inputs || f.outputs.len() != 1 {
            let s = |n: usize| if n == 1 { "" } else { "s" };
            return Err(format!(
                "`{path}` has {} data input{} and {} output{}, and a function here takes {inputs} input{} and gives 1 output",
                f.inputs.len(),
                s(f.inputs.len()),
                f.outputs.len(),
                s(f.outputs.len()),
                s(inputs)
            ));
        }
        Ok(f)
    }
    fn list_of(t: &Type) -> Type {
        Type::List(Box::new(t.clone()))
    }
    /// The pins every one of them has, from the function F: `tick` and the `list` come in; `call`
    /// ticks the function, if it has a tick input; `done` is the end.
    fn start(f: &FnPorts) -> Signature {
        Signature::new().tick_in("tick").wire_in("list", t(list_of(&f.inputs[f.inputs.len() - 1].1)))
    }
    fn end(f: &FnPorts, s: Signature) -> Signature {
        let s = if f.tick { s.tick_out("call") } else { s };
        s.tick_out("done")
    }
    /// The loop over the items of SNAP: VALUE gets each, PRE runs, then the function is
    /// called (if it ticks), then POST.
    fn per_item(cx: &mut Lx, snap: SlotId, value: SlotId, pre: Vec<Stmt>, post: Vec<Stmt>) -> Stmt {
        let i = cx.local(Type::Int);
        let mut body = vec![Stmt::Set(value, Expr::ListGet(snap, Box::new(Expr::Local(i))))];
        body.extend(pre);
        if let Some(call) = cx.sig().output_index("call") {
            body.push(cx.fire(call));
        }
        body.extend(post);
        Stmt::For { local: i, from: Expr::int(0), to: Expr::ListLen(snap), body }
    }
    /// The slot that keeps the list as it was when the loop started.
    fn snapshot(cx: &mut Lx, item: Type) -> SlotId {
        let ty = list_of(&item);
        let init = Value::default_of(&ty);
        cx.private_slot("items", ty, init)
    }
    fn done(cx: &mut Lx) -> Stmt {
        let o = cx.sig().output_index("done").unwrap_or(0);
        cx.fire(o)
    }

    m.push(
        fb(
            "list/map",
            "A list of what `function` (a subgraph: item in, value out) gives for each item of `list`",
            &["value"],
        )
        .sig(|p, cx| {
            let f = ports(p, cx, 1)?;
            let s = start(&f).output("value", t(f.inputs[0].1.clone())).output("result", t(list_of(&f.outputs[0].1)));
            Ok(end(&f, s))
        })
        .tick(|cx, _| {
            let (value, result) = (cx.out_slot(0), cx.out_slot(1));
            let item = cx.out_ty(0);
            let snap = snapshot(cx, item);
            let give = cx.fn_result(0)?;
            let post = vec![Stmt::Do(Effect::ListPush(result), vec![give])];
            Ok(vec![
                Stmt::Set(snap, cx.input(1)?),
                Stmt::Do(Effect::ListClear(result), vec![]),
                per_item(cx, snap, value, vec![], post),
                done(cx),
            ])
        })
        .done(),
    );
    m.push(
        fb(
            "list/filter",
            "The items of `list` for which `function` (a subgraph: item in, bool out) gives true",
            &["value"],
        )
        .sig(|p, cx| {
            let f = ports(p, cx, 1)?;
            if f.outputs[0].1 != Type::Bool {
                return Err(format!("`{}` gives {}, and a filter's function gives bool", p.str("fn"), f.outputs[0].1));
            }
            let s = start(&f).output("value", t(f.inputs[0].1.clone())).output("result", t(list_of(&f.inputs[0].1)));
            Ok(end(&f, s))
        })
        .tick(|cx, _| {
            let (value, result) = (cx.out_slot(0), cx.out_slot(1));
            let item = cx.out_ty(0);
            let snap = snapshot(cx, item);
            let keep = cx.fn_result(0)?;
            let post = vec![Stmt::If(keep, vec![Stmt::Do(Effect::ListPush(result), vec![Expr::Slot(value)])], vec![])];
            Ok(vec![
                Stmt::Set(snap, cx.input(1)?),
                Stmt::Do(Effect::ListClear(result), vec![]),
                per_item(cx, snap, value, vec![], post),
                done(cx),
            ])
        })
        .done(),
    );
    m.push(
        fb(
            "list/reduce",
            "Folds `list` into one value: `function` (a subgraph: acc and item in, new acc out) is called for each item, starting from `initial`",
            &["acc", "value"],
        )
        .sig(|p, cx| {
            let f = ports(p, cx, 2)?;
            let acc = f.inputs[0].1.clone();
            if f.outputs[0].1 != acc {
                return Err(format!(
                    "`{}` gives {}, and its first input, the accumulator, is {acc}: they have to be the same type",
                    p.str("fn"),
                    f.outputs[0].1
                ));
            }
            let s = Signature::new().tick_in("tick").wire_in("list", t(list_of(&f.inputs[1].1)));
            let s = if acc.is_scalar() {
                let d = &f.inputs[0].2;
                s.input("initial", t(acc.clone()), if Value::parse(d, &acc).is_some() { d } else { "" })
            } else {
                s.wire_in("initial", t(acc.clone()))
            };
            Ok(end(&f, s.output("result", t(acc.clone())).output("acc", t(acc)).output("value", t(f.inputs[1].1.clone()))))
        })
        .tick(|cx, _| {
            let (result, acc, value) = (cx.out_slot(0), cx.out_slot(1), cx.out_slot(2));
            let item = cx.out_ty(2);
            let snap = snapshot(cx, item);
            let a = cx.local(cx.out_ty(0));
            let give = cx.fn_result(0)?;
            Ok(vec![
                Stmt::Set(snap, cx.input(1)?),
                Stmt::Let(a, cx.input(2)?),
                per_item(cx, snap, value, vec![Stmt::Set(acc, Expr::Local(a))], vec![Stmt::Let(a, give)]),
                Stmt::Set(result, Expr::Local(a)),
                done(cx),
            ])
        })
        .done(),
    );
}

fn map(m: &mut Vec<Arc<dyn Module>>) {
    fn mb(path: &str, doc: &str) -> B {
        b(path, doc).param("map", var_param(), "table")
    }
    fn slot(cx: &mut Lx) -> Result<SlotId, String> {
        let name = cx.params().str("map");
        cx.var(&name)
    }
    m.push(
        mb("map/set", "Sets the value of `key`")
            .sig(|p, cx| {
                let s = Signature::new().tick_in("tick").input("key", string(), "");
                Ok(value_in(s, "value", &map_elem(p, cx)?).tick_out("then"))
            })
            .tick(|cx, _| Ok(vec![Stmt::Do(Effect::MapSet(slot(cx)?), vec![cx.input(1)?, cx.input(2)?]), cx.fire(0)]))
            .done(),
    );
    m.push(
        mb("map/delete", "Takes `key` out of the map")
            .sig(|p, cx| {
                map_elem(p, cx).map(|_| Signature::new().tick_in("tick").input("key", string(), "").tick_out("then"))
            })
            .tick(|cx, _| Ok(vec![Stmt::Do(Effect::MapDelete(slot(cx)?), vec![cx.input(1)?]), cx.fire(0)]))
            .done(),
    );
    m.push(
        mb("map/clear", "Empties the map")
            .sig(|p, cx| map_elem(p, cx).map(|_| Signature::new().tick_in("tick").tick_out("then")))
            .tick(|cx, _| Ok(vec![Stmt::Do(Effect::MapClear(slot(cx)?), vec![]), cx.fire(0)]))
            .done(),
    );
    m.push(
        mb("map/get", "The value of `key` (the default if it isn't there)")
            .sig(|p, cx| Ok(Signature::new().input("key", string(), "").output("value", t(map_elem(p, cx)?))))
            .eval(|cx, _| Ok(Expr::MapGet(slot(cx)?, Box::new(cx.input(0)?))))
            .done(),
    );
    m.push(
        mb("map/has", "Whether `key` is in the map")
            .sig(|p, cx| {
                map_elem(p, cx).map(|_| Signature::new().input("key", string(), "").output("result", boolean()))
            })
            .eval(|cx, _| Ok(Expr::MapHas(slot(cx)?, Box::new(cx.input(0)?))))
            .done(),
    );
    m.push(
        mb("map/keys", "The keys, in order")
            .sig(|p, cx| map_elem(p, cx).map(|_| Signature::new().output("keys", t(Type::List(Box::new(Type::Str))))))
            .eval(|cx, _| Ok(Expr::MapKeys(slot(cx)?)))
            .done(),
    );
    m.push(
        mb("map/length", "How many keys the map has")
            .sig(|p, cx| map_elem(p, cx).map(|_| Signature::new().output("length", int())))
            .eval(|cx, _| Ok(Expr::MapLen(slot(cx)?)))
            .done(),
    );
}

fn strings(m: &mut Vec<Arc<dyn Module>>) {
    m.push(
        b("string/concat", "a then b, as text (any type)")
            .fixed(
                Signature::new()
                    .vars(&[Class::Any, Class::Any])
                    .input("a", v(0), "")
                    .input("b", v(1), "")
                    .output("result", string()),
            )
            .eval(|cx, _| {
                let (a, b) = (text(cx.input(0)?, &cx.in_ty(0)), text(cx.input(1)?, &cx.in_ty(1)));
                Ok(Expr::call(Func::Concat, vec![a, b]))
            })
            .done(),
    );
    m.push(
        b("string/format", "The format, with each {} replaced by an input, of any type ({{ and }} are braces)")
            .param("format", ParamKind::Text, "{} and {}")
            .sig(|p, _| {
                let n = lineit_rt::text::format_count(&p.str("format")).min(32);
                let mut s = Signature::new().vars(&vec![Class::Any; n]);
                for i in 0..n {
                    s = s.input(&(i + 1).to_string(), v(i), "");
                }
                Ok(s.output("text", string()))
            })
            .eval(|cx, _| {
                let mut args = vec![Expr::str(&cx.params().str("format"))];
                for i in 0..cx.sig().inputs.len() {
                    args.push(cx.input(i)?);
                }
                Ok(Expr::call(Func::Format, args))
            })
            .done(),
    );
    let simple: &[(&str, &str, Func, &[(&str, TExpr, &str)], TExpr)] = &[
        (
            "length",
            "How many characters it has",
            Func::StrLen,
            &[("text", TExpr::T(Type::Str), "")],
            TExpr::T(Type::Int),
        ),
        ("upper", "In capitals", Func::Upper, &[("text", TExpr::T(Type::Str), "")], TExpr::T(Type::Str)),
        ("lower", "In lower case", Func::Lower, &[("text", TExpr::T(Type::Str), "")], TExpr::T(Type::Str)),
        (
            "trim",
            "Without spaces at the start and end",
            Func::Trim,
            &[("text", TExpr::T(Type::Str), "")],
            TExpr::T(Type::Str),
        ),
        (
            "find",
            "Where `part` first is in `text`, in characters from 0, or -1",
            Func::Find,
            &[("text", TExpr::T(Type::Str), ""), ("part", TExpr::T(Type::Str), "")],
            TExpr::T(Type::Int),
        ),
        (
            "substring",
            "`length` characters from `start` (negative: from the end)",
            Func::Substring,
            &[
                ("text", TExpr::T(Type::Str), ""),
                ("start", TExpr::T(Type::Int), "0"),
                ("length", TExpr::T(Type::Int), "1"),
            ],
            TExpr::T(Type::Str),
        ),
        (
            "replace",
            "`text` with every `from` replaced by `to`",
            Func::Replace,
            &[("text", TExpr::T(Type::Str), ""), ("from", TExpr::T(Type::Str), ""), ("to", TExpr::T(Type::Str), "")],
            TExpr::T(Type::Str),
        ),
        (
            "repeat",
            "`text`, `count` times",
            Func::Repeat,
            &[("text", TExpr::T(Type::Str), ""), ("count", TExpr::T(Type::Int), "2")],
            TExpr::T(Type::Str),
        ),
        (
            "split",
            "The parts of `text` between each `separator` (empty: each character)",
            Func::Split,
            &[("text", TExpr::T(Type::Str), ""), ("separator", TExpr::T(Type::Str), ",")],
            TExpr::T(Type::List(Box::new(Type::Str))),
        ),
    ];
    for (name, doc, f, ins, out) in simple {
        let mut s = Signature::new();
        for (n, ty, d) in ins.iter() {
            s = s.input(n, ty.clone(), d);
        }
        let s = s.output("result", out.clone());
        let f = f.clone();
        m.push(
            b(&format!("string/{name}"), doc)
                .fixed(s)
                .eval(move |cx, _| {
                    let args = (0..cx.sig().inputs.len()).map(|i| cx.input(i)).collect::<Result<_, _>>()?;
                    Ok(Expr::call(f.clone(), args))
                })
                .done(),
        );
    }
    m.push(
        b("string/join", "The strings of the list, with `separator` between them")
            .fixed(
                Signature::new()
                    .wire_in("list", t(Type::List(Box::new(Type::Str))))
                    .input("separator", string(), ", ")
                    .output("result", string()),
            )
            .eval(|cx, _| Ok(Expr::call(Func::Join, vec![cx.input(0)?, cx.input(1)?])))
            .done(),
    );
}

fn io(m: &mut Vec<Arc<dyn Module>>) {
    m.push(
        b("io/print", "Prints the value (of any type) on a line of its own")
            .fixed(
                Signature::new()
                    .vars(&[Class::Any])
                    .tick_in("tick")
                    .input("value", v(0), "Hello, world!")
                    .tick_out("then"),
            )
            .tick(|cx, _| {
                let e = text(cx.input(1)?, &cx.in_ty(1));
                Ok(vec![Stmt::Print(e), cx.fire(0)])
            })
            .done(),
    );
    m.push(
        b("io/display", "Shows the value on the square each time it ticks, and passes it on")
            .fixed(
                Signature::new()
                    .vars(&[Class::Any])
                    .tick_in("tick")
                    .input("value", v(0), "")
                    .tick_out("then")
                    .output("value", v(0)),
            )
            .tick(|cx, _| {
                let s = cx.out_slot(1);
                Ok(vec![Stmt::Set(s, cx.input(1)?), cx.fire(0)])
            })
            .done(),
    );
}

fn images(m: &mut Vec<Arc<dyn Module>>) {
    fn path(e: Expr) -> Expr {
        Expr::call(Func::Path, vec![e])
    }
    m.push(
        b("image/open", "Reads an image file: png, jpg, gif, bmp, qoi or tga (empty if it can't)")
            .fixed(Signature::new().input("path", string(), "").output("image", image()))
            .eval(|cx, _| Ok(Expr::call(Func::ImgOpen, vec![path(cx.input(0)?)])))
            .done(),
    );
    m.push(
        b("image/save", "Writes the image, in the format of the path's extension: png, jpg, bmp, qoi, tga or gif")
            .fixed(
                Signature::new()
                    .tick_in("tick")
                    .wire_in("image", image())
                    .input("path", string(), "out.png")
                    .tick_out("then")
                    .output("ok", boolean()),
            )
            .tick(|cx, _| {
                let ok = cx.out_slot(1);
                Ok(vec![Stmt::Do(Effect::ImageSave { ok }, vec![cx.input(1)?, path(cx.input(2)?)]), cx.fire(0)])
            })
            .done(),
    );
    m.push(
        b("image/new", "An image of one color (#rgb, #rrggbb, #rrggbbaa or a name)")
            .fixed(
                Signature::new()
                    .input("width", int(), "64")
                    .input("height", int(), "64")
                    .input("color", string(), "#000000")
                    .output("image", image()),
            )
            .eval(|cx, _| Ok(Expr::call(Func::ImgNew, vec![cx.input(0)?, cx.input(1)?, cx.input(2)?])))
            .done(),
    );
    m.push(
        b("image/size", "Its width and height, in pixels")
            .fixed(Signature::new().wire_in("image", image()).output("width", int()).output("height", int()))
            .eval(|cx, o| Ok(Expr::call(if o == 0 { Func::ImgWidth } else { Func::ImgHeight }, vec![cx.input(0)?])))
            .done(),
    );
    m.push(
        b("image/pixel", "The color at x, y, as #rrggbbaa (empty outside the image)")
            .fixed(
                Signature::new()
                    .wire_in("image", image())
                    .input("x", int(), "0")
                    .input("y", int(), "0")
                    .output("color", string()),
            )
            .eval(|cx, _| Ok(Expr::call(Func::ImgPixel, vec![cx.input(0)?, cx.input(1)?, cx.input(2)?])))
            .done(),
    );
    let filters: &[(&str, &str, Func, &[(&str, Type, &str)])] = &[
        ("grayscale", "In shades of gray", Func::ImgGray, &[]),
        ("invert", "Every color turned into its opposite", Func::ImgInvert, &[]),
        (
            "resize",
            "Stretched to width x height",
            Func::ImgResize,
            &[("width", Type::Int, "32"), ("height", Type::Int, "32")],
        ),
        (
            "crop",
            "The part from x, y, of width x height",
            Func::ImgCrop,
            &[("x", Type::Int, "0"), ("y", Type::Int, "0"), ("width", Type::Int, "32"), ("height", Type::Int, "32")],
        ),
        ("blur", "Blurred, `radius` pixels around", Func::ImgBlur, &[("radius", Type::Int, "2")]),
        (
            "brightness",
            "Lighter (positive, up to 1) or darker (negative)",
            Func::ImgBrightness,
            &[("amount", Type::Float, "0.1")],
        ),
        ("contrast", "More contrast (above 1) or less (below)", Func::ImgContrast, &[("factor", Type::Float, "1.2")]),
        (
            "tint",
            "Mixed with a color, by `amount` (0 to 1)",
            Func::ImgTint,
            &[("color", Type::Str, "#ff0000"), ("amount", Type::Float, "0.5")],
        ),
    ];
    for (name, doc, f, ins) in filters {
        let mut s = Signature::new().wire_in("image", image());
        for (n, ty, d) in ins.iter() {
            s = s.input(n, t(ty.clone()), d);
        }
        let s = s.output("image", image());
        let f = f.clone();
        m.push(
            b(&format!("image/{name}"), doc)
                .fixed(s)
                .eval(move |cx, _| {
                    let args = (0..cx.sig().inputs.len()).map(|i| cx.input(i)).collect::<Result<_, _>>()?;
                    Ok(Expr::call(f.clone(), args))
                })
                .done(),
        );
    }
    m.push(
        b(
            "image/shader",
            "Through a shader, a file in Godot's language (gdshader, shader_type canvas_item), run on the CPU. Its uniforms are inputs; TIME is what TIME reads",
        )
        .param("file", ParamKind::Text, "")
        .sig(|p, cx| {
            // no file: the image as it is
            let file = p.str("file");
            let file = file.trim();
            let uniforms = if file.is_empty() { Vec::new() } else { lineit_rt::shader::uniforms(&cx.read(file)?)? };
            let mut s = Signature::new().wire_in("TEXTURE", image()).input("TIME", float(), "0");
            let mut seen = vec!["TEXTURE".to_string(), "TIME".to_string()];
            for u in uniforms {
                use lineit_rt::shader::Kind;
                if seen.contains(&u.name) {
                    return Err(format!("two inputs would be called {}", u.name));
                }
                seen.push(u.name.clone());
                s = match u.kind {
                    Kind::Float => s.input(&u.name, float(), &u.default),
                    Kind::Int => s.input(&u.name, int(), &u.default),
                    Kind::Bool => s.input(&u.name, boolean(), &u.default),
                    Kind::Color(_) | Kind::Numbers(_) => s.input(&u.name, string(), &u.default),
                    Kind::Image => s.wire_in(&u.name, image()),
                };
            }
            Ok(s.output("image", image()))
        })
        .eval(|cx, _| {
            let n = cx.sig().inputs.len();
            let file = cx.params().str("file");
            let mut args = vec![cx.input(0)?, path(Expr::str(file.trim())), cx.input(1)?];
            for i in 2..n {
                args.push(cx.input(i)?);
            }
            Ok(Expr::call(Func::ImgShader, args))
        })
        .done(),
    );
    m.push(
        b("image/flip", "Mirrored: left to right, or upside down")
            .param("direction", ParamKind::Choice(vec!["horizontal".into(), "vertical".into()]), "horizontal")
            .fixed(Signature::new().wire_in("image", image()).output("image", image()))
            .eval(|cx, _| {
                let vertical = cx.params().str("direction") == "vertical";
                Ok(Expr::call(Func::ImgFlip, vec![cx.input(0)?, Expr::bool(vertical)]))
            })
            .done(),
    );
    m.push(
        b("image/rotate", "Turned clockwise")
            .param("degrees", ParamKind::Choice(vec!["90".into(), "180".into(), "270".into()]), "90")
            .fixed(Signature::new().wire_in("image", image()).output("image", image()))
            .eval(|cx, _| {
                let quarters = cx.params().int("degrees") / 90;
                Ok(Expr::call(Func::ImgRotate, vec![cx.input(0)?, Expr::int(quarters)]))
            })
            .done(),
    );
}

fn tests(m: &mut Vec<Arc<dyn Module>>) {
    m.push(
        b(
            "test/case",
            "A test: Run tests starts the program here, once for each case. Its fields are the canvas's variables: what's typed in one is what that variable starts with (empty: what it starts with anyway); a list or record is wired in. A normal run doesn't start here",
        )
        .param("name", ParamKind::Text, "a test")
        .sig(|_, cx| {
            let mut s = Signature::new();
            for (name, ty) in cx.vars() {
                // empty: the variable as it is
                s = if ty.is_scalar() { s.input(&name, t(ty), "") } else { s.wire_in(&name, t(ty)) };
            }
            Ok(s.tick_out("then"))
        })
        .start(|cx| {
            if cx.test_case() != Some(cx.square()) {
                return Ok(Vec::new());
            }
            let mut b = Vec::new();
            for i in 0..cx.sig().inputs.len() {
                // what it gives: what's wired, or typed
                if cx.field_text(i).is_some_and(|t| t.trim().is_empty()) {
                    continue;
                }
                let name = cx.sig().inputs[i].name.clone();
                let s = cx.var(&name)?;
                b.push(Stmt::Set(s, cx.input(i)?));
            }
            b.push(cx.fire(0));
            Ok(b)
        })
        .done(),
    );
    m.push(
        b(
            "test/expect",
            "Checks that `value` is `expected` when a tick gets here, and goes on: Run tests says which checks failed, and the square shows how it went",
        )
        .param("label", ParamKind::Text, "")
        .sig(|_, _| {
            Ok(Signature::new()
                .vars(&[Class::Any])
                .tick_in("tick")
                .input("value", v(0), "")
                .input("expected", v(0), "")
                .tick_out("then"))
        })
        .tick(|cx, _| {
            // a normal run passes by: it checks in tests
            if cx.test_case().is_none() {
                return Ok(vec![cx.fire(0)]);
            }
            let label = cx.params().str("label");
            Ok(vec![
                Stmt::Check { square: cx.square(), label, value: cx.input(1)?, expected: cx.input(2)? },
                cx.fire(0),
            ])
        })
        .done(),
    );
}

fn graph(m: &mut Vec<Arc<dyn Module>>) {
    m.push(
        b(
            "graph/input",
            "An input of the subgraph this canvas is: its name, its type (or tick), and its value when nothing comes in",
        )
        .param("name", ParamKind::Text, "in")
        .param("type", ParamKind::Type { allow_tick: true }, "int")
        .param("default", ParamKind::Text, "0")
        .sig(|p, cx| {
            let ty = p.str("type");
            Ok(if ty == "tick" { Signature::new().tick_out("value") } else { Signature::new().output("value", t(cx.parse_type(&ty)?)) })
        })
        .done(),
    );
    m.push(
        b("graph/output", "An output of the subgraph this canvas is: its name and its type (or tick)")
            .param("name", ParamKind::Text, "out")
            .param("type", ParamKind::Type { allow_tick: true }, "int")
            .sig(|p, cx| {
                let ty = p.str("type");
                Ok(if ty == "tick" {
                    Signature::new().tick_in("value")
                } else {
                    Signature::new().wire_in("value", t(cx.parse_type(&ty)?))
                })
            })
            .done(),
    );
}

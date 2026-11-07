//! The IR: what a canvas means, with nothing left to look up.
//!
//! Lowering ([`crate::lower`]) makes it from the canvas: subgraphs are
//! inlined, every type is known, pure squares are expressions inside the
//! statements that read them, and each tick input that something fires is a
//! [`Handler`]. The interpreter ([`crate::interp`]) runs it and the codegen
//! ([`crate::codegen`]) turns it into Rust.

use std::sync::Arc;

use lineit_rt::{RecordType, Type, Value};

use crate::model::SquareId;

pub type SlotId = u32;
pub type LocalId = u32;
pub type HandlerId = u32;
pub type NodeId = u32;

#[derive(Clone, Debug, Default)]
pub struct Program {
    pub slots: Vec<SlotDef>,
    pub handlers: Vec<Handler>,
    /// Fired together (as branches, if there's more than one) when the
    /// program starts.
    pub starts: Fire,
    pub clocks: Vec<Clock>,
    pub nodes: Vec<NodeInfo>,
    pub exts: Vec<ExtDef>,
    pub ext_instances: Vec<ExtInstance>,
    pub records: Vec<Arc<RecordType>>,
}

/// State: the output of a square that runs on ticks, or a variable.
#[derive(Clone, Debug)]
pub struct SlotDef {
    pub ty: Type,
    pub init: Value,
    /// For the generated code and for people: `print_3_value`, `var_total`.
    pub name: String,
    /// The square (at the top of the canvas) and output it shows on, if any.
    pub shows: Option<(SquareId, String)>,
}

#[derive(Clone, Debug)]
pub struct Handler {
    pub node: NodeId,
    /// What it is, for the generated code: `loop_4_tick`.
    pub name: String,
    pub locals: Vec<Type>,
    pub body: Block,
}

#[derive(Clone, Debug)]
pub struct Clock {
    /// Seconds between ticks, read each time.
    pub interval: Expr,
    pub handler: HandlerId,
}

/// A square of the flattened canvas.
#[derive(Clone, Debug)]
pub struct NodeInfo {
    /// The square of the canvas it is, or is inside of (a subgraph square).
    pub square: SquareId,
    pub module: String,
}

/// A module that runs code of its own: Lua, or Rust from a package.
#[derive(Clone, Debug)]
pub struct ExtDef {
    pub path: String,
    pub kind: ExtKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExtKind {
    Lua {
        source: String,
    },
    /// Type TYPE of the crate in CRATE_DIR (a package's `native/`).
    Native {
        crate_dir: std::path::PathBuf,
        crate_name: String,
        ty: String,
    },
}

#[derive(Clone, Debug)]
pub struct ExtInstance {
    pub module: usize,
    pub params: Vec<(String, Value)>,
}

pub type Block = Vec<Stmt>;

#[derive(Clone, Debug)]
pub enum Stmt {
    /// A square starts running: Step stops before it.
    Enter(NodeId),
    Set(SlotId, Expr),
    Let(LocalId, Expr),
    /// Prints a line (a `str` expression).
    Print(Expr),
    Do(Effect, Vec<Expr>),
    If(Expr, Block, Block),
    While(Expr, Block),
    /// LOCAL goes from FROM up to TO, TO left out. Both are read once.
    For {
        local: LocalId,
        from: Expr,
        to: Expr,
        body: Block,
    },
    Fire(Fire),
    /// Ends the program: every branch, and the clocks.
    Stop,
    /// A Lua or Rust module's `tick` (or `start`, with no input).
    Ext {
        instance: usize,
        input: Option<String>,
        inputs: Vec<(String, Expr)>,
        outputs: Vec<(String, SlotId)>,
        fires: Vec<(String, Fire)>,
    },
}

/// Runs handlers. One is a call. More are branches, in parallel; MEETS are
/// the handlers two or more of them can reach, where they stop, and which
/// run once after they've all finished (if any got there).
#[derive(Clone, Debug, Default)]
pub struct Fire {
    pub targets: Vec<HandlerId>,
    /// Sorted.
    pub meets: Vec<HandlerId>,
}

/// What changes a slot in place, or the world. Arguments in the comments.
#[derive(Clone, Debug)]
pub enum Effect {
    ListPush(SlotId), // value
    ListPop { list: SlotId, into: SlotId },
    ListSet(SlotId),    // index, value
    ListInsert(SlotId), // index, value
    ListRemove(SlotId), // index
    ListClear(SlotId),
    MapSet(SlotId),    // key, value
    MapDelete(SlotId), // key
    MapClear(SlotId),
    ImageSave { ok: SlotId }, // image, path
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Num {
    Int,
    Float,
}

impl Num {
    pub fn of(t: &Type) -> Num {
        if *t == Type::Float { Num::Float } else { Num::Int }
    }
    pub fn ty(self) -> Type {
        match self {
            Num::Int => Type::Int,
            Num::Float => Type::Float,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Expr {
    /// A scalar.
    Const(Value),
    /// What a value of the type is before anything sets it.
    Default(Type),
    Slot(SlotId),
    Local(LocalId),
    Un(Un, Box<Expr>),
    Bin(Bin, Box<Expr>, Box<Expr>),
    Call(Func, Vec<Expr>),
    /// cond, then, else
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    ListGet(SlotId, Box<Expr>),
    ListLen(SlotId),
    MapGet(SlotId, Box<Expr>),
    MapHas(SlotId, Box<Expr>),
    MapKeys(SlotId),
    MapLen(SlotId),
    Record(Arc<RecordType>, Vec<Expr>),
    Field(Box<Expr>, Arc<RecordType>, usize),
    /// A Lua or Rust module's `eval`, and the output OUTPUT of what it returns.
    Ext {
        instance: usize,
        output: String,
        ty: Type,
        inputs: Vec<(String, Expr)>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Un {
    Neg(Num),
    Abs(Num),
    Not,
    /// int to float
    Widen,
    /// float to int, rounding toward zero
    Trunc,
    Floor,
    Ceil,
    Round,
    Math(MathFn),
    BoolToInt,
    IntToBool,
    FloatToBool,
    BoolToFloat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathFn {
    Sqrt,
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Exp,
    Ln,
    Log10,
}

impl MathFn {
    pub const ALL: &[(&str, MathFn)] = &[
        ("sqrt", MathFn::Sqrt),
        ("sin", MathFn::Sin),
        ("cos", MathFn::Cos),
        ("tan", MathFn::Tan),
        ("asin", MathFn::Asin),
        ("acos", MathFn::Acos),
        ("atan", MathFn::Atan),
        ("exp", MathFn::Exp),
        ("ln", MathFn::Ln),
        ("log10", MathFn::Log10),
    ];

    pub fn apply(self, x: f64) -> f64 {
        match self {
            MathFn::Sqrt => x.sqrt(),
            MathFn::Sin => x.sin(),
            MathFn::Cos => x.cos(),
            MathFn::Tan => x.tan(),
            MathFn::Asin => x.asin(),
            MathFn::Acos => x.acos(),
            MathFn::Atan => x.atan(),
            MathFn::Exp => x.exp(),
            MathFn::Ln => x.ln(),
            MathFn::Log10 => x.log10(),
        }
    }

    pub fn rust(self) -> &'static str {
        match self {
            MathFn::Sqrt => "sqrt",
            MathFn::Sin => "sin",
            MathFn::Cos => "cos",
            MathFn::Tan => "tan",
            MathFn::Asin => "asin",
            MathFn::Acos => "acos",
            MathFn::Atan => "atan",
            MathFn::Exp => "exp",
            MathFn::Ln => "ln",
            MathFn::Log10 => "log10",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Bin {
    Add(Num),
    Sub(Num),
    Mul(Num),
    Div(Num),
    Mod(Num),
    Pow(Num),
    Min(Num),
    Max(Num),
    /// On values of the type.
    Eq(Type),
    Ne(Type),
    Lt(Type),
    Le(Type),
    Gt(Type),
    Ge(Type),
    And,
    Or,
    Xor,
}

/// Calls into `lineit-rt`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Func {
    /// The value, as `io/print` prints it.
    ToStr(Type),
    StrToInt,
    StrToFloat,
    StrToBool,
    Concat,
    StrLen,
    Upper,
    Lower,
    Trim,
    Find,
    Substring,
    Split,
    Join,
    Replace,
    Repeat,
    /// The format, then one argument per `{}`, of any type.
    Format,
    Clamp(Num),
    Random(Num),
    /// Seconds since the program started.
    Time,
    /// A list value and an index: the item (of type ELEMENT).
    ListAt(Type),
    ListSize,
    /// A path, relative to the canvas in the editor and to the current
    /// folder in a compiled program.
    Path,
    ImgOpen,
    ImgNew,
    ImgWidth,
    ImgHeight,
    ImgPixel,
    ImgGray,
    ImgInvert,
    ImgFlip,
    ImgRotate,
    ImgResize,
    ImgCrop,
    ImgBlur,
    ImgBrightness,
    ImgContrast,
    ImgTint,
}

impl Expr {
    pub fn int(i: i64) -> Expr {
        Expr::Const(Value::Int(i))
    }
    pub fn bool(b: bool) -> Expr {
        Expr::Const(Value::Bool(b))
    }
    pub fn str(s: &str) -> Expr {
        Expr::Const(Value::Str(s.into()))
    }
    pub fn un(op: Un, a: Expr) -> Expr {
        Expr::Un(op, Box::new(a))
    }
    pub fn bin(op: Bin, a: Expr, b: Expr) -> Expr {
        Expr::Bin(op, Box::new(a), Box::new(b))
    }
    pub fn call(f: Func, args: Vec<Expr>) -> Expr {
        Expr::Call(f, args)
    }

    /// Its type, given the program's slots and the handler's locals.
    pub fn ty(&self, slots: &[SlotDef], locals: &[Type]) -> Type {
        let elem = |s: &SlotId| match &slots[*s as usize].ty {
            Type::List(t) | Type::Map(t) => (**t).clone(),
            _ => Type::Int,
        };
        match self {
            Expr::Const(v) => v.ty(),
            Expr::Default(t) => t.clone(),
            Expr::Slot(s) => slots[*s as usize].ty.clone(),
            Expr::Local(l) => locals[*l as usize].clone(),
            Expr::Un(op, _) => match op {
                Un::Neg(n) | Un::Abs(n) => n.ty(),
                Un::Not | Un::IntToBool | Un::FloatToBool => Type::Bool,
                Un::Widen | Un::Math(_) | Un::BoolToFloat => Type::Float,
                Un::Trunc | Un::Floor | Un::Ceil | Un::Round | Un::BoolToInt => Type::Int,
            },
            Expr::Bin(op, _, _) => match op {
                Bin::Add(n)
                | Bin::Sub(n)
                | Bin::Mul(n)
                | Bin::Div(n)
                | Bin::Mod(n)
                | Bin::Pow(n)
                | Bin::Min(n)
                | Bin::Max(n) => n.ty(),
                _ => Type::Bool,
            },
            Expr::Call(f, _) => match f {
                Func::ToStr(_)
                | Func::Concat
                | Func::Upper
                | Func::Lower
                | Func::Trim
                | Func::Substring
                | Func::Join
                | Func::Replace
                | Func::Repeat
                | Func::Format
                | Func::Path
                | Func::ImgPixel => Type::Str,
                Func::StrToInt | Func::StrLen | Func::Find | Func::ListSize | Func::ImgWidth | Func::ImgHeight => {
                    Type::Int
                }
                Func::StrToFloat | Func::Time => Type::Float,
                Func::StrToBool => Type::Bool,
                Func::Split => Type::List(Box::new(Type::Str)),
                Func::Clamp(n) | Func::Random(n) => n.ty(),
                Func::ListAt(t) => t.clone(),
                _ => Type::Image,
            },
            Expr::If(_, a, _) => a.ty(slots, locals),
            Expr::ListGet(s, _) | Expr::MapGet(s, _) => elem(s),
            Expr::ListLen(_) | Expr::MapLen(_) => Type::Int,
            Expr::MapHas(_, _) => Type::Bool,
            Expr::MapKeys(_) => Type::List(Box::new(Type::Str)),
            Expr::Record(r, _) => Type::Record(r.clone()),
            Expr::Field(_, r, i) => r.fields[*i].1.clone(),
            Expr::Ext { ty, .. } => ty.clone(),
        }
    }
}

impl Stmt {
    /// Every fire in it, nested blocks included.
    pub fn fires_mut(&mut self, f: &mut impl FnMut(&mut Fire)) {
        match self {
            Stmt::Fire(fire) => f(fire),
            Stmt::Ext { fires, .. } => fires.iter_mut().for_each(|(_, fire)| f(fire)),
            Stmt::If(_, a, b) => {
                a.iter_mut().for_each(|s| s.fires_mut(f));
                b.iter_mut().for_each(|s| s.fires_mut(f));
            }
            Stmt::While(_, b) | Stmt::For { body: b, .. } => b.iter_mut().for_each(|s| s.fires_mut(f)),
            _ => {}
        }
    }

    pub fn fires(&self, f: &mut impl FnMut(&Fire)) {
        match self {
            Stmt::Fire(fire) => f(fire),
            Stmt::Ext { fires, .. } => fires.iter().for_each(|(_, fire)| f(fire)),
            Stmt::If(_, a, b) => {
                a.iter().for_each(|s| s.fires(f));
                b.iter().for_each(|s| s.fires(f));
            }
            Stmt::While(_, b) | Stmt::For { body: b, .. } => b.iter().for_each(|s| s.fires(f)),
            _ => {}
        }
    }
}

impl Program {
    /// Whether any fire runs branches: then a compiled program shares its
    /// state between threads.
    pub fn has_branches(&self) -> bool {
        let mut any = self.starts.targets.len() > 1;
        for h in &self.handlers {
            for s in &h.body {
                s.fires(&mut |f| any |= f.targets.len() > 1);
            }
        }
        any
    }

    /// The handlers each handler fires, directly.
    pub fn calls(&self) -> Vec<Vec<HandlerId>> {
        self.handlers
            .iter()
            .map(|h| {
                let mut v = Vec::new();
                for s in &h.body {
                    s.fires(&mut |f| v.extend(f.targets.iter().chain(&f.meets)));
                }
                v.sort_unstable();
                v.dedup();
                v
            })
            .collect()
    }

    /// Works out every fire's meets (see [`Fire`]).
    pub fn find_meets(&mut self) {
        let calls = self.calls();
        // what FROM reaches, not going past the handlers in STOP
        let reach = |from: HandlerId, stop: &[bool]| -> Vec<bool> {
            let mut seen = vec![false; calls.len()];
            let mut stack = vec![from];
            while let Some(h) = stack.pop() {
                if !std::mem::replace(&mut seen[h as usize], true) && (h == from || !stop[h as usize]) {
                    stack.extend(&calls[h as usize]);
                }
            }
            seen
        };
        let meets = |f: &mut Fire| {
            f.meets.clear();
            if f.targets.len() < 2 {
                return;
            }
            let mut targets = f.targets.clone();
            targets.sort_unstable();
            targets.dedup();
            // a branch stops at the first meet it gets to, so the meets
            // behind other meets aren't: go again, stopping at them, until
            // nothing changes (only taking some out, so it ends)
            let mut stop = vec![false; calls.len()];
            let mut first = true;
            loop {
                let mut count = vec![0u32; calls.len()];
                for t in &targets {
                    for (h, r) in reach(*t, &stop).iter().enumerate() {
                        count[h] += *r as u32;
                    }
                }
                // a target fired twice by the same fire is reached twice
                for t in &targets {
                    if f.targets.iter().filter(|x| *x == t).count() > 1 {
                        count[*t as usize] = 2;
                    }
                }
                let now: Vec<bool> = count.iter().zip(&stop).map(|(c, s)| *c >= 2 && (first || *s)).collect();
                if now == stop && !first {
                    break;
                }
                first = false;
                stop = now;
            }
            f.meets = (0..calls.len() as u32).filter(|h| stop[*h as usize]).collect();
        };
        meets(&mut self.starts);
        for h in &mut self.handlers {
            for s in &mut h.body {
                s.fires_mut(&mut |f| meets(f));
            }
        }
    }
}

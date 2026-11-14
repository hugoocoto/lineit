//! A shader as the evaluator runs it: types, expressions and statements,
//! with every name already turned into a slot (see `parse.rs`).

use super::eval::Interp;
use super::val::{R, V};

/// What a number is made of.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sc {
    Bool,
    Int,
    UInt,
    Float,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Ty {
    Void,
    /// A scalar (1) or a vector (2 to 4) of a kind of number.
    Vec(Sc, u8),
    /// A square matrix of floats, 2 to 4 on a side.
    Mat(u8),
    Sampler,
    Array(Box<Ty>, usize),
    /// A struct, by its place in [`Program::structs`].
    Struct(usize),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Bin {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Shl,
    Shr,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    BitAnd,
    BitOr,
    BitXor,
    And,
    Or,
    Xor,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Un {
    Neg,
    Plus,
    Not,
    BitNot,
}

/// `.name`: a struct's field, or a swizzle (`xyz`, `rgba`, `st`...), with
/// the components it takes.
pub struct Field {
    pub name: String,
    pub swz: Option<([u8; 4], u8)>,
}

pub type BFn = fn(&mut Interp, &[V]) -> R<V>;

pub enum Callee {
    User(usize),
    Builtin(BFn),
    /// `vec3(...)`, `float[3](...)`, `MyStruct(...)`.
    Ctor(Ty),
}

pub enum E {
    Lit(V),
    Local(usize),
    Global(usize),
    Un(Un, Box<E>),
    Bin(Bin, Box<E>, Box<E>),
    Cond(Box<E>, Box<E>, Box<E>),
    Call(Callee, Vec<E>),
    Field(Box<E>, Field),
    Index(Box<E>, Box<E>),
    /// `a = b`, or (with an operator) `a += b`.
    Assign(Box<E>, Option<Bin>, Box<E>),
    /// `a++` (post: true) or `--a`: the amount added, and whether the
    /// value is the one before.
    IncDec(Box<E>, f64, bool),
    Comma(Vec<E>),
}

pub struct Case {
    /// None: `default`.
    pub value: Option<E>,
    pub body: Vec<S>,
}

pub enum S {
    Expr(E),
    /// A local: its slot, its type and what starts it.
    Decl(usize, Ty, Option<E>),
    Block(Vec<S>),
    If(E, Vec<S>, Vec<S>),
    For(Vec<S>, Option<E>, Option<E>, Vec<S>),
    While(E, Vec<S>),
    Do(Vec<S>, E),
    Switch(E, Vec<Case>),
    Break,
    Continue,
    Return(Option<E>),
    Discard,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dir {
    In,
    Out,
    InOut,
}

pub struct Param {
    pub slot: usize,
    pub ty: Ty,
    pub dir: Dir,
}

pub struct Func {
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Ty,
    pub body: Vec<S>,
    /// How many locals it has, parameters included.
    pub slots: usize,
}

pub struct StructDef {
    pub name: String,
    pub fields: Vec<(String, Ty)>,
}

pub struct UniformDecl {
    pub name: String,
    pub slot: usize,
    pub ty: Ty,
    pub init: Option<E>,
    /// `source_color`: a color, for the editor.
    pub color: bool,
    pub filter_nearest: bool,
    pub repeat: bool,
    pub default_white: bool,
}

pub struct Program {
    pub structs: Vec<StructDef>,
    pub funcs: Vec<Func>,
    pub fragment: Option<usize>,
    pub globals: usize,
    /// Consts and globals that start with a value, in order.
    pub init: Vec<(usize, Ty, Option<E>)>,
    /// The globals a shader can change (not the consts): each pixel starts
    /// them again.
    pub mutable: Vec<usize>,
    pub uniforms: Vec<UniformDecl>,
}

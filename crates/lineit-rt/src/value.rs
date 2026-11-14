//! Values with their type attached, for the interpreter, Lua and the editor.
//! Compiled programs don't use them: they have the types themselves.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::image::Image;
use crate::types::{RecordType, Type};

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(Arc<str>),
    List(Arc<Vec<Value>>),
    Map(Arc<BTreeMap<String, Value>>),
    Image(Image),
    Record(Arc<Record>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub ty: Arc<RecordType>,
    pub fields: Vec<Value>,
}

impl Value {
    /// What a slot of type TY holds before anything sets it.
    pub fn default_of(ty: &Type) -> Value {
        match ty {
            Type::Int => Value::Int(0),
            Type::Float => Value::Float(0.0),
            Type::Bool => Value::Bool(false),
            Type::Str => Value::Str("".into()),
            Type::List(_) => Value::List(Arc::new(Vec::new())),
            Type::Map(_) => Value::Map(Arc::new(BTreeMap::new())),
            Type::Image => Value::Image(Image::default()),
            Type::Record(r) => Value::Record(Arc::new(Record {
                ty: r.clone(),
                fields: r.fields.iter().map(|(_, t)| Value::default_of(t)).collect(),
            })),
        }
    }

    /// Reads TEXT, typed in a field, as a value of type TY. Only scalars
    /// can be typed. Leading and trailing spaces count for strings only.
    pub fn parse(text: &str, ty: &Type) -> Option<Value> {
        Some(match ty {
            Type::Int => Value::Int(parse_int(text.trim())?),
            Type::Float => Value::Float(parse_float(text.trim())?),
            Type::Bool => Value::Bool(parse_bool(text.trim())?),
            Type::Str => Value::Str(text.into()),
            _ => return None,
        })
    }

    /// The type a literal typed where any type goes has: `2` is an int,
    /// `2.5` a float, `true` a bool, anything else a string.
    pub fn literal_type(text: &str) -> Type {
        let t = text.trim();
        if t.is_empty() {
            Type::Str
        } else if parse_int(t).is_some() {
            Type::Int
        } else if parse_float(t).is_some() && t.chars().any(|c| c.is_ascii_digit()) {
            Type::Float
        } else if parse_bool(t).is_some() {
            Type::Bool
        } else {
            Type::Str
        }
    }

    pub fn ty(&self) -> Type {
        match self {
            Value::Int(_) => Type::Int,
            Value::Float(_) => Type::Float,
            Value::Bool(_) => Type::Bool,
            Value::Str(_) => Type::Str,
            Value::List(l) => Type::List(Box::new(l.first().map(Value::ty).unwrap_or(Type::Int))),
            Value::Map(m) => Type::Map(Box::new(m.values().next().map(Value::ty).unwrap_or(Type::Int))),
            Value::Image(_) => Type::Image,
            Value::Record(r) => Type::Record(r.ty.clone()),
        }
    }

    pub fn int(&self) -> i64 {
        match self {
            Value::Int(i) => *i,
            Value::Float(f) => *f as i64,
            Value::Bool(b) => *b as i64,
            _ => 0,
        }
    }

    pub fn float(&self) -> f64 {
        match self {
            Value::Float(f) => *f,
            Value::Int(i) => *i as f64,
            Value::Bool(b) => *b as i64 as f64,
            _ => 0.0,
        }
    }

    pub fn bool(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            _ => false,
        }
    }

    pub fn str(&self) -> &str {
        match self {
            Value::Str(s) => s,
            _ => "",
        }
    }

    pub fn list(&self) -> &[Value] {
        match self {
            Value::List(l) => l,
            _ => &[],
        }
    }

    pub fn image(&self) -> &Image {
        static EMPTY: std::sync::OnceLock<Image> = std::sync::OnceLock::new();
        match self {
            Value::Image(i) => i,
            _ => EMPTY.get_or_init(Image::default),
        }
    }

    /// The list in it, to change in place (copied first if shared).
    pub fn list_mut(&mut self) -> &mut Vec<Value> {
        if !matches!(self, Value::List(_)) {
            *self = Value::List(Arc::new(Vec::new()));
        }
        match self {
            Value::List(l) => Arc::make_mut(l),
            _ => unreachable!(),
        }
    }

    /// The map in it, to change in place (copied first if shared).
    pub fn map_mut(&mut self) -> &mut BTreeMap<String, Value> {
        if !matches!(self, Value::Map(_)) {
            *self = Value::Map(Arc::new(BTreeMap::new()));
        }
        match self {
            Value::Map(m) => Arc::make_mut(m),
            _ => unreachable!(),
        }
    }

    pub fn str_value(s: impl Into<Arc<str>>) -> Value {
        Value::Str(s.into())
    }

    /// As the program would print it.
    pub fn text(&self) -> String {
        crate::show::text(self)
    }
}

pub fn parse_int(t: &str) -> Option<i64> {
    let t = t.strip_prefix('+').unwrap_or(t);
    if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return i64::from_str_radix(h, 16).ok();
    }
    if let Some(h) = t.strip_prefix("-0x").or_else(|| t.strip_prefix("-0X")) {
        return i64::from_str_radix(h, 16).ok().map(|v| v.wrapping_neg());
    }
    t.parse().ok()
}

pub fn parse_float(t: &str) -> Option<f64> {
    let t = t.strip_prefix('+').unwrap_or(t);
    match t {
        "inf" => Some(f64::INFINITY),
        "-inf" => Some(f64::NEG_INFINITY),
        "nan" => Some(f64::NAN),
        _ if t.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | 'e' | 'E' | '+')) => t.parse().ok(),
        _ => None,
    }
}

pub fn parse_bool(t: &str) -> Option<bool> {
    match t {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// Plain Rust types to and from [`Value`]s, for what a compiled program
/// gives Lua and gets back.
pub trait Typed: Sized {
    fn to_value(&self) -> Value;
    fn from_value(v: Value) -> Self;
}

impl Typed for i64 {
    fn to_value(&self) -> Value {
        Value::Int(*self)
    }
    fn from_value(v: Value) -> Self {
        v.int()
    }
}

impl Typed for f64 {
    fn to_value(&self) -> Value {
        Value::Float(*self)
    }
    fn from_value(v: Value) -> Self {
        v.float()
    }
}

impl Typed for bool {
    fn to_value(&self) -> Value {
        Value::Bool(*self)
    }
    fn from_value(v: Value) -> Self {
        v.bool()
    }
}

impl Typed for String {
    fn to_value(&self) -> Value {
        Value::Str(self.as_str().into())
    }
    fn from_value(v: Value) -> Self {
        v.str().to_string()
    }
}

impl Typed for Image {
    fn to_value(&self) -> Value {
        Value::Image(self.clone())
    }
    fn from_value(v: Value) -> Self {
        v.image().clone()
    }
}

impl<T: Typed> Typed for Vec<T> {
    fn to_value(&self) -> Value {
        Value::List(Arc::new(self.iter().map(Typed::to_value).collect()))
    }
    fn from_value(v: Value) -> Self {
        v.list().iter().cloned().map(T::from_value).collect()
    }
}

impl<T: Typed> Typed for BTreeMap<String, T> {
    fn to_value(&self) -> Value {
        Value::Map(Arc::new(self.iter().map(|(k, v)| (k.clone(), v.to_value())).collect()))
    }
    fn from_value(v: Value) -> Self {
        match v {
            Value::Map(m) => m.iter().map(|(k, v)| (k.clone(), T::from_value(v.clone()))).collect(),
            _ => BTreeMap::new(),
        }
    }
}

/// Whether A and B are the same value (floats: nearly).
pub fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => (x - y).abs() <= 1e-9 * x.abs().max(y.abs()).max(1.0),
        (Value::Int(x), Value::Float(y)) | (Value::Float(y), Value::Int(x)) => {
            same(&Value::Float(*x as f64), &Value::Float(*y))
        }
        (Value::List(x), Value::List(y)) => x.len() == y.len() && x.iter().zip(y.iter()).all(|(a, b)| same(a, b)),
        (Value::Map(x), Value::Map(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|((ka, a), (kb, b))| ka == kb && same(a, b))
        }
        (Value::Record(x), Value::Record(y)) => {
            x.fields.len() == y.fields.len() && x.fields.iter().zip(&y.fields).all(|(a, b)| same(a, b))
        }
        _ => a == b,
    }
}

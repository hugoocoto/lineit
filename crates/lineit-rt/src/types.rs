//! The types a wire can carry.

use std::fmt;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Type {
    Int,
    Float,
    Bool,
    Str,
    /// A list of values of one type.
    List(Box<Type>),
    /// Strings to values of one type, in key order.
    Map(Box<Type>),
    Image,
    /// A record type, by name (see [`RecordType`]).
    Record(Arc<RecordType>),
}

/// A custom type: named fields, each of a type. Two records are the same
/// type if they have the same name.
#[derive(Clone, Debug)]
pub struct RecordType {
    pub name: String,
    pub fields: Vec<(String, Type)>,
}

impl PartialEq for RecordType {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}
impl Eq for RecordType {}
impl std::hash::Hash for RecordType {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        self.name.hash(h)
    }
}
impl PartialOrd for RecordType {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for RecordType {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.name.cmp(&other.name)
    }
}

impl Type {
    pub fn is_number(&self) -> bool {
        matches!(self, Type::Int | Type::Float)
    }

    /// Whether it fits in a field: typed as text, read back with
    /// [`crate::value::Value::parse`].
    pub fn is_scalar(&self) -> bool {
        matches!(self, Type::Int | Type::Float | Type::Bool | Type::Str)
    }

    /// Whether two values of it can be ordered (`compare/less`...).
    pub fn is_ordered(&self) -> bool {
        matches!(self, Type::Int | Type::Float | Type::Str | Type::Bool)
    }

    /// Reads a type written as [`Type`]'s `Display` writes it: `int`,
    /// `list<str>`, `map<list<int>>`... Records are looked up with RECORD.
    pub fn parse(s: &str, record: &dyn Fn(&str) -> Option<Arc<RecordType>>) -> Option<Type> {
        let s = s.trim();
        let inner =
            |prefix: &str| s.strip_prefix(prefix).and_then(|r| r.strip_prefix('<')).and_then(|r| r.strip_suffix('>'));
        Some(match s {
            "int" => Type::Int,
            "float" => Type::Float,
            "bool" => Type::Bool,
            "str" => Type::Str,
            "image" => Type::Image,
            _ => {
                if let Some(i) = inner("list") {
                    Type::List(Box::new(Type::parse(i, record)?))
                } else if let Some(i) = inner("map") {
                    Type::Map(Box::new(Type::parse(i, record)?))
                } else {
                    Type::Record(record(s)?)
                }
            }
        })
    }

    /// The Rust type a compiled program uses for it.
    pub fn rust(&self) -> String {
        match self {
            Type::Int => "i64".into(),
            Type::Float => "f64".into(),
            Type::Bool => "bool".into(),
            Type::Str => "String".into(),
            Type::List(t) => format!("Vec<{}>", t.rust()),
            Type::Map(t) => format!("std::collections::BTreeMap<String, {}>", t.rust()),
            Type::Image => "lineit_rt::Image".into(),
            Type::Record(r) => format!("R_{}", r.name),
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Int => f.write_str("int"),
            Type::Float => f.write_str("float"),
            Type::Bool => f.write_str("bool"),
            Type::Str => f.write_str("str"),
            Type::List(t) => write!(f, "list<{t}>"),
            Type::Map(t) => write!(f, "map<{t}>"),
            Type::Image => f.write_str("image"),
            Type::Record(r) => f.write_str(&r.name),
        }
    }
}

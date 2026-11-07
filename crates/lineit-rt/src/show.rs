//! How values print. The interpreter prints [`Value`]s and compiled
//! programs print plain Rust types, through the same [`Show`], so a canvas
//! prints the same either way.

use std::collections::BTreeMap;

use crate::image::Image;
use crate::value::{Record, Value};

pub trait Show {
    /// As `io/print` prints it.
    fn show(&self, out: &mut String);

    /// Inside a list, a map or a record: strings get quotes.
    fn show_nested(&self, out: &mut String) {
        self.show(out)
    }
}

pub fn text<T: Show + ?Sized>(v: &T) -> String {
    let mut s = String::new();
    v.show(&mut s);
    s
}

impl Show for i64 {
    fn show(&self, out: &mut String) {
        use std::fmt::Write;
        let _ = write!(out, "{self}");
    }
}

/// Floats always look like floats: `2.0`, not `2`.
impl Show for f64 {
    fn show(&self, out: &mut String) {
        use std::fmt::Write;
        if self.is_nan() {
            out.push_str("nan");
        } else if self.is_infinite() {
            out.push_str(if *self > 0.0 { "inf" } else { "-inf" });
        } else if self.fract() == 0.0 && self.abs() < 1e16 {
            let _ = write!(out, "{self:.1}");
        } else {
            let _ = write!(out, "{self}");
        }
    }
}

impl Show for bool {
    fn show(&self, out: &mut String) {
        out.push_str(if *self { "true" } else { "false" });
    }
}

impl Show for str {
    fn show(&self, out: &mut String) {
        out.push_str(self);
    }
    fn show_nested(&self, out: &mut String) {
        use std::fmt::Write;
        let _ = write!(out, "{self:?}");
    }
}

impl Show for String {
    fn show(&self, out: &mut String) {
        self.as_str().show(out)
    }
    fn show_nested(&self, out: &mut String) {
        self.as_str().show_nested(out)
    }
}

impl<T: Show> Show for [T] {
    fn show(&self, out: &mut String) {
        out.push('[');
        for (i, v) in self.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            v.show_nested(out);
        }
        out.push(']');
    }
}

impl<T: Show> Show for Vec<T> {
    fn show(&self, out: &mut String) {
        self.as_slice().show(out)
    }
}

impl<T: Show> Show for BTreeMap<String, T> {
    fn show(&self, out: &mut String) {
        out.push('{');
        for (i, (k, v)) in self.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            k.show_nested(out);
            out.push_str(": ");
            v.show_nested(out);
        }
        out.push('}');
    }
}

impl Show for Image {
    fn show(&self, out: &mut String) {
        use std::fmt::Write;
        let _ = write!(out, "image({}x{})", self.width, self.height);
    }
}

/// How a record prints: `point(x: 1.0, y: 2.0)`. Compiled records call it
/// with their name and fields.
pub fn show_record(out: &mut String, name: &str, fields: &[(&str, &dyn Show)]) {
    out.push_str(name);
    out.push('(');
    for (i, (n, v)) in fields.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(n);
        out.push_str(": ");
        v.show_nested(out);
    }
    out.push(')');
}

impl Show for Record {
    fn show(&self, out: &mut String) {
        let fields: Vec<(&str, &dyn Show)> =
            self.ty.fields.iter().zip(&self.fields).map(|((n, _), v)| (n.as_str(), v as &dyn Show)).collect();
        show_record(out, &self.ty.name, &fields);
    }
}

impl Show for Value {
    fn show(&self, out: &mut String) {
        match self {
            Value::Int(v) => v.show(out),
            Value::Float(v) => v.show(out),
            Value::Bool(v) => v.show(out),
            Value::Str(v) => v.show(out),
            Value::List(v) => v.as_slice().show(out),
            Value::Map(v) => v.as_ref().show(out),
            Value::Image(v) => v.show(out),
            Value::Record(v) => v.show(out),
        }
    }
    fn show_nested(&self, out: &mut String) {
        match self {
            Value::Str(v) => v.show_nested(out),
            _ => self.show(out),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_look_like_floats() {
        assert_eq!(text(&2.0f64), "2.0");
        assert_eq!(text(&2.5f64), "2.5");
        assert_eq!(text(&-0.125f64), "-0.125");
        assert_eq!(text(&f64::INFINITY), "inf");
        assert_eq!(text(&1e20f64), "100000000000000000000");
    }

    #[test]
    fn nested() {
        let v = vec!["a".to_string(), "b\"".to_string()];
        assert_eq!(text(&v), r#"["a", "b\""]"#);
        let mut m = BTreeMap::new();
        m.insert("k".to_string(), vec![1i64, 2]);
        assert_eq!(text(&m), r#"{"k": [1, 2]}"#);
    }
}

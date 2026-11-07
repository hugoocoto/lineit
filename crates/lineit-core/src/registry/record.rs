//! Record types, from `*.type.lua` files:
//!
//! ```lua
//! return {
//!         doc = "A point on the plane",
//!         fields = { { "x", "float" }, { "y", "float" } },
//! }
//! ```
//!
//! The file's name is the type's. Each type gets two squares:
//! `record/make/NAME`, from its fields to a record, and `record/split/NAME`,
//! the other way.

use std::sync::Arc;

use lineit_rt::{RecordType, Type};

use super::builtin::default_text;
use super::{Kind, Module, ParamDef, Params, SigCx, Signature, TExpr};
use crate::ir::Expr;
use crate::lower::Lx;

pub fn read(source: &str, name: &str, record: &dyn Fn(&str) -> Option<Arc<RecordType>>) -> Result<RecordType, String> {
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') || name.is_empty() {
        return Err(format!("`{name}`: a type's name is letters, digits and _"));
    }
    crate::file::with_data(source, name, |t| {
        let fields: mlua::Table = t.get("fields").map_err(|_| "it needs `fields = { { \"name\", \"type\" }, ... }`")?;
        let mut out = Vec::new();
        for f in fields.sequence_values::<mlua::Table>() {
            let f = f.map_err(|_| "each field is { \"name\", \"type\" }")?;
            let (n, ty): (String, String) =
                (f.get(1).map_err(|e| e.to_string())?, f.get(2).map_err(|e| e.to_string())?);
            let ty = Type::parse(&ty, record).ok_or_else(|| format!("field `{n}`: `{ty}` isn't a type"))?;
            if out.iter().any(|(m, _): &(String, Type)| *m == n) {
                return Err(format!("two fields are called `{n}`"));
            }
            out.push((n, ty));
        }
        Ok(RecordType { name: name.to_string(), fields: out })
    })
}

struct Make(Arc<RecordType>, String);
struct Split(Arc<RecordType>, String);

pub fn modules(r: &Arc<RecordType>) -> Vec<Arc<dyn Module>> {
    vec![
        Arc::new(Make(r.clone(), format!("record/make/{}", r.name))),
        Arc::new(Split(r.clone(), format!("record/split/{}", r.name))),
    ]
}

impl Module for Make {
    fn path(&self) -> &str {
        &self.1
    }
    fn doc(&self) -> &str {
        "A record, from its fields"
    }
    fn kind(&self) -> Kind {
        Kind::Record
    }
    fn params(&self) -> &[ParamDef] {
        &[]
    }
    fn signature(&self, _: &Params, _: &dyn SigCx) -> Result<Signature, String> {
        let mut s = Signature::new();
        for (n, t) in &self.0.fields {
            s = if t.is_scalar() {
                s.input(n, TExpr::T(t.clone()), default_text(t))
            } else {
                s.wire_in(n, TExpr::T(t.clone()))
            };
        }
        Ok(s.output(&self.0.name, TExpr::T(Type::Record(self.0.clone()))))
    }
    fn eval(&self, cx: &mut Lx, _: usize) -> Result<Expr, String> {
        let fields = (0..self.0.fields.len()).map(|i| cx.input(i)).collect::<Result<_, _>>()?;
        Ok(Expr::Record(self.0.clone(), fields))
    }
}

impl Module for Split {
    fn path(&self) -> &str {
        &self.1
    }
    fn doc(&self) -> &str {
        "The fields of a record"
    }
    fn kind(&self) -> Kind {
        Kind::Record
    }
    fn params(&self) -> &[ParamDef] {
        &[]
    }
    fn signature(&self, _: &Params, _: &dyn SigCx) -> Result<Signature, String> {
        let mut s = Signature::new().wire_in(&self.0.name, TExpr::T(Type::Record(self.0.clone())));
        for (n, t) in &self.0.fields {
            s = s.output(n, TExpr::T(t.clone()));
        }
        Ok(s)
    }
    fn eval(&self, cx: &mut Lx, output: usize) -> Result<Expr, String> {
        Ok(Expr::Field(Box::new(cx.input(0)?), self.0.clone(), output))
    }
}

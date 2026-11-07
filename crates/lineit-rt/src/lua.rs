//! Lua modules: a `.lua` file that returns a table saying its pins and what
//! it does.
//!
//! ```lua
//! return {
//!     doc = "Reverses a string",
//!     inputs = { { "text", "str", default = "" } },
//!     outputs = { { "result", "str" } },
//!     eval = function(i) return { result = i.text:reverse() } end,
//! }
//! ```
//!
//! A module with tick pins has `tick(self, input, i)` instead: INPUT is the
//! name of the tick input that fired, I has the values of the data inputs,
//! and it returns a table of outputs to set and, after it, a list of the
//! tick outputs to fire, in order. SELF is the square's own table, kept from
//! one tick to the next, with its params in `self.params`. A module can
//! also have `start(self, i)`, called when the program starts, which
//! returns the same (so a module with only tick outputs can start things).
//! `eval(i, self)` gets the square's table too.
//!
//! The interpreter and compiled programs both run them through this, with
//! all of Lua's libraries: a module is code, only use the ones you trust.

use std::collections::BTreeMap;
use std::sync::Arc;

use mlua::{Function, Lua, RegistryKey, Table};

use crate::types::Type;
use crate::value::{Record, Value};

pub struct LuaModule {
    lua: Lua,
    module: Table,
    instances: Vec<RegistryKey>,
}

/// A pin or param as the module declares it.
#[derive(Clone, Debug)]
pub struct PinSpec {
    pub name: String,
    /// `tick`, or a type as [`Type::parse`] reads it.
    pub ty: String,
    /// What its field has when the square is made, if it has one.
    pub default: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Spec {
    pub doc: String,
    pub inputs: Vec<PinSpec>,
    pub outputs: Vec<PinSpec>,
    pub params: Vec<PinSpec>,
    pub has_eval: bool,
    pub has_tick: bool,
    pub has_start: bool,
}

fn err(e: mlua::Error) -> String {
    e.to_string()
}

impl LuaModule {
    /// Runs SOURCE, the module's file, which must return its table.
    pub fn load(name: &str, source: &str) -> Result<LuaModule, String> {
        let lua = Lua::new();
        let module: Table = lua.load(source).set_name(format!("@{name}")).eval().map_err(|e| format!("{name}: {e}"))?;
        Ok(LuaModule { lua, module, instances: Vec::new() })
    }

    pub fn spec(&self) -> Result<Spec, String> {
        let pins = |key: &str| -> Result<Vec<PinSpec>, String> {
            let Ok(t) = self.module.get::<Table>(key) else {
                return Ok(Vec::new());
            };
            let mut v = Vec::new();
            for pin in t.sequence_values::<Table>() {
                let pin = pin.map_err(|_| format!("each of `{key}` is a table: {{ \"name\", \"type\" }}"))?;
                let name: String = pin.get(1).map_err(|_| format!("a pin in `{key}` has no name"))?;
                let ty: String = pin.get(2).map_err(|_| format!("`{name}` in `{key}` has no type"))?;
                let default = match pin.get::<mlua::Value>("default").map_err(err)? {
                    mlua::Value::Nil => None,
                    mlua::Value::String(s) => Some(s.to_string_lossy()),
                    v => Some(v.to_string().map_err(err)?),
                };
                v.push(PinSpec { name, ty, default });
            }
            Ok(v)
        };
        // a function, or (for a Rust module's pins) `true`
        let has = |key: &str| self.module.get::<Function>(key).is_ok() || self.module.get::<bool>(key).unwrap_or(false);
        Ok(Spec {
            doc: self.module.get::<String>("doc").unwrap_or_default(),
            inputs: pins("inputs")?,
            outputs: pins("outputs")?,
            params: pins("params")?,
            has_eval: has("eval"),
            has_tick: has("tick"),
            has_start: has("start"),
        })
    }

    /// Makes the table of a square, with its PARAMS. Returns its number, for
    /// [`LuaModule::tick`] and the rest.
    pub fn instance(&mut self, params: &[(&str, Value)]) -> Result<usize, String> {
        let t = self.lua.create_table().map_err(err)?;
        let p = self.lua.create_table().map_err(err)?;
        for (k, v) in params {
            p.set(*k, to_lua(&self.lua, v)?).map_err(err)?;
        }
        t.set("params", p).map_err(err)?;
        self.instances.push(self.lua.create_registry_value(t).map_err(err)?);
        Ok(self.instances.len() - 1)
    }

    fn inputs(&self, inputs: &[(&str, Value)]) -> Result<Table, String> {
        let t = self.lua.create_table().map_err(err)?;
        for (k, v) in inputs {
            t.set(*k, to_lua(&self.lua, v)?).map_err(err)?;
        }
        Ok(t)
    }

    fn outputs(&self, got: Option<Table>, outputs: &[(&str, &Type)]) -> Result<Vec<Option<Value>>, String> {
        let Some(got) = got else {
            return Ok(outputs.iter().map(|_| None).collect());
        };
        outputs
            .iter()
            .map(|(name, ty)| match got.get::<mlua::Value>(*name).map_err(err)? {
                mlua::Value::Nil => Ok(None),
                v => from_lua(v, ty).map(Some).map_err(|e| format!("output `{name}`: {e}")),
            })
            .collect()
    }

    fn instance_table(&self, inst: usize) -> Result<Table, String> {
        self.lua.registry_value(&self.instances[inst]).map_err(err)
    }

    /// Calls `eval(i)` and returns OUTPUTS, by name and type, from what it
    /// returns (the default of each type for those it leaves out).
    pub fn eval(&self, inst: usize, inputs: &[(&str, Value)], outputs: &[(&str, &Type)]) -> Result<Vec<Value>, String> {
        let f: Function = self.module.get("eval").map_err(err)?;
        let got: Option<Table> = f.call((self.inputs(inputs)?, self.instance_table(inst)?)).map_err(err)?;
        Ok(self
            .outputs(got, outputs)?
            .into_iter()
            .zip(outputs)
            .map(|(v, (_, t))| v.unwrap_or_else(|| Value::default_of(t)))
            .collect())
    }

    /// Calls `tick(self, input, i)`, or `start(self, i)` with no INPUT.
    /// Returns the outputs it set (None for those it didn't) and the tick
    /// outputs to fire, by name.
    pub fn tick(
        &self,
        inst: usize,
        input: Option<&str>,
        inputs: &[(&str, Value)],
        outputs: &[(&str, &Type)],
    ) -> Result<(Vec<Option<Value>>, Vec<String>), String> {
        let (got, fire): (Option<Table>, Option<Table>) = match input {
            Some(input) => {
                let f: Function = self.module.get("tick").map_err(err)?;
                f.call((self.instance_table(inst)?, input, self.inputs(inputs)?)).map_err(err)?
            }
            None => match self.module.get::<Function>("start") {
                Ok(f) => f.call((self.instance_table(inst)?, self.inputs(inputs)?)).map_err(err)?,
                Err(_) => (None, None),
            },
        };
        let fire = match fire {
            Some(t) => t.sequence_values::<String>().collect::<Result<_, _>>().map_err(err)?,
            None => Vec::new(),
        };
        Ok((self.outputs(got, outputs)?, fire))
    }
}

pub fn to_lua(lua: &Lua, v: &Value) -> Result<mlua::Value, String> {
    Ok(match v {
        Value::Int(i) => mlua::Value::Integer(*i),
        Value::Float(f) => mlua::Value::Number(*f),
        Value::Bool(b) => mlua::Value::Boolean(*b),
        Value::Str(s) => mlua::Value::String(lua.create_string(s.as_bytes()).map_err(err)?),
        Value::List(l) => {
            let t = lua.create_table_with_capacity(l.len(), 0).map_err(err)?;
            for (i, v) in l.iter().enumerate() {
                t.set(i + 1, to_lua(lua, v)?).map_err(err)?;
            }
            mlua::Value::Table(t)
        }
        Value::Map(m) => {
            let t = lua.create_table_with_capacity(0, m.len()).map_err(err)?;
            for (k, v) in m.iter() {
                t.set(k.as_str(), to_lua(lua, v)?).map_err(err)?;
            }
            mlua::Value::Table(t)
        }
        Value::Record(r) => {
            let t = lua.create_table().map_err(err)?;
            for ((name, _), v) in r.ty.fields.iter().zip(&r.fields) {
                t.set(name.as_str(), to_lua(lua, v)?).map_err(err)?;
            }
            mlua::Value::Table(t)
        }
        Value::Image(_) => return Err("images can't go to Lua".into()),
    })
}

pub fn from_lua(v: mlua::Value, ty: &Type) -> Result<Value, String> {
    let bad = |v: &mlua::Value| format!("expected {ty}, got a {}", v.type_name());
    Ok(match (ty, &v) {
        (Type::Int, mlua::Value::Integer(i)) => Value::Int(*i),
        (Type::Int, mlua::Value::Number(f)) => Value::Int(*f as i64),
        (Type::Float, mlua::Value::Number(f)) => Value::Float(*f),
        (Type::Float, mlua::Value::Integer(i)) => Value::Float(*i as f64),
        (Type::Bool, mlua::Value::Boolean(b)) => Value::Bool(*b),
        (Type::Str, mlua::Value::String(s)) => Value::Str(s.to_string_lossy().into()),
        (Type::Str, mlua::Value::Integer(_) | mlua::Value::Number(_)) => Value::Str(v.to_string().map_err(err)?.into()),
        (Type::List(t), mlua::Value::Table(tab)) => {
            let mut l = Vec::new();
            for v in tab.clone().sequence_values::<mlua::Value>() {
                l.push(from_lua(v.map_err(err)?, t)?);
            }
            Value::List(Arc::new(l))
        }
        (Type::Map(t), mlua::Value::Table(tab)) => {
            let mut m = BTreeMap::new();
            for pair in tab.clone().pairs::<String, mlua::Value>() {
                let (k, v) = pair.map_err(err)?;
                m.insert(k, from_lua(v, t)?);
            }
            Value::Map(Arc::new(m))
        }
        (Type::Record(r), mlua::Value::Table(tab)) => {
            let mut fields = Vec::new();
            for (name, t) in &r.fields {
                fields.push(match tab.get::<mlua::Value>(name.as_str()).map_err(err)? {
                    mlua::Value::Nil => Value::default_of(t),
                    v => from_lua(v, t)?,
                });
            }
            Value::Record(Arc::new(Record { ty: r.clone(), fields }))
        }
        _ => return Err(bad(&v)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eval_and_tick() {
        let mut m = LuaModule::load(
            "t",
            r#"return {
                inputs = { { "tick", "tick" }, { "n", "int", default = 2 } },
                outputs = { { "then", "tick" }, { "total", "int" }, { "parts", "list<str>" } },
                tick = function(self, input, i)
                    self.total = (self.total or 0) + i.n
                    return { total = self.total, parts = { "a", "b" } }, { "then" }
                end,
            }"#,
        )
        .unwrap();
        let spec = m.spec().unwrap();
        assert_eq!(spec.inputs[1].default.as_deref(), Some("2"));
        assert!(spec.has_tick && !spec.has_eval);
        let i = m.instance(&[]).unwrap();
        let list = Type::List(Box::new(Type::Str));
        let outs = [("total", &Type::Int), ("parts", &list)];
        m.tick(i, Some("tick"), &[("n", Value::Int(3))], &outs).unwrap();
        let (got, fire) = m.tick(i, Some("tick"), &[("n", Value::Int(4))], &outs).unwrap();
        assert_eq!(got[0], Some(Value::Int(7)));
        assert_eq!(got[1].as_ref().unwrap().text(), r#"["a", "b"]"#);
        assert_eq!(fire, ["then"]);
    }
}

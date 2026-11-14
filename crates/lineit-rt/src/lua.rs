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
use std::sync::atomic::{AtomicBool, Ordering};

use mlua::{Function, Lua, RegistryKey, Table};

use crate::types::Type;
use crate::value::{Record, Value};

pub struct LuaModule {
    lua: Lua,
    module: Table,
    instances: Vec<RegistryKey>,
    stop: Arc<AtomicBool>,
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
        let (lua, stop) = new_lua()?;
        let module: Table = lua.load(source).set_name(format!("@{name}")).eval().map_err(|e| format!("{name}: {e}"))?;
        Ok(LuaModule { lua, module, instances: Vec::new(), stop })
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

/// A code square's function in Lua (see `code/function` in lineit-core),
/// through the glue lineit writes around it: a table with `new()`, which
/// runs the code in a fresh environment for each square (its globals are
/// the square's state), `call(sq, i)`, which returns the outputs it set and
/// the outputs it fired, each with its values (`{ name, { pin = value } }`),
/// and `state(sq)` and `set_state(sq, t)`.
pub struct LuaCode {
    lua: Lua,
    glue: Table,
    squares: Vec<RegistryKey>,
    stop: Arc<AtomicBool>,
}

impl LuaCode {
    pub fn load(name: &str, glue: &str) -> Result<LuaCode, String> {
        let (lua, stop) = new_lua()?;
        let glue: Table = lua.load(glue).set_name(format!("@{name}")).eval().map_err(|e| format!("{name}: {e}"))?;
        Ok(LuaCode { lua, glue, squares: Vec::new(), stop })
    }

    fn square(&self, sq: usize) -> Result<mlua::Value, String> {
        self.lua.registry_value(&self.squares[sq]).map_err(err)
    }

    fn call(&self, sq: usize, inputs: &[(&str, Value)]) -> Result<(Table, Table), String> {
        let i = self.lua.create_table().map_err(err)?;
        for (k, v) in inputs {
            i.set(*k, to_lua(&self.lua, v)?).map_err(err)?;
        }
        let f: Function = self.glue.get("call").map_err(err)?;
        f.call((self.square(sq)?, i)).map_err(|e| without_traceback(&e))
    }
}

/// What went wrong, without Lua's traceback (which goes through lineit's
/// glue, not only the code).
fn without_traceback(e: &mlua::Error) -> String {
    let s = e.to_string();
    s.split("\nstack traceback:").next().unwrap_or(&s).trim_end().to_string()
}

/// OUTPUTS' values in T, by name: None for those it doesn't have.
fn by_name(t: &Table, outputs: &[(&str, &Type)]) -> Result<Vec<Option<Value>>, String> {
    outputs
        .iter()
        .map(|(name, ty)| match t.get::<mlua::Value>(*name).map_err(err)? {
            mlua::Value::Nil => Ok(None),
            v => from_lua(v, ty).map(Some).map_err(|e| format!("`{name}`: {e}")),
        })
        .collect()
}

impl crate::ext::Ext for LuaCode {
    fn handles(&mut self) -> crate::ext::Handles {
        lua_handles(&self.stop)
    }

    fn instance(&mut self, _params: &[(&str, Value)]) -> Result<usize, String> {
        let f: Function = self.glue.get("new").map_err(err)?;
        let sq: mlua::Value = f.call(()).map_err(err)?;
        self.squares.push(self.lua.create_registry_value(sq).map_err(err)?);
        Ok(self.squares.len() - 1)
    }

    fn eval(&mut self, sq: usize, inputs: &[(&str, Value)], outputs: &[(&str, &Type)]) -> Result<Vec<Value>, String> {
        let (set, _) = self.call(sq, inputs)?;
        Ok(by_name(&set, outputs)?.into_iter().zip(outputs).map(|(v, (_, t))| v.unwrap_or_else(|| Value::default_of(t))).collect())
    }

    fn tick(
        &mut self,
        sq: usize,
        input: Option<&str>,
        inputs: &[(&str, Value)],
        outputs: &[(&str, &Type)],
    ) -> Result<crate::ext::Ticked, String> {
        let run = self.run(sq, input, inputs, outputs)?;
        let mut set = run.set;
        let mut fire = Vec::new();
        for (name, values) in run.fire {
            for (o, v) in values {
                set[o] = Some(v);
            }
            fire.push(name);
        }
        Ok((set, fire))
    }

    fn run(
        &mut self,
        sq: usize,
        _input: Option<&str>,
        inputs: &[(&str, Value)],
        outputs: &[(&str, &Type)],
    ) -> Result<crate::ext::Run, String> {
        let (set, fired) = self.call(sq, inputs)?;
        let mut fire = Vec::new();
        for f in fired.sequence_values::<Table>() {
            let f = f.map_err(err)?;
            let name: String = f.get(1).map_err(err)?;
            let values: Table = f.get(2).map_err(err)?;
            let got = by_name(&values, outputs)?;
            fire.push((name, got.into_iter().enumerate().filter_map(|(o, v)| Some((o, v?))).collect()));
        }
        Ok(crate::ext::Run { set: by_name(&set, outputs)?, fire })
    }

    fn state(&mut self, sq: usize) -> Result<Vec<(String, Value)>, String> {
        let f: Function = self.glue.get("state").map_err(err)?;
        let t: Table = f.call(self.square(sq)?).map_err(err)?;
        let mut v = Vec::new();
        for pair in t.pairs::<String, mlua::Value>() {
            let (k, x) = pair.map_err(err)?;
            if let Some(x) = from_lua_any(x) {
                v.push((k, x));
            }
        }
        v.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(v)
    }

    fn set_state(&mut self, sq: usize, state: &[(&str, Value)]) -> Result<(), String> {
        let t = self.lua.create_table().map_err(err)?;
        for (k, v) in state {
            t.set(*k, to_lua(&self.lua, v)?).map_err(err)?;
        }
        let f: Function = self.glue.get("set_state").map_err(err)?;
        f.call((self.square(sq)?, t)).map_err(err)
    }
}

/// V as a lineit value of whatever type it looks like, to show it (None
/// for what has no value: functions...). A table with items 1, 2... is a
/// list, another a map.
pub fn from_lua_any(v: mlua::Value) -> Option<Value> {
    Some(match v {
        mlua::Value::Integer(i) => Value::Int(i),
        mlua::Value::Number(f) => Value::Float(f),
        mlua::Value::Boolean(b) => Value::Bool(b),
        mlua::Value::String(s) => Value::Str(s.to_string_lossy().into()),
        mlua::Value::Table(t) => {
            if t.raw_len() > 0 {
                Value::List(Arc::new(t.sequence_values::<mlua::Value>().filter_map(|x| from_lua_any(x.ok()?)).collect()))
            } else {
                let mut m = BTreeMap::new();
                for (k, x) in t.pairs::<String, mlua::Value>().flatten() {
                    if let Some(x) = from_lua_any(x) {
                        m.insert(k, x);
                    }
                }
                Value::Map(Arc::new(m))
            }
        }
        _ => return None,
    })
}

/// A Lua whose `print` prints where the program's lines go
/// ([`crate::ext::print`]), and the flag that stops what it runs (the
/// program's Stop: Lua checks it every so often, and fails if it's set).
fn new_lua() -> Result<(Lua, Arc<AtomicBool>), String> {
    let lua = Lua::new();
    let stop = Arc::new(AtomicBool::new(false));
    lua.set_hook(mlua::HookTriggers::new().every_nth_instruction(10_000), stop_hook(stop.clone(), false))
        .map_err(err)?;
    let print = lua
        .create_function(|_, args: mlua::MultiValue| {
            let parts: Vec<String> =
                args.iter().map(|v| v.to_string().unwrap_or_else(|_| v.type_name().to_string())).collect();
            crate::ext::print(&parts.join("\t"));
            Ok(())
        })
        .map_err(err)?;
    lua.globals().set("print", print).map_err(err)?;
    Ok((lua, stop))
}

/// The hook that stops what a Lua runs once FLAG is set: then it checks
/// at EVERY instruction, so code that catches the error (`pcall` in a
/// loop) fails again at the next one outside of it.
fn stop_hook(flag: Arc<AtomicBool>, every: bool) -> impl Fn(&Lua, &mlua::debug::Debug) -> mlua::Result<mlua::VmState> + Send + 'static {
    move |lua, _| {
        if !flag.load(Ordering::Relaxed) {
            return Ok(mlua::VmState::Continue);
        }
        if !every {
            let _ = lua.set_hook(mlua::HookTriggers::new().every_nth_instruction(1), stop_hook(flag.clone(), true));
        }
        Err(mlua::Error::runtime("stopped"))
    }
}

/// What stops what a Lua runs ([`crate::ext::Handles`]).
fn lua_handles(stop: &Arc<AtomicBool>) -> crate::ext::Handles {
    let stop = stop.clone();
    crate::ext::Handles { kill: Some(Box::new(move || stop.store(true, Ordering::Relaxed))), stderr: None }
}

impl LuaModule {
    pub fn handles(&self) -> crate::ext::Handles {
        lua_handles(&self.stop)
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

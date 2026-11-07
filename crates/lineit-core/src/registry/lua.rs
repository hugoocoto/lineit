//! Lua modules and Rust modules as squares. A Lua module's file says its
//! pins (see [`lineit_rt::lua`]); a Rust module's are in a
//! `NAME.native.lua` file of its package, the same table with
//! `native = "Type"` (and `eval = true`, `start = true` for what it has).
//! Lowering makes an instance for each square, and IR that calls its
//! `eval`, `tick` or `start`.

use lineit_rt::lua::{LuaModule, PinSpec, Spec};
use lineit_rt::{Type, Value};

use super::builtin::default_text;
use super::{Kind, Module, ParamDef, ParamKind, Params, PinTy, SigCx, Signature, TExpr};
use crate::ir::*;
use crate::lower::Lx;
use std::path::Path;

pub struct ExtModuleDef {
    path: String,
    kind: ExtKind,
    spec: Spec,
    params: Vec<ParamDef>,
}

impl ExtModuleDef {
    /// A Lua module, from its file.
    pub fn lua(path: &str, source: &str) -> Result<ExtModuleDef, String> {
        let spec = LuaModule::load(path, source)?.spec()?;
        if !spec.has_eval && !spec.has_tick && !spec.has_start {
            return Err("it has no `eval`, `tick` or `start` function".into());
        }
        Ok(Self::with(path, ExtKind::Lua { source: source.into() }, spec))
    }

    /// A Rust module of the crate in CRATE_DIR, from its `.native.lua`.
    pub fn native(path: &str, spec_source: &str, crate_dir: &Path) -> Result<ExtModuleDef, String> {
        // only a table: read in the sandbox, as a canvas is
        let ty = crate::file::with_data(spec_source, path, |t| {
            t.get::<String>("native")
                .map_err(|_| "it needs `native = \"Type\"`, the Rust type of the module".to_string())
        })?;
        let crate_name = crate::package::crate_name(crate_dir)?;
        let spec = LuaModule::load(path, spec_source)?.spec()?;
        Ok(Self::with(path, ExtKind::Native { crate_dir: crate_dir.to_path_buf(), crate_name, ty }, spec))
    }

    fn with(path: &str, kind: ExtKind, spec: Spec) -> ExtModuleDef {
        let params = spec
            .params
            .iter()
            .map(|p| {
                let kind = match p.ty.as_str() {
                    "int" => ParamKind::Int,
                    "float" => ParamKind::Float,
                    "bool" => ParamKind::Bool,
                    _ => ParamKind::Text,
                };
                ParamDef::new(&p.name, kind, p.default.as_deref().unwrap_or(""))
            })
            .collect();
        ExtModuleDef { path: path.into(), kind, spec, params }
    }

    fn instance(&self, cx: &mut Lx) -> Result<usize, String> {
        let p = cx.params();
        let mut params = Vec::new();
        for spec in &self.spec.params {
            let ty = match spec.ty.as_str() {
                "int" => Type::Int,
                "float" => Type::Float,
                "bool" => Type::Bool,
                _ => Type::Str,
            };
            let text = p.str(&spec.name);
            let v = Value::parse(&text, &ty).ok_or_else(|| format!("param `{}`: `{text}` isn't a {ty}", spec.name))?;
            params.push((spec.name.clone(), v));
        }
        Ok(cx.ext_instance(&self.path, &self.kind, params))
    }

    fn data_inputs(&self, cx: &mut Lx) -> Result<Vec<(String, Expr)>, String> {
        let mut v = Vec::new();
        for (i, pin) in cx.sig().inputs.iter().enumerate() {
            if pin.ty != PinTy::Tick {
                v.push((pin.name.clone(), cx.input(i)?));
            }
        }
        Ok(v)
    }

    fn call(&self, cx: &mut Lx, input: Option<String>) -> Result<Block, String> {
        let instance = self.instance(cx)?;
        let inputs = self.data_inputs(cx)?;
        let mut outputs = Vec::new();
        let mut fires = Vec::new();
        for (o, pin) in cx.sig().outputs.iter().enumerate() {
            if pin.ty == PinTy::Tick {
                fires.push((pin.name.clone(), cx.fire_of(o)));
            } else {
                outputs.push((pin.name.clone(), cx.out_slot(o)));
            }
        }
        Ok(vec![Stmt::Ext { instance, input, inputs, outputs, fires }])
    }
}

fn pin_type(p: &PinSpec, cx: &dyn SigCx) -> Result<Option<Type>, String> {
    if p.ty == "tick" {
        return Ok(None);
    }
    cx.parse_type(&p.ty).map(Some).map_err(|e| format!("`{}`: {e}", p.name))
}

impl Module for ExtModuleDef {
    fn path(&self) -> &str {
        &self.path
    }
    fn doc(&self) -> &str {
        &self.spec.doc
    }
    fn kind(&self) -> Kind {
        match self.kind {
            ExtKind::Lua { .. } => Kind::Lua,
            ExtKind::Native { .. } => Kind::Native,
        }
    }
    fn params(&self) -> &[ParamDef] {
        &self.params
    }

    fn signature(&self, _: &Params, cx: &dyn SigCx) -> Result<Signature, String> {
        let mut s = Signature::new();
        for p in &self.spec.inputs {
            s = match pin_type(p, cx)? {
                None => s.tick_in(&p.name),
                Some(Type::Image) if matches!(self.kind, ExtKind::Lua { .. }) => {
                    return Err(format!("`{}`: Lua modules can't take images", p.name));
                }
                Some(t) if t.is_scalar() => {
                    let d = p.default.clone().unwrap_or_else(|| default_text(&t).to_string());
                    s.input(&p.name, TExpr::T(t), &d)
                }
                Some(t) => s.wire_in(&p.name, TExpr::T(t)),
            };
        }
        for p in &self.spec.outputs {
            s = match pin_type(p, cx)? {
                None => s.tick_out(&p.name),
                Some(t) => s.output(&p.name, TExpr::T(t)),
            };
        }
        let ticks_in = s.has_tick_input();
        if ticks_in && !self.spec.has_tick {
            return Err("it has tick inputs and no `tick` function".into());
        }
        if !ticks_in && !self.spec.has_eval && s.outputs.iter().any(|o| o.ty != PinTy::Tick) && !self.spec.has_start {
            return Err("it has data outputs and no `eval` function".into());
        }
        Ok(s)
    }

    fn eval(&self, cx: &mut Lx, output: usize) -> Result<Expr, String> {
        if cx.sig().has_tick_input() || !self.spec.has_eval {
            return Ok(Expr::Slot(cx.out_slot(output)));
        }
        let instance = self.instance(cx)?;
        let inputs = self.data_inputs(cx)?;
        let pin = &cx.sig().outputs[output];
        Ok(Expr::Ext { instance, output: pin.name.clone(), ty: cx.out_ty(output), inputs })
    }

    fn tick(&self, cx: &mut Lx, input: usize) -> Result<Block, String> {
        let name = cx.sig().inputs[input].name.clone();
        self.call(cx, Some(name))
    }

    fn start(&self, cx: &mut Lx) -> Result<Option<Block>, String> {
        if !self.spec.has_start {
            return Ok(None);
        }
        self.call(cx, None).map(Some)
    }
}

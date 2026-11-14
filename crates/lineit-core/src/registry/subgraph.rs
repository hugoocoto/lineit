//! Subgraphs: a canvas used as a module. Its `graph/input` and
//! `graph/output` squares are its pins, from top to bottom. Lowering inlines
//! a copy of the canvas for each square of it.

use std::path::{Path, PathBuf};

use lineit_rt::Value;

use super::{FnPorts, Kind, Module, ParamDef, Params, SigCx, Signature, TExpr};
use crate::model::Canvas;

pub struct Subgraph {
    pub path: String,
    pub canvas: Canvas,
    /// Its file, if it has one.
    pub file: Option<PathBuf>,
}

impl Subgraph {
    pub fn read(path: &str, source: &str, file: Option<&Path>) -> Result<Subgraph, String> {
        let canvas = crate::file::read_canvas(source, path)?;
        Ok(Subgraph { path: path.into(), canvas, file: file.map(Path::to_path_buf) })
    }

    /// Its ports of MODULE (`graph/input` or `graph/output`), from top to
    /// bottom, as (name, type, default).
    pub fn ports(&self, module: &str) -> Vec<(String, String, String)> {
        let mut squares: Vec<_> = self.canvas.squares.iter().filter(|s| s.module == module).collect();
        squares.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
        let get =
            |s: &crate::model::Square, k: &str, d: &str| s.params.get(k).cloned().unwrap_or_else(|| d.to_string());
        squares
            .into_iter()
            .map(|s| {
                let default_name = if module == "graph/input" { "in" } else { "out" };
                (get(s, "name", default_name), get(s, "type", "int"), get(s, "default", "0"))
            })
            .collect()
    }

    /// Its pins as a function: its data ports, and whether it has a tick
    /// input (tick outputs don't matter: a call ends when what it ticks has).
    pub fn fn_ports(&self, cx: &dyn SigCx) -> Result<FnPorts, String> {
        let mut f = FnPorts { tick: false, inputs: Vec::new(), outputs: Vec::new() };
        for (name, ty, default) in self.ports("graph/input") {
            if ty == "tick" {
                if std::mem::replace(&mut f.tick, true) {
                    return Err(format!("`{}` has more than one tick input, and a function takes one", self.path));
                }
            } else {
                f.inputs.push((name, cx.parse_type(&ty)?, default));
            }
        }
        for (name, ty, _) in self.ports("graph/output") {
            if ty != "tick" {
                f.outputs.push((name, cx.parse_type(&ty)?));
            }
        }
        Ok(f)
    }
}

impl Module for Subgraph {
    fn path(&self) -> &str {
        &self.path
    }
    fn doc(&self) -> &str {
        &self.canvas.doc
    }
    fn kind(&self) -> Kind {
        Kind::Subgraph
    }
    fn params(&self) -> &[ParamDef] {
        &[]
    }
    fn signature(&self, _: &Params, cx: &dyn SigCx) -> Result<Signature, String> {
        let mut s = Signature::new();
        for (name, ty, default) in self.ports("graph/input") {
            s = if ty == "tick" {
                s.tick_in(&name)
            } else {
                let t = cx.parse_type(&ty)?;
                if t.is_scalar() {
                    let d = if Value::parse(&default, &t).is_some() { default } else { String::new() };
                    s.input(&name, TExpr::T(t), &d)
                } else {
                    s.wire_in(&name, TExpr::T(t))
                }
            };
        }
        for (name, ty, _) in self.ports("graph/output") {
            s = if ty == "tick" { s.tick_out(&name) } else { s.output(&name, TExpr::T(cx.parse_type(&ty)?)) };
        }
        Ok(s)
    }
    fn subgraph(&self) -> Option<&Subgraph> {
        Some(self)
    }
}

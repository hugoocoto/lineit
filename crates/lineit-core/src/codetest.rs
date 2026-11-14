//! The tests of a code square: a table on the square, each row a call of
//! its function, with what it's given and what it should give.
//!
//! - **Given**: its inputs (by pin name), its state (by name: `count`) and
//!   the globals it uses (`::total`). An input left out has its field's
//!   value; state left out is what the row before left (the first row
//!   starts as the code does), so rows one after the other test a
//!   sequence.
//! - **Expected**: `fires`, the tick outputs it fires, in order (`even`,
//!   `item, item`, `then`; empty: none), its outputs, its state after the
//!   call and the globals it sets. What's left out isn't checked.
//!
//! Values are typed as fields are (`2`, `2.5`, `true`, text as it is);
//! lists, maps and records as Lua tables: `{1, 2}`, `{a = 1}`,
//! `{x = 1.5, y = 2}`. The rows are kept in the square's `tests` param, as
//! a Lua table (read in the canvas's sandbox).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use lineit_rt::{Type, Value};

use crate::file::{key, quote};
use crate::ir::ExtKind;
use crate::registry::code::{Function, Role, shared_pin};

/// The key of the tick outputs a row expects.
pub const FIRES: &str = "fires";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    pub given: BTreeMap<String, String>,
    pub expect: BTreeMap<String, String>,
}

/// The rows in TEXT (a square's `tests` param).
pub fn read_rows(text: &str) -> Result<Vec<Row>, String> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    crate::file::with_data(&format!("return {text}"), "tests", |t| {
        let mut rows = Vec::new();
        for row in t.sequence_values::<mlua::Table>() {
            let row = row.map_err(|_| "each test is a table: { given = {...}, expect = {...} }".to_string())?;
            let half = |k: &str| -> Result<BTreeMap<String, String>, String> {
                let mut m = BTreeMap::new();
                if let Ok(t) = row.get::<mlua::Table>(k) {
                    for pair in t.pairs::<String, mlua::Value>() {
                        let (k, v) = pair.map_err(|e| e.to_string())?;
                        let v = match v {
                            mlua::Value::String(s) => s.to_string_lossy(),
                            v => v.to_string().map_err(|e| e.to_string())?,
                        };
                        m.insert(k, v);
                    }
                }
                Ok(m)
            };
            rows.push(Row { given: half("given")?, expect: half("expect")? });
        }
        Ok(rows)
    })
}

/// ROWS as a square's `tests` param.
pub fn write_rows(rows: &[Row]) -> String {
    let half = |m: &BTreeMap<String, String>| {
        let items: Vec<String> = m.iter().map(|(k, v)| format!("{} = {}", key(k), quote(v))).collect();
        format!("{{ {} }}", items.join(", "))
    };
    let mut o = String::from("{\n");
    for r in rows {
        let _ = writeln!(o, "  {{ given = {}, expect = {} }},", half(&r.given), half(&r.expect));
    }
    o.push('}');
    o
}

/// TEXT as a value of TY: as a field reads it, or (lists, maps, records)
/// a Lua table.
pub fn value_of(text: &str, ty: &Type) -> Result<Value, String> {
    if ty.is_scalar() {
        let an = if matches!(ty, Type::Int) { "an" } else { "a" };
        return Value::parse(text, ty).ok_or_else(|| format!("`{text}` isn't {an} {ty}"));
    }
    let lua = mlua::Lua::new();
    let v: mlua::Value = crate::file::with_data(&format!("return {{ v = {text} }}"), "value", |t| {
        let v: mlua::Value = t.get("v").map_err(|e| e.to_string())?;
        // out of the sandbox: into a Lua of its own, as a value
        Ok(match v {
            mlua::Value::Table(t) => mlua::Value::Table(copy_table(&lua, &t)?),
            v => copy_plain(&lua, v)?,
        })
    })
    .map_err(|e| format!("`{text}`: {e}"))?;
    lineit_rt::lua::from_lua(v, ty).map_err(|e| format!("`{text}`: {e}"))
}

fn copy_plain(lua: &mlua::Lua, v: mlua::Value) -> Result<mlua::Value, String> {
    Ok(match v {
        mlua::Value::String(s) => mlua::Value::String(lua.create_string(s.as_bytes()).map_err(|e| e.to_string())?),
        mlua::Value::Table(t) => mlua::Value::Table(copy_table(lua, &t)?),
        v => v,
    })
}

fn copy_table(lua: &mlua::Lua, t: &mlua::Table) -> Result<mlua::Table, String> {
    let out = lua.create_table().map_err(|e| e.to_string())?;
    for pair in t.pairs::<mlua::Value, mlua::Value>() {
        let (k, v) = pair.map_err(|e| e.to_string())?;
        out.set(copy_plain(lua, k)?, copy_plain(lua, v)?).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// How a row went.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RowResult {
    /// What's wrong, if anything: a line each.
    pub problems: Vec<String>,
    /// What the code printed.
    pub printed: Vec<String>,
}

impl RowResult {
    pub fn ok(&self) -> bool {
        self.problems.is_empty()
    }
}

/// The pins and types of what F gives: its outputs, and the globals it sets.
fn outputs(f: &Function) -> Vec<(String, Type)> {
    let mut v: Vec<(String, Type)> = Vec::new();
    if f.exits().next().is_some() {
        for (_, values) in f.exits() {
            v.extend(values.iter().cloned());
        }
    } else {
        v.extend(f.returns.iter().cloned());
    }
    for ((g, set), t) in f.shared.iter().zip(&f.shared_types) {
        if *set {
            v.push((shared_pin(g), t.clone()));
        }
    }
    v
}

/// Runs ROWS on a square of F (run by KIND), one after the other. Each
/// input with no value in a row has DEFAULTS' (its field's text).
pub fn run(
    f: &Function,
    kind: &ExtKind,
    rows: &[Row],
    defaults: &BTreeMap<String, String>,
) -> Result<Vec<RowResult>, String> {
    let mut ext = crate::interp::open_ext(&f.name, kind)?;
    let sq = ext.instance(&[])?;
    let outs = outputs(f);
    let out_refs: Vec<(&str, &Type)> = outs.iter().map(|(n, t)| (n.as_str(), t)).collect();
    let mut results = Vec::new();
    for row in rows {
        let mut r = RowResult::default();
        let mut problem = |p: String| r.problems.push(p);

        // what it's given: names it doesn't have are mistakes
        for k in row.given.keys() {
            let known = f.inputs().any(|(n, _)| n == k)
                || f.state.iter().any(|s| s == k)
                || f.shared.iter().any(|(g, _)| shared_pin(g) == *k);
            if !known {
                problem(format!("given `{k}`: it has no input, state or global of that name"));
            }
        }
        for k in row.expect.keys() {
            let known = k == FIRES || outs.iter().any(|(n, _)| n == k) || f.state.iter().any(|s| s == k);
            if !known {
                problem(format!("expected `{k}`: it has no output, state or global it sets of that name"));
            }
        }

        // its state, where the row gives it
        let now: BTreeMap<String, Value> = ext.state(sq).unwrap_or_default().into_iter().collect();
        let mut state = Vec::new();
        for s in &f.state {
            if let Some(text) = row.given.get(s) {
                let ty = now.get(s).map(|v| v.ty()).unwrap_or_else(|| Value::literal_type(text));
                match value_of(text, &ty) {
                    Ok(v) => state.push((s.as_str(), v)),
                    Err(e) => problem(format!("given `{s}`: {e}")),
                }
            }
        }
        if !state.is_empty()
            && let Err(e) = ext.set_state(sq, &state)
        {
            problem(format!("its state can't be set: {e}"));
        }

        // its inputs and globals
        let mut inputs: Vec<(String, Value)> = Vec::new();
        for p in &f.params {
            let Role::Input(ty) = &p.role else { continue };
            let text = row.given.get(&p.name).or_else(|| defaults.get(&p.name)).cloned().unwrap_or_default();
            let ty = ty.clone().unwrap_or_else(|| Value::literal_type(&text));
            match value_of(&text, &ty) {
                Ok(v) => inputs.push((p.name.clone(), v)),
                Err(e) => problem(format!("given `{}`: {e}", p.name)),
            }
        }
        for ((g, _), t) in f.shared.iter().zip(&f.shared_types) {
            let pin = shared_pin(g);
            let v = match row.given.get(&pin) {
                Some(text) => match value_of(text, t) {
                    Ok(v) => v,
                    Err(e) => {
                        problem(format!("given `{pin}`: {e}"));
                        continue;
                    }
                },
                None => Value::default_of(t),
            };
            inputs.push((pin, v));
        }
        if !r.problems.is_empty() {
            results.push(r);
            continue;
        }

        // the call
        let args: Vec<(&str, Value)> = inputs.iter().map(|(n, v)| (n.as_str(), v.clone())).collect();
        let got = if f.ticked() {
            ext.run(sq, Some("tick"), &args, &out_refs)
        } else {
            let returns: Vec<(&str, &Type)> = f.returns.iter().map(|(n, t)| (n.as_str(), t)).collect();
            ext.eval(sq, &args, &returns)
                .map(|v| lineit_rt::ext::Run { set: v.into_iter().map(Some).collect(), fire: Vec::new() })
        };
        r.printed = lineit_rt::ext::take_printed();
        let run = match got {
            Ok(run) => run,
            Err(e) => {
                r.problems.push(format!("it failed: {e}"));
                results.push(r);
                continue;
            }
        };

        // what it gave: each output's last value, and what it fired
        let mut last: Vec<Option<Value>> = run.set.clone();
        last.resize(outs.len(), None);
        let mut fired = Vec::new();
        for (name, values) in &run.fire {
            fired.push(name.as_str());
            for (o, v) in values {
                if let Some(slot) = last.get_mut(*o) {
                    *slot = Some(v.clone());
                }
            }
        }
        if let Some(want) = row.expect.get(FIRES) {
            let want: Vec<&str> = want.split(',').map(str::trim).filter(|w| !w.is_empty()).collect();
            if want != fired {
                r.problems.push(format!("fires: expected {}, got {}", list(&want), list(&fired)));
            }
        }
        for (k, (pin, ty)) in outs.iter().enumerate() {
            let Some(text) = row.expect.get(pin) else { continue };
            match (value_of(text, ty), &last[k]) {
                (Err(e), _) => r.problems.push(format!("expected `{pin}`: {e}")),
                (Ok(_), None) => r.problems.push(format!("{pin}: expected {text}, it gave nothing")),
                (Ok(want), Some(got)) if !lineit_rt::value::same(&want, got) => {
                    r.problems.push(format!("{pin}: expected {}, got {}", want.text(), got.text()))
                }
                _ => {}
            }
        }
        let after: BTreeMap<String, Value> = ext.state(sq).unwrap_or_default().into_iter().collect();
        for s in &f.state {
            let Some(text) = row.expect.get(s) else { continue };
            let Some(got) = after.get(s) else {
                r.problems.push(format!("{s}: expected {text}, it has none"));
                continue;
            };
            match value_of(text, &got.ty()) {
                Err(e) => r.problems.push(format!("expected `{s}`: {e}")),
                Ok(want) if !lineit_rt::value::same(&want, got) => {
                    r.problems.push(format!("{s}: expected {}, got {}", want.text(), got.text()))
                }
                _ => {}
            }
        }
        results.push(r);
    }
    Ok(results)
}

/// The tests of a code square of a canvas, and how they went.
pub struct SquareTests {
    pub square: crate::model::SquareId,
    /// Its function's name.
    pub name: String,
    pub rows: usize,
    /// Per row; or why they couldn't run.
    pub results: Result<Vec<RowResult>, String>,
}

/// Runs the tests of every code square of CANVAS (as L lowered it) that
/// has some.
pub fn run_canvas(canvas: &crate::model::Canvas, l: &crate::lower::Lowered) -> Vec<SquareTests> {
    let mut out = Vec::new();
    for sq in canvas.squares.iter().filter(|s| s.module == crate::registry::code::PATH) {
        let text = sq.params.get("tests").map(String::as_str).unwrap_or("");
        let rows = match read_rows(text) {
            Ok(r) if r.is_empty() => continue,
            Ok(r) => r,
            Err(e) => {
                out.push(SquareTests {
                    square: sq.id,
                    name: String::new(),
                    rows: 0,
                    results: Err(format!("its tests: {e}")),
                });
                continue;
            }
        };
        let Some(info) = l.squares.get(&sq.id) else { continue };
        let Some(f) = info.sig.code.clone() else {
            out.push(SquareTests {
                square: sq.id,
                name: String::new(),
                rows: rows.len(),
                results: Err("its code has a problem".into()),
            });
            continue;
        };
        // an input with no value in a row has its field's
        let mut defaults = BTreeMap::new();
        for p in &info.sig.inputs {
            if let Some(d) = &p.field {
                defaults.insert(p.name.clone(), sq.fields.get(&p.name).unwrap_or(d).clone());
            }
        }
        let lang = sq.params.get("language").cloned().unwrap_or_else(crate::registry::code::default_language);
        let results = crate::registry::code::ext_of(&lang, &f).and_then(|kind| run(&f, &kind, &rows, &defaults));
        out.push(SquareTests { square: sq.id, name: f.name.clone(), rows: rows.len(), results });
    }
    out
}

/// How a `test/case` went.
#[derive(Clone, Debug)]
pub struct CaseResult {
    pub square: crate::model::SquareId,
    pub name: String,
    /// The `test/expect`s the run got to, in order.
    pub checks: Vec<crate::interp::CheckResult>,
    /// What went wrong, if the run didn't end well (or didn't run).
    pub error: Option<String>,
    /// What it printed.
    pub output: Vec<String>,
}

impl CaseResult {
    /// It ran to its end, and every check it got to passed.
    pub fn ok(&self) -> bool {
        self.error.is_none() && self.checks.iter().all(|c| c.ok)
    }
}

/// How long a test may run (a canvas with a clock doesn't end).
pub const CASE_SECONDS: f64 = 10.0;

/// The `test/case` squares of CANVAS, by their name.
pub fn cases(canvas: &crate::model::Canvas) -> Vec<(crate::model::SquareId, String)> {
    canvas
        .squares
        .iter()
        .filter(|s| s.module == "test/case")
        .map(|s| (s.id, s.params.get("name").cloned().unwrap_or_else(|| "a test".into())))
        .collect()
}

/// Runs each `test/case` of CANVAS, a run each.
pub fn run_cases(canvas: &crate::model::Canvas, reg: &crate::registry::Registry) -> Vec<CaseResult> {
    run_cases_within(canvas, reg, CASE_SECONDS)
}

/// [`run_cases`], each case stopped after SECONDS: also one stuck in a code
/// square's call, which never gets back to the interpreter by itself.
pub fn run_cases_within(
    canvas: &crate::model::Canvas,
    reg: &crate::registry::Registry,
    seconds: f64,
) -> Vec<CaseResult> {
    let dir = canvas.dir.clone().unwrap_or_default();
    cases(canvas)
        .into_iter()
        .map(|(square, name)| {
            let mut r = CaseResult { square, name, checks: Vec::new(), error: None, output: Vec::new() };
            let l = crate::lower::lower_test(canvas, reg, square);
            let Some(prog) = l.program else {
                let n = l.diags.len();
                r.error = Some(format!("the canvas has {n} problem{}", if n == 1 { "" } else { "s" }));
                return r;
            };
            let mut it = match crate::interp::Interp::new(prog, dir.clone()) {
                Ok(it) => it,
                Err(e) => {
                    r.error = Some(e);
                    return r;
                }
            };
            let started = std::time::Instant::now();
            // what stops it at the limit, wherever it is
            let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let timed_out = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let watchdog = {
                let (w, done, timed_out) = (it.w.clone(), done.clone(), timed_out.clone());
                std::thread::spawn(move || {
                    let limit = std::time::Duration::from_secs_f64(seconds);
                    while !done.load(std::sync::atomic::Ordering::Relaxed) {
                        if started.elapsed() > limit {
                            timed_out.store(true, std::sync::atomic::Ordering::Relaxed);
                            w.stop.store(true, std::sync::atomic::Ordering::Relaxed);
                            w.kill_externals();
                            return;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(20));
                    }
                })
            };
            let run = (|| -> Result<(), String> {
                it.start()?;
                loop {
                    let st = it.run(100_000)?;
                    if timed_out.load(std::sync::atomic::Ordering::Relaxed) {
                        return Err(String::new());
                    }
                    if st == crate::interp::Status::Idle {
                        let Some(wait) = it.next_clock() else { return Ok(()) };
                        // a clock: it runs until the limit, and that's fine
                        if started.elapsed().as_secs_f64() + wait > seconds {
                            return Ok(());
                        }
                        std::thread::sleep(std::time::Duration::from_secs_f64(wait));
                        it.tick_clocks(false)?;
                    }
                }
            })();
            done.store(true, std::sync::atomic::Ordering::Relaxed);
            let _ = watchdog.join();
            // stopped at the limit: that's what went wrong, whatever the run said
            let run = if timed_out.load(std::sync::atomic::Ordering::Relaxed) {
                Err(format!(
                    "it ran for more than {seconds} second{}: stuck in a loop?",
                    if seconds == 1.0 { "" } else { "s" }
                ))
            } else {
                run
            };
            if run.is_err() {
                it.w.kill_externals();
            }
            // what it did, however it ended
            r.error = run.err();
            r.checks = it.w.checks();
            r.output = it.w.take_output();
            r
        })
        .collect()
}

fn list(v: &[&str]) -> String {
    if v.is_empty() { "nothing".into() } else { v.join(", ") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_there_and_back() {
        let rows = vec![
            Row {
                given: BTreeMap::from([("n".into(), "4".into()), ("::total".into(), "10".into())]),
                expect: BTreeMap::from([("fires".into(), "even".into()), ("half".into(), "2".into())]),
            },
            Row { given: BTreeMap::new(), expect: BTreeMap::from([("text".into(), "a \"b\"\n".into())]) },
        ];
        assert_eq!(read_rows(&write_rows(&rows)).unwrap(), rows);
        assert_eq!(read_rows("").unwrap(), []);
        assert_eq!(value_of("{1, 2}", &Type::List(Box::new(Type::Int))).unwrap().text(), "[1, 2]");
        assert_eq!(value_of("{a = 1.5}", &Type::Map(Box::new(Type::Float))).unwrap().text(), "{\"a\": 1.5}");
        assert_eq!(value_of("x", &Type::Int).unwrap_err(), "`x` isn't an int");
    }

    #[test]
    fn failures() {
        struct Cx;
        impl crate::registry::SigCx for Cx {
            fn var(&self, n: &str) -> Option<Type> {
                (n == "::total").then_some(Type::Int)
            }
            fn record(&self, _: &str) -> Option<std::sync::Arc<lineit_rt::RecordType>> {
                None
            }
            fn read(&self, p: &str) -> Result<String, String> {
                Err(p.into())
            }
        }
        let m = crate::registry::code::CodeModule::new();
        let code = "count = 0\n---@param n integer\n---@param even fun(half: integer)\n---@param odd fun(calls: integer)\nfunction collatz(n, even, odd)\n    Shared.total = Shared.total + n\n    if n == 99 then error(\"not 99\") end\n    if n % 2 == 0 then return even(n // 2) end\n    count = count + 1\n    print(\"odd\", n)\n    return odd(count)\nend\n";
        let values = BTreeMap::from([("code".to_string(), code.to_string())]);
        let sig = crate::registry::Module::signature(
            &m,
            &crate::registry::Params { defs: crate::registry::Module::params(&m), values: &values },
            &Cx,
        )
        .unwrap();
        let f = sig.code.unwrap();
        let rows = read_rows(r#"{
            { given = { n = "4" }, expect = { fires = "odd", half = "3" } },
            { given = { n = "3", nn = "1" }, expect = { calls = "1", nope = "2" } },
            { given = { n = "99" }, expect = {} },
            { given = { n = "5", ["::total"] = "1" }, expect = { calls = "1", ["::total"] = "6", count = "1", fires = "odd" } },
        }"#)
        .unwrap();
        let kind = crate::registry::code::ext_of("lua", &f).unwrap();
        let r = run(&f, &kind, &rows, &BTreeMap::new()).unwrap();
        assert_eq!(r[0].problems, ["fires: expected odd, got even", "half: expected 3, got 2"]);
        assert_eq!(
            r[1].problems,
            [
                "given `nn`: it has no input, state or global of that name",
                "expected `nope`: it has no output, state or global it sets of that name"
            ]
        );
        assert_eq!(r[2].problems, ["it failed: runtime error: collatz:7: not 99"]);
        assert!(r[3].ok(), "{:?}", r[3]);
        assert_eq!(r[3].printed, ["odd\t5"]);
    }
}

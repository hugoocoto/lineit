//! Editing while running: `Interp::swap` goes on with the edited program and
//! keeps what the old one had.

use lineit_core::interp::{Interp, Status};
use lineit_core::ir::Program;
use lineit_core::lower::lower;
use lineit_core::registry::Registry;
use lineit_rt::Value;

/// A clock every INTERVAL seconds that adds INC to `total` (of type TY),
/// and EXTRA more variables, `extra`, each set to 7 on every tick.
fn canvas(interval: &str, inc: &str, ty: &str, extra: bool) -> lineit_core::model::Canvas {
    let mut squares = format!(
        r#"{{ id = 1, module = "flow/clock", fields = {{ interval = "{interval}" }} }},
        {{ id = 2, module = "var/var", params = {{ name = "total", type = "{ty}", initial = "0" }} }},
        {{ id = 3, module = "math/add", fields = {{ b = "{inc}" }} }},
        {{ id = 4, module = "var/set", params = {{ name = "total" }} }},
        {{ id = 5, module = "var/get", params = {{ name = "total" }} }},"#
    );
    let mut links = r#"{ 1, "tick", 4, "tick" }, { 5, "value", 3, "a" }, { 3, "result", 4, "value" },"#.to_string();
    if extra {
        squares += r#"
        { id = 6, module = "var/var", params = { name = "extra", type = "int", initial = "1" } },
        { id = 7, module = "value/constant", params = { type = "int", value = "7" } },
        { id = 8, module = "var/set", params = { name = "extra" } },"#;
        links += r#" { 4, "then", 8, "tick" }, { 7, "value", 8, "value" },"#;
    }
    let src = format!("return {{ lineit = 1, squares = {{ {squares} }}, links = {{ {links} }} }}");
    lineit_core::file::read_canvas(&src, "hot").unwrap()
}

fn program(c: &lineit_core::model::Canvas) -> Program {
    let l = lower(c, &Registry::builtin());
    assert!(l.diags.is_empty(), "{:?}", l.diags);
    l.program.unwrap()
}

fn start(c: &lineit_core::model::Canvas) -> Interp {
    let mut it = Interp::new(program(c), std::env::temp_dir()).unwrap();
    it.start().unwrap();
    it
}

/// One tick of the clocks, run to the end.
fn tick(it: &mut Interp) {
    it.tick_clocks(true).unwrap();
    while it.run(1 << 20).unwrap() != Status::Idle {}
}

fn var(it: &Interp, name: &str) -> Option<Value> {
    it.w.variables().into_iter().find(|(n, _)| n == name).map(|(_, v)| v)
}

#[test]
fn the_new_increment_applies_and_the_variable_keeps_its_value() {
    let mut it = start(&canvas("100", "1", "int", false));
    for _ in 0..3 {
        tick(&mut it);
    }
    assert_eq!(var(&it, "total"), Some(Value::Int(3)));
    let r = it.swap(program(&canvas("100", "10", "int", false)), false).unwrap();
    assert_eq!(r.kept, 1);
    assert_eq!(var(&it, "total"), Some(Value::Int(3)));
    tick(&mut it);
    assert_eq!(var(&it, "total"), Some(Value::Int(13)));
}

#[test]
fn a_removed_square_drops_its_state_and_a_new_one_starts() {
    let mut it = start(&canvas("100", "1", "int", true));
    tick(&mut it);
    assert_eq!(var(&it, "extra"), Some(Value::Int(7)));
    let r = it.swap(program(&canvas("100", "1", "int", false)), false).unwrap();
    assert_eq!((r.kept, r.dropped), (1, 1));
    assert_eq!(var(&it, "extra"), None);
    assert_eq!(var(&it, "total"), Some(Value::Int(1)));
    // and back: the new variable starts at its initial value
    it.swap(program(&canvas("100", "1", "int", true)), false).unwrap();
    assert_eq!(var(&it, "extra"), Some(Value::Int(1)));
    tick(&mut it);
    assert_eq!((var(&it, "total"), var(&it, "extra")), (Some(Value::Int(2)), Some(Value::Int(7))));
}

#[test]
fn a_type_change_converts_the_value() {
    let mut it = start(&canvas("100", "1", "int", true));
    tick(&mut it);
    tick(&mut it);
    let r = it.swap(program(&canvas("100", "0.5", "float", true)), false).unwrap();
    assert_eq!(r.converted, vec!["total".to_string()]);
    assert!(r.reset.is_empty());
    assert_eq!(var(&it, "total"), Some(Value::Float(2.0)));
    assert_eq!(var(&it, "extra"), Some(Value::Int(7)));
    tick(&mut it);
    assert_eq!(var(&it, "total"), Some(Value::Float(2.5)));
}

#[test]
fn a_type_with_no_conversion_starts_again() {
    let list = |ty: &str| {
        let src = format!(
            r#"return {{ lineit = 1, squares = {{
            {{ id = 1, module = "var/var", params = {{ name = "x", type = "{ty}", initial = "3" }} }},
            {{ id = 2, module = "flow/entry" }},
            {{ id = 3, module = "var/set", params = {{ name = "x" }} }},
            }}, links = {{ {{ 2, "then", 3, "tick" }} }} }}"#
        );
        lineit_core::file::read_canvas(&src, "list").unwrap()
    };
    let mut it = start(&list("int"));
    while it.run(1 << 20).unwrap() != Status::Idle {}
    let r = it.swap(program(&list("list<int>")), false).unwrap();
    assert_eq!(r.reset, vec!["x".to_string()]);
}

#[test]
fn an_edit_with_errors_doesnt_lower_and_the_old_program_goes_on() {
    let mut it = start(&canvas("100", "1", "int", false));
    tick(&mut it);
    // a wire from a string into an int
    let mut bad = canvas("100", "1", "int", false);
    bad.squares.iter_mut().find(|s| s.id == 3).unwrap().fields.insert("b".into(), "\"x\"".into());
    let l = lower(&bad, &Registry::builtin());
    assert!(l.program.is_none() && !l.diags.is_empty());
    // the editor has nothing to swap in: the run is as it was
    tick(&mut it);
    assert_eq!(var(&it, "total"), Some(Value::Int(2)));
}

#[test]
fn it_waits_for_the_branches_in_the_middle_of_a_run() {
    let src = r#"return { lineit = 1, squares = {
        { id = 1, module = "flow/entry" },
        { id = 2, module = "io/print", fields = { value = "a" } },
        { id = 3, module = "io/print", fields = { value = "b" } },
    }, links = { { 1, "then", 2, "tick" }, { 2, "then", 3, "tick" } } }"#;
    let c = lineit_core::file::read_canvas(src, "steps").unwrap();
    let mut it = start(&c);
    assert!(!it.can_swap());
    assert!(it.swap(program(&c), false).is_err());
    while it.run(1 << 20).unwrap() != Status::Idle {}
    assert!(it.can_swap());
}

#[test]
fn the_clock_keeps_its_schedule_unless_the_new_interval_comes_first() {
    let mut it = start(&canvas("100", "1", "int", false));
    assert!(it.next_clock().unwrap() > 90.0);
    it.swap(program(&canvas("100", "2", "int", false)), false).unwrap();
    assert!(it.next_clock().unwrap() > 90.0);
    it.swap(program(&canvas("0.5", "2", "int", false)), false).unwrap();
    assert!(it.next_clock().unwrap() <= 0.5);
}

/// Branches on real threads: two of them add to two variables on each
/// tick, and the program is swapped (for a copy, and back) between ticks:
/// nothing is lost or torn.
#[test]
fn branches_on_threads_across_swaps() {
    let canvas = |inc: &str| {
        let src = format!(
            r#"return {{ lineit = 1, squares = {{
            {{ id = 1, module = "flow/clock", fields = {{ interval = "100" }} }},
            {{ id = 2, module = "var/var", params = {{ name = "a", type = "int", initial = "0" }} }},
            {{ id = 3, module = "var/var", params = {{ name = "b", type = "int", initial = "0" }} }},
            {{ id = 4, module = "var/get", params = {{ name = "a" }} }},
            {{ id = 5, module = "math/add", fields = {{ b = "{inc}" }} }},
            {{ id = 6, module = "var/set", params = {{ name = "a" }} }},
            {{ id = 7, module = "var/get", params = {{ name = "b" }} }},
            {{ id = 8, module = "math/add", fields = {{ b = "{inc}" }} }},
            {{ id = 9, module = "var/set", params = {{ name = "b" }} }},
            }}, links = {{
            {{ 1, "tick", 6, "tick" }}, {{ 1, "tick", 9, "tick" }},
            {{ 4, "value", 5, "a" }}, {{ 5, "result", 6, "value" }},
            {{ 7, "value", 8, "a" }}, {{ 8, "result", 9, "value" }},
            }} }}"#
        );
        lineit_core::file::read_canvas(&src, "threads").unwrap()
    };
    let mut it = start(&canvas("1"));
    for _ in 0..50 {
        tick(&mut it);
        it.swap(program(&canvas("1")), false).unwrap();
    }
    assert_eq!((var(&it, "a"), var(&it, "b")), (Some(Value::Int(50)), Some(Value::Int(50))));
    it.swap(program(&canvas("3")), false).unwrap();
    tick(&mut it);
    assert_eq!((var(&it, "a"), var(&it, "b")), (Some(Value::Int(53)), Some(Value::Int(53))));
}

/// A clock that ticks a code square in LANGUAGE (CODE has a function that
/// keeps `ticks` and returns it), which a string/format prints with FORMAT.
fn counter(language: &str, code: &str, format: &str) -> lineit_core::model::Canvas {
    let src = format!(
        r#"return {{ lineit = 1, squares = {{
        {{ id = 1, module = "flow/clock", fields = {{ interval = "100" }} }},
        {{ id = 2, module = "code/function", params = {{ language = "{language}", code = {} }} }},
        {{ id = 3, module = "string/format", params = {{ format = "{format}" }} }},
        {{ id = 4, module = "io/print" }},
        }}, links = {{ {{ 1, "tick", 2, "tick" }}, {{ 2, "result", 3, "1" }}, {{ 3, "text", 4, "value" }},
        {{ 2, "then", 4, "tick" }} }} }}"#,
        lineit_core::file::quote(code)
    );
    lineit_core::file::read_canvas(&src, "counter").unwrap()
}

fn ticks(it: &Interp) -> Option<Value> {
    it.w.code_states().get(&2)?.iter().find(|(n, _)| n == "ticks").map(|(_, v)| v.clone())
}

fn output(it: &Interp) -> Vec<String> {
    it.w.take_output()
}

const PYTHON: &str = "ticks = 0\n\ndef counter() -> int:\n    global ticks\n    ticks += 1\n    return ticks\n";
const CSHARP: &str = "static class Counter\n{\n    static long ticks = 0;\n\n    public static long Counter1()\n    {\n        ticks++;\n        return ticks;\n    }\n}\n";

/// A code square keeps its state across an edit elsewhere (a process
/// isn't restarted), and loses it when its own code changes.
fn keeps_state(language: &str, code: &str, other: &str) {
    let mut it = start(&counter(language, code, "tick {}"));
    tick(&mut it);
    tick(&mut it);
    assert_eq!(ticks(&it), Some(Value::Int(2)), "{language}");
    assert_eq!(output(&it), vec!["tick 1", "tick 2"]);
    it.swap(program(&counter(language, code, "number {}")), false).unwrap();
    assert_eq!(ticks(&it), Some(Value::Int(2)), "{language}: the state is kept");
    tick(&mut it);
    assert_eq!(output(&it), vec!["number 3"], "{language}");
    // other code: another process, from the start
    it.swap(program(&counter(language, other, "number {}")), false).unwrap();
    tick(&mut it);
    assert_eq!(output(&it), vec!["number 2"], "{language}: new code starts again, from zero, adding 2");
}

#[test]
fn a_python_square_keeps_its_state() {
    keeps_state("python", PYTHON, &PYTHON.replace("ticks += 1", "ticks += 2"));
}

#[test]
fn a_csharp_square_keeps_its_state_and_the_files_changing_opens_it_again() {
    if !lineit_core::registry::code::sdk_ready() {
        eprintln!("skipped: no .NET SDK");
        return;
    }
    keeps_state("csharp", CSHARP, &CSHARP.replace("ticks++", "ticks += 2"));
    let mut it = start(&counter("csharp", CSHARP, "tick {}"));
    tick(&mut it);
    tick(&mut it);
    // files it's built from changed: it ends and builds again
    it.swap(program(&counter("csharp", CSHARP, "tick {}")), true).unwrap();
    assert_eq!(ticks(&it), Some(Value::Int(0)));
    tick(&mut it);
    assert_eq!(ticks(&it), Some(Value::Int(1)));
}

#[test]
fn a_failed_swap_leaves_the_old_program_as_it_was() {
    // the interval comes from a Lua function: the edit's fails, when the clocks are set
    let canvas = |iv: &str| {
        let src = format!(
            r#"return {{ lineit = 1, squares = {{
            {{ id = 1, module = "flow/clock" }},
            {{ id = 2, module = "code/function", params = {{ code = {} }} }},
            {{ id = 3, module = "code/function", params = {{ code = {} }} }},
            }}, links = {{ {{ 2, "result", 1, "interval" }}, {{ 1, "tick", 3, "tick" }} }} }}"#,
            lineit_core::file::quote(iv),
            lineit_core::file::quote(
                "ticks = 0\n\n---@return integer\nfunction counter()\n    ticks = ticks + 1\n    return ticks\nend\n"
            )
        );
        lineit_core::file::read_canvas(&src, "failing").unwrap()
    };
    let good = "---@return number\nfunction iv()\n    return 100.0\nend\n";
    let bad = "---@return number\nfunction iv()\n    error('boom')\nend\n";
    let mut it = start(&canvas(good));
    tick(&mut it);
    let before = it.w.clone();
    let e = it.swap(program(&canvas(bad)), false).unwrap_err();
    assert!(e.contains("interval"), "{e}");
    assert!(std::sync::Arc::ptr_eq(&before, &it.w));
    tick(&mut it);
    let state = it.w.code_states();
    assert_eq!(state[&3].iter().find(|(n, _)| n == "ticks").map(|(_, v)| v.clone()), Some(Value::Int(2)));
}

#[test]
fn a_new_entry_runs_after_the_swap_and_the_old_ones_dont() {
    let canvas = |two: bool| {
        let extra = if two {
            r#"{ id = 3, module = "flow/entry" }, { id = 4, module = "io/print", fields = { value = "b" } },"#
        } else {
            ""
        };
        let more = if two { r#", { 3, "then", 4, "tick" }"# } else { "" };
        let src = format!(
            r#"return {{ lineit = 1, squares = {{
            {{ id = 1, module = "flow/entry" }}, {{ id = 2, module = "io/print", fields = {{ value = "a" }} }}, {extra}
            }}, links = {{ {{ 1, "then", 2, "tick" }} {more} }} }}"#
        );
        lineit_core::file::read_canvas(&src, "entries").unwrap()
    };
    let mut it = start(&canvas(false));
    while it.run(1 << 20).unwrap() != Status::Idle {}
    assert_eq!(output(&it), vec!["a"]);
    let r = it.swap(program(&canvas(true)), false).unwrap();
    assert_eq!(r.started, 1);
    while it.run(1 << 20).unwrap() != Status::Idle {}
    assert_eq!(output(&it), vec!["b"]);
}

/// Stop reaches the processes of the world an edit made, and a new entry
/// that gets stuck is stopped.
#[test]
fn stop_kills_the_process_of_the_new_world() {
    let canvas = |code: Option<&str>| {
        let (extra, more) = match code {
            Some(c) => (
                format!(
                    r#"{{ id = 3, module = "flow/entry" }}, {{ id = 4, module = "code/function", params = {{ language = "python", code = {} }} }},"#,
                    lineit_core::file::quote(c)
                ),
                r#", { 3, "then", 4, "tick" }"#,
            ),
            None => (String::new(), ""),
        };
        let src = format!(
            r#"return {{ lineit = 1, squares = {{
            {{ id = 1, module = "flow/entry" }}, {{ id = 2, module = "io/print", fields = {{ value = "a" }} }}, {extra}
            }}, links = {{ {{ 1, "then", 2, "tick" }} {more} }} }}"#
        );
        lineit_core::file::read_canvas(&src, "stuck").unwrap()
    };
    let mut it = start(&canvas(None));
    while it.run(1 << 20).unwrap() != Status::Idle {}
    let live = it.live();
    it.swap(program(&canvas(Some("def spin():\n    while True:\n        pass\n"))), false).unwrap();
    let w = live.lock().unwrap().clone();
    assert!(std::sync::Arc::ptr_eq(&w, &it.w), "the cell follows the swap");
    let killer = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(500));
        w.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        w.kill_externals();
    });
    let started = std::time::Instant::now();
    let r = loop {
        match it.run(100_000) {
            Ok(Status::Idle) => break Ok(()),
            Ok(_) => {}
            Err(e) => break Err(e),
        }
    };
    killer.join().unwrap();
    assert!(r.is_err());
    assert!(started.elapsed().as_secs_f64() < 10.0);
}

/// A program that never goes idle: the branches are dropped, the edit goes
/// on with the state, and the entries run again.
#[test]
fn a_loop_that_never_ends_takes_the_edit_when_restarted() {
    // both variables count on a branch of their own, each in a loop of wires
    let canvas = |inc: &str| {
        let src = format!(
            r#"return {{ lineit = 1, squares = {{
            {{ id = 1, module = "flow/entry" }},
            {{ id = 2, module = "var/var", params = {{ name = "a", type = "int", initial = "0" }} }},
            {{ id = 3, module = "var/var", params = {{ name = "b", type = "int", initial = "0" }} }},
            {{ id = 4, module = "var/get", params = {{ name = "a" }} }},
            {{ id = 5, module = "math/add", fields = {{ b = "{inc}" }} }},
            {{ id = 6, module = "var/set", params = {{ name = "a" }} }},
            {{ id = 7, module = "var/get", params = {{ name = "b" }} }},
            {{ id = 8, module = "math/add", fields = {{ b = "{inc}" }} }},
            {{ id = 9, module = "var/set", params = {{ name = "b" }} }},
            }}, links = {{
            {{ 1, "then", 6, "tick" }}, {{ 1, "then", 9, "tick" }},
            {{ 6, "then", 6, "tick" }}, {{ 9, "then", 9, "tick" }},
            {{ 4, "value", 5, "a" }}, {{ 5, "result", 6, "value" }},
            {{ 7, "value", 8, "a" }}, {{ 8, "result", 9, "value" }},
            }} }}"#
        );
        lineit_core::file::read_canvas(&src, "forever").unwrap()
    };
    let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut it = Interp::new(program(&canvas("1")), std::env::temp_dir()).unwrap();
    it.set_interrupt(flag.clone());
    it.start().unwrap();
    let interrupt_soon = |flag: &std::sync::Arc<std::sync::atomic::AtomicBool>| {
        let flag = flag.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(150));
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
        })
    };
    let t = interrupt_soon(&flag);
    // the branches are on threads: this returns when they're told to stop
    it.run(1 << 20).unwrap();
    t.join().unwrap();
    assert!(it.can_swap(), "the branches are dropped");
    let (a, b) = (var(&it, "a").unwrap().int(), var(&it, "b").unwrap().int());
    assert!(a > 0 && b > 0);
    let r = it.swap(program(&canvas("1000")), false).unwrap();
    assert_eq!(r.kept, 2);
    assert_eq!((var(&it, "a").unwrap().int(), var(&it, "b").unwrap().int()), (a, b));
    it.restart().unwrap();
    let t = interrupt_soon(&flag);
    it.run(1 << 20).unwrap();
    t.join().unwrap();
    for (name, before) in [("a", a), ("b", b)] {
        let now = var(&it, name).unwrap().int();
        assert!(now > before && (now - before) % 1000 == 0, "{name}: {before} to {now}");
    }
}

/// A clock that ticks two copies of `test/accumulate` (a subgraph with a
/// `total` of its own) as branches, on threads: square 2 adds 5, square 3
/// adds INC. With FIRST, another copy comes before them in the canvas, so
/// the scopes of theirs are numbered after its.
fn accumulators(inc: &str, first: bool) -> Program {
    let other = if first { r#"{ id = 9, module = "test/accumulate", fields = { value = "100" } },"# } else { "" };
    let src = format!(
        r#"return {{ lineit = 1, squares = {{ {other}
        {{ id = 1, module = "flow/clock", fields = {{ interval = "100" }} }},
        {{ id = 2, module = "test/accumulate", fields = {{ value = "5" }} }},
        {{ id = 3, module = "test/accumulate", fields = {{ value = "{inc}" }} }},
        }}, links = {{ {{ 1, "tick", 2, "tick" }}, {{ 1, "tick", 3, "tick" }} }} }}"#
    );
    let c = lineit_core::file::read_canvas(&src, "accumulators").unwrap();
    let mut reg = Registry::builtin();
    reg.load_dir(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/modules"), "");
    let l = lower(&c, &reg);
    assert!(l.diags.is_empty(), "{:?}", l.diags);
    l.program.unwrap()
}

/// The value of the variable NAME inside the subgraph square SQUARE.
fn inside(it: &Interp, square: u32, name: &str) -> Option<Value> {
    let home = (vec![square.into()], format!("var {name}"));
    let s = it.program().slots.iter().position(|s| s.home == home)?;
    Some(it.w.slot(s))
}

#[test]
fn the_variables_inside_subgraphs_keep_their_values() {
    let mut it = Interp::new(accumulators("1", false), std::env::temp_dir()).unwrap();
    it.start().unwrap();
    for _ in 0..3 {
        tick(&mut it);
    }
    assert_eq!((inside(&it, 2, "total"), inside(&it, 3, "total")), (Some(Value::Int(15)), Some(Value::Int(3))));
    // an edit of one copy, and a new copy before both
    let r = it.swap(accumulators("10", true), false).unwrap();
    assert!(r.reset.is_empty() && r.converted.is_empty(), "{r}");
    assert_eq!((inside(&it, 2, "total"), inside(&it, 3, "total")), (Some(Value::Int(15)), Some(Value::Int(3))));
    tick(&mut it);
    assert_eq!((inside(&it, 2, "total"), inside(&it, 3, "total")), (Some(Value::Int(20)), Some(Value::Int(13))));
    // and the copy before them goes away: theirs stay
    it.swap(accumulators("10", false), false).unwrap();
    tick(&mut it);
    assert_eq!((inside(&it, 2, "total"), inside(&it, 3, "total")), (Some(Value::Int(25)), Some(Value::Int(23))));
}

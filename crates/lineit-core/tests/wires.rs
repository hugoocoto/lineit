//! A data input takes one wire: two are an error, on the square of the input.
//! A tick input takes any number.

use std::path::Path;

use lineit_core::file::read_canvas;
use lineit_core::lower::{self, Lowered};
use lineit_core::registry::Registry;

fn registry() -> Registry {
    let mut reg = Registry::builtin();
    reg.load_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/modules"), "");
    assert!(reg.errors.is_empty(), "{:?}", reg.errors);
    reg
}

fn lowered(reg: &Registry, src: &str) -> Lowered {
    lower::lower(&read_canvas(src, "test").unwrap(), reg)
}

/// What it says about the square, if anything.
fn about(l: &Lowered, square: u64) -> Vec<&str> {
    l.diags.iter().filter(|d| d.square == Some(square)).map(|d| d.message.as_str()).collect()
}

#[test]
fn two_wires_into_a_data_input() {
    let l = lowered(
        &registry(),
        r#"return { lineit = 1, squares = {
            { id = 1, module = "flow/entry" },
            { id = 2, module = "math/add", fields = { a = "1", b = "2" } },
            { id = 3, module = "math/add", fields = { a = "3", b = "4" } },
            { id = 4, module = "math/add", fields = { b = "5" } },
            { id = 5, module = "io/print" },
        }, links = {
            { 1, "then", 5, "tick" }, { 4, "result", 5, "value" },
            { 2, "result", 4, "a" }, { 3, "result", 4, "a" },
        } }"#,
    );
    assert!(l.program.is_none());
    assert_eq!(about(&l, 4), ["`a` takes one wire, and has 2"]);
    assert!(about(&l, 5).is_empty() && about(&l, 2).is_empty());
}

#[test]
fn one_wire_is_fine() {
    let l = lowered(
        &registry(),
        r#"return { lineit = 1, squares = {
            { id = 1, module = "flow/entry" },
            { id = 2, module = "math/add", fields = { a = "1", b = "2" } },
            { id = 3, module = "io/print" },
        }, links = { { 1, "then", 3, "tick" }, { 2, "result", 3, "value" } } }"#,
    );
    assert!(l.diags.is_empty(), "{:?}", l.diags);
    assert!(l.program.is_some());
}

#[test]
fn many_wires_into_a_tick_input() {
    let l = lowered(
        &registry(),
        r#"return { lineit = 1, squares = {
            { id = 1, module = "flow/entry" },
            { id = 2, module = "flow/entry" },
            { id = 3, module = "io/print", fields = { value = "x" } },
        }, links = { { 1, "then", 3, "tick" }, { 2, "then", 3, "tick" } } }"#,
    );
    assert!(l.diags.is_empty(), "{:?}", l.diags);
    assert!(l.program.is_some());
}

#[test]
fn two_wires_into_a_subgraph_input() {
    // test/double has an input `x`
    let l = lowered(
        &registry(),
        r#"return { lineit = 1, squares = {
            { id = 1, module = "flow/entry" },
            { id = 2, module = "math/add", fields = { a = "1", b = "2" } },
            { id = 3, module = "math/add", fields = { a = "3", b = "4" } },
            { id = 4, module = "test/double" },
            { id = 5, module = "io/print" },
        }, links = {
            { 1, "then", 5, "tick" }, { 4, "y", 5, "value" },
            { 2, "result", 4, "x" }, { 3, "result", 4, "x" },
        } }"#,
    );
    assert!(l.program.is_none());
    assert_eq!(about(&l, 4), ["`x` takes one wire, and has 2"]);
}

#[test]
fn two_wires_inside_a_subgraph_are_on_its_square() {
    // a subgraph whose graph/output has two wires into it, and a data input with two
    let dir = std::env::temp_dir().join(format!("lineit-wires-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("twice.graph.lua"),
        r#"return { lineit = 1, squares = {
            { id = 1, module = "graph/input", params = { name = "x", type = "int" } },
            { id = 2, module = "math/add" },
            { id = 3, module = "graph/output", params = { name = "y", type = "int" } },
            { id = 4, module = "graph/input", params = { name = "z", type = "int" } },
        }, links = {
            { 1, "value", 2, "a" }, { 4, "value", 2, "a" }, { 1, "value", 2, "b" },
            { 2, "result", 3, "value" }, { 1, "value", 3, "value" },
        } }"#,
    )
    .unwrap();
    let mut reg = Registry::builtin();
    reg.load_dir(&dir, "");
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(reg.errors.is_empty(), "{:?}", reg.errors);
    let l = lowered(
        &reg,
        r#"return { lineit = 1, squares = {
            { id = 1, module = "flow/entry" },
            { id = 2, module = "twice" },
            { id = 3, module = "io/print" },
        }, links = { { 1, "then", 3, "tick" }, { 2, "y", 3, "value" } } }"#,
    );
    assert!(l.program.is_none());
    let mut msgs = about(&l, 2);
    msgs.sort();
    assert_eq!(msgs, ["`a` takes one wire, and has 2", "`y` takes one wire, and has 2"]);
}

#[test]
fn identical_wires_are_one_when_read() {
    let c = read_canvas(
        r#"return { lineit = 1, squares = { { id = 1, module = "math/add" }, { id = 2, module = "math/add" } },
        links = { { 1, "result", 2, "a" }, { 1, "result", 2, "a" } } }"#,
        "test",
    )
    .unwrap();
    assert_eq!(c.links.len(), 1);
    let l = lower::lower(&c, &registry());
    assert!(l.diags.is_empty(), "{:?}", l.diags);
    assert!(l.double.is_empty());
}

#[test]
fn wires_to_a_pin_that_isnt_there_are_left_out() {
    let l = lowered(
        &registry(),
        r#"return { lineit = 1, squares = {
            { id = 1, module = "flow/entry" },
            { id = 2, module = "math/add", fields = { a = "1", b = "2" } },
            { id = 3, module = "math/add", fields = { a = "3", b = "4" } },
            { id = 4, module = "io/print" },
        }, links = {
            { 1, "then", 4, "tick" }, { 2, "result", 4, "value" },
            { 2, "result", 4, "nope" }, { 3, "result", 4, "nope" },
        } }"#,
    );
    assert!(l.diags.is_empty(), "{:?}", l.diags);
}

#[test]
fn the_doubled_wires_are_listed() {
    let l = lowered(
        &registry(),
        r#"return { lineit = 1, squares = {
            { id = 1, module = "math/add", fields = { a = "1", b = "2" } },
            { id = 2, module = "math/add", fields = { a = "3", b = "4" } },
            { id = 3, module = "math/add" },
            { id = 4, module = "test/double" },
        }, links = {
            { 1, "result", 3, "a" }, { 2, "result", 3, "a" }, { 2, "result", 3, "b" },
            { 1, "result", 4, "x" }, { 2, "result", 4, "x" },
        } }"#,
    );
    let mut got: Vec<_> = l.double.iter().map(|l| (l.from, l.to, l.input.as_str())).collect();
    got.sort();
    assert_eq!(got, [(1, 3, "a"), (1, 4, "x"), (2, 3, "a"), (2, 4, "x")]);
}

/// A subgraph file in a registry of its own.
fn with_graph(name: &str, src: &str) -> Registry {
    let dir = std::env::temp_dir().join(format!("lineit-wires-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{name}.graph.lua")), src).unwrap();
    let mut reg = Registry::builtin();
    reg.load_dir(&dir, "");
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(reg.errors.is_empty(), "{:?}", reg.errors);
    reg
}

#[test]
fn a_subgraph_used_twice_reports_once() {
    // its graph/output has no name: the port is `out`
    let reg = with_graph(
        "twice",
        r#"return { lineit = 1, squares = {
            { id = 1, module = "graph/input", params = { name = "x", type = "int" } },
            { id = 2, module = "graph/input", params = { name = "z", type = "int" } },
            { id = 3, module = "graph/output" },
        }, links = { { 1, "value", 3, "value" }, { 2, "value", 3, "value" } } }"#,
    );
    let l = lowered(
        &reg,
        r#"return { lineit = 1, squares = {
            { id = 1, module = "twice" }, { id = 2, module = "twice" },
        }, links = {} }"#,
    );
    assert_eq!(about(&l, 1), ["`out` takes one wire, and has 2"]);
    assert_eq!(about(&l, 2), ["`out` takes one wire, and has 2"]);
    assert_eq!(l.diags.len(), 2);
}

#[test]
fn many_wires_into_a_tick_port() {
    let reg = with_graph(
        "ticks",
        r#"return { lineit = 1, squares = {
            { id = 1, module = "graph/input", params = { name = "go", type = "tick" } },
            { id = 2, module = "graph/output", params = { name = "done", type = "tick" } },
            { id = 3, module = "graph/input", params = { name = "go2", type = "tick" } },
        }, links = { { 1, "value", 2, "value" }, { 3, "value", 2, "value" } } }"#,
    );
    let l = lowered(
        &reg,
        r#"return { lineit = 1, squares = {
            { id = 1, module = "flow/entry" }, { id = 2, module = "flow/entry" },
            { id = 3, module = "ticks" },
            { id = 4, module = "io/print", fields = { value = "x" } },
        }, links = {
            { 1, "then", 3, "go" }, { 2, "then", 3, "go" }, { 3, "done", 4, "tick" },
        } }"#,
    );
    assert!(l.diags.is_empty(), "{:?}", l.diags);
}

//! A module of Rust code, in a process of its own (as lineit runs it), frees
//! the state of a square when asked: an edit while running that deletes a
//! square drops its state there, and the other's stays. A `Drop` that writes
//! to a file shows it. (`lineit-core/tests/hot_swap.rs` is the same with the
//! code in this process.)

use lineit_core::interp::{Interp, Status};
use lineit_core::lower::lower;
use lineit_core::registry::Registry;
use lineit_rt::Value;

/// The code of a square, whose state writes a line to LOG when it's dropped.
fn code(log: &std::path::Path) -> String {
    format!(
        "#[derive(Default)]\npub struct Probe {{\n    ticks: i64,\n}}\n\nimpl Drop for Probe {{\n    fn drop(&mut self) {{\n        use std::io::Write;\n        if let Ok(mut f) = std::fs::OpenOptions::new().append(true).create(true).open({:?}) {{\n            let _ = writeln!(f, \"x\");\n        }}\n    }}\n}}\n\nimpl Probe {{\n    pub fn counter(&mut self) -> i64 {{\n        self.ticks += 1;\n        self.ticks\n    }}\n}}\n",
        log.display().to_string()
    )
}

/// A clock that ticks the squares 2 and 3 (the second only if TWO).
fn canvas(log: &std::path::Path, two: bool) -> lineit_core::model::Canvas {
    let square = |id: u32| {
        format!(
            r#"{{ id = {id}, module = "code/function", params = {{ language = "rust", code = {} }} }},"#,
            lineit_core::file::quote(&code(log))
        )
    };
    let (second, link) = if two { (square(3), r#", { 1, "tick", 3, "tick" }"#) } else { (String::new(), "") };
    let src = format!(
        r#"return {{ lineit = 1, squares = {{ {{ id = 1, module = "flow/clock", fields = {{ interval = "100" }} }},
        {} {second} }}, links = {{ {{ 1, "tick", 2, "tick" }} {link} }} }}"#,
        square(2)
    );
    lineit_core::file::read_canvas(&src, "rust_free").unwrap()
}

fn program(c: &lineit_core::model::Canvas) -> lineit_core::ir::Program {
    let l = lower(c, &Registry::builtin());
    assert!(l.diags.is_empty(), "{:?}", l.diags);
    l.program.unwrap()
}

fn freed(log: &std::path::Path) -> usize {
    std::fs::read_to_string(log).map_or(0, |s| s.lines().count())
}

#[test]
fn a_rust_module_in_a_process_frees_the_state_of_a_square() {
    lineit_core::native::run_apart(env!("CARGO_BIN_EXE_lineit").into());
    let log = std::env::temp_dir().join(format!("lineit-rust-apart-{}", std::process::id()));
    let _ = std::fs::remove_file(&log);

    // the module: the state of one is dropped in the process, the other's isn't
    let p = program(&canvas(&log, true));
    let mut m = lineit_core::interp::open_ext(&p.exts[0].path, &p.exts[0].kind).unwrap();
    let (a, b) = (m.instance(&[]).unwrap(), m.instance(&[]).unwrap());
    let out = [("result", &lineit_rt::Type::Int)];
    m.eval(b, &[], &out).unwrap();
    assert_eq!(freed(&log), 0);
    m.drop_instance(a).unwrap();
    assert_eq!(freed(&log), 1, "the state of the one that was freed");
    assert!(m.drop_instance(a).is_err(), "and can't go twice");
    assert!(m.eval(a, &[], &out).is_err(), "a freed square");
    assert_eq!(m.eval(b, &[], &out).unwrap(), vec![Value::Int(2)], "the other one has its state");
    drop(m);
    let _ = std::fs::remove_file(&log);

    // an edit while running that deletes a square
    let mut it = Interp::new(p, std::env::temp_dir()).unwrap();
    it.start().unwrap();
    let tick = |it: &mut Interp| {
        it.tick_clocks(true).unwrap();
        while it.run(1 << 20).unwrap() != Status::Idle {}
    };
    let ticks = |it: &Interp, sq| it.w.code_states()[&sq].iter().find(|(n, _)| n == "ticks").map(|(_, v)| v.clone());
    tick(&mut it);
    tick(&mut it);
    assert_eq!(freed(&log), 0);
    it.swap(program(&canvas(&log, false)), false).unwrap();
    assert_eq!(freed(&log), 1, "square 3 is freed, in the process");
    tick(&mut it);
    assert_eq!(ticks(&it, 2), Some(Value::Int(3)), "square 2 kept its state");
    assert!(!it.w.code_states().contains_key(&3));
    it.swap(program(&canvas(&log, false)), false).unwrap();
    assert_eq!(freed(&log), 1, "and square 2 stays");
    let _ = std::fs::remove_file(&log);
}

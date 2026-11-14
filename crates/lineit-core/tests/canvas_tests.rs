//! The tests of a canvas: a run from each `test/case`, with what its
//! `test/expect`s found.

use std::path::Path;

fn canvas(src: &str) -> lineit_core::model::Canvas {
    let mut c = lineit_core::file::read_canvas(src, "testing").unwrap();
    c.dir = Some(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests"));
    c
}

#[test]
fn cases_pass_and_fail() {
    let src = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/testing.lua")).unwrap();
    let reg = lineit_core::registry::Registry::builtin();
    let r = lineit_core::codetest::run_cases(&canvas(&src), &reg);
    assert_eq!(r.iter().map(|c| (c.name.as_str(), c.ok(), c.checks.len())).collect::<Vec<_>>(), [("up to 3", true, 1), ("up to 10", true, 1)]);

    // a case that wants what it doesn't get
    let wrong = src.replace(r#"["::want"] = "55""#, r#"["::want"] = "50""#);
    assert_ne!(wrong, src);
    let r = lineit_core::codetest::run_cases(&canvas(&wrong), &reg);
    let bad = &r[1];
    assert!(!bad.ok());
    assert_eq!((bad.checks[0].label.as_str(), bad.checks[0].expected.text(), bad.checks[0].got.text()), ("the total", "50".into(), "55".into()));
    assert_eq!(bad.output, ["55"]);

    // a case whose run fails says why
    let broken = src.replace(r#"{ id = 15, module = "io/print""#, r#"{ id = 15, module = "code/function", params = { code = "function boom()\n  error('no')\nend\n" }"#)
        .replace(r#"{ 13, "then", 15, "tick" }, { 10, "value", 15, "value" },"#, r#"{ 13, "then", 15, "tick" },"#);
    assert_ne!(broken, src, "the print became a code square");
    let r = lineit_core::codetest::run_cases(&canvas(&broken), &reg);
    assert!(r[0].error.as_deref().is_some_and(|e| e.contains("no")), "{:?}", r[0].error);
}

#[test]
fn a_normal_run_doesnt_start_at_cases() {
    let src = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/testing.lua")).unwrap();
    let prog = lineit_core::lower::lower(&canvas(&src), &lineit_core::registry::Registry::builtin()).program.unwrap();
    // the entry's start only
    assert_eq!(prog.starts.targets.len(), 1);
}

#[test]
fn a_stuck_case_ends_at_the_limit() {
    // stuck in a code square's call (Python's), and in plain squares (a loop to a huge number)
    let src = r#"return { lineit = 1, squares = {
        { id = 1, module = "test/case", params = { name = "stuck in Python" } },
        { id = 2, module = "code/function", params = { language = "python", code = "def spin():\n    while True:\n        pass\n" } },
        { id = 3, module = "test/case", params = { name = "stuck in squares" } },
        { id = 4, module = "flow/loop", fields = { from = "0", to = "9000000000000" } },
        { id = 5, module = "math/add" },
    }, links = { { 1, "then", 2, "tick" }, { 3, "then", 4, "tick" } } }"#;
    let started = std::time::Instant::now();
    let r = lineit_core::codetest::run_cases_within(&canvas(src), &lineit_core::registry::Registry::builtin(), 1.0);
    for c in &r {
        assert!(c.error.as_deref().is_some_and(|e| e.contains("more than 1 second:")), "{}: {:?}", c.name, c.error);
    }
    assert!(started.elapsed().as_secs_f64() < 8.0, "{:?}", started.elapsed());
}

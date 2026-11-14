//! `lineit check`, `run` and `compile` refuse a canvas with two wires into a data input.

use std::process::Command;

const TWO: &str = r#"return { lineit = 1, squares = {
 { id = 1, module = "flow/entry" },
 { id = 2, module = "math/add", fields = { a = "1", b = "2" } },
 { id = 3, module = "math/add", fields = { a = "3", b = "4" } },
 { id = 4, module = "math/add", fields = { b = "5" } },
 { id = 5, module = "io/print" },
}, links = { { 1, "then", 5, "tick" }, { 4, "result", 5, "value" }, { 2, "result", 4, "a" }, { 3, "result", 4, "a" } } }
"#;

#[test]
fn check_run_and_compile_refuse_it() {
    let d = std::env::temp_dir().join(format!("lineit-cli-wires-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("two.lua"), TWO).unwrap();
    for cmd in ["check", "run", "compile"] {
        let o = Command::new(env!("CARGO_BIN_EXE_lineit")).args([cmd, "two.lua"]).current_dir(&d).output().unwrap();
        let err = String::from_utf8_lossy(&o.stderr);
        assert!(!o.status.success(), "{cmd}: {err}");
        assert!(err.contains("`a` takes one wire, and has 2"), "{cmd}: {err}");
    }
    // nothing was compiled
    assert!(!d.join(lineit_core::paths::exe("two")).exists());
    let _ = std::fs::remove_dir_all(&d);
}

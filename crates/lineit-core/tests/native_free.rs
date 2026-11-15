//! A module of C code (in this process) frees a square when asked, and the
//! others go on. (`lineit/tests/native_free.rs` is the same with the code in
//! a process of its own.)

use lineit_core::lower::lower;
use lineit_core::registry::Registry;

#[test]
fn a_native_module_frees_an_instance() {
    let code = "static int64_t ticks = 0;\n\nint64_t counter(void) {\n    return ++ticks;\n}\n";
    let src = format!(
        r#"return {{ lineit = 1, squares = {{
        {{ id = 1, module = "flow/clock", fields = {{ interval = "100" }} }},
        {{ id = 2, module = "code/function", params = {{ language = "c", code = {} }} }},
        {{ id = 3, module = "string/format", params = {{ format = "{{}}" }} }},
        {{ id = 4, module = "io/print" }},
        }}, links = {{ {{ 1, "tick", 2, "tick" }}, {{ 2, "result", 3, "1" }}, {{ 3, "text", 4, "value" }},
        {{ 2, "then", 4, "tick" }} }} }}"#,
        lineit_core::file::quote(code)
    );
    let canvas = lineit_core::file::read_canvas(&src, "native").unwrap();
    let l = lower(&canvas, &Registry::builtin());
    assert!(l.diags.is_empty(), "{:?}", l.diags);
    let p = l.program.unwrap();
    let mut m = lineit_core::interp::open_ext(&p.exts[0].path, &p.exts[0].kind).unwrap();
    let (a, b) = (m.instance(&[]).unwrap(), m.instance(&[]).unwrap());
    assert!(m.state(a).is_ok() && m.state(b).is_ok());
    m.drop_instance(a).unwrap();
    assert!(m.state(a).is_err(), "it's gone");
    assert!(m.drop_instance(a).is_err(), "and can't go twice");
    assert!(m.state(b).is_ok(), "the other one isn't");
    assert_eq!(m.instance(&[]).unwrap(), 2, "its number isn't used again");
}

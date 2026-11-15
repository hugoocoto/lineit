//! A C square whose function keeps a `static` of its own is refused, with
//! the line on the square: it'd have state lineit can't see, so it'd be a
//! pure function that runs only if something reads it (it was a square that
//! did nothing, with no diagnostics). `()` is no parameters, as in C23.

use lineit_core::lower::lower;
use lineit_core::registry::Registry;

fn lowered(code: &str) -> lineit_core::lower::Lowered {
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
    lower(&lineit_core::file::read_canvas(&src, "static").unwrap(), &Registry::builtin())
}

#[test]
fn a_static_inside_the_function_is_a_diagnostic() {
    let l = lowered("int64_t counter() { static int64_t n = 0; return ++n; }");
    assert!(l.program.is_none());
    assert!(format!("{:?}", l.diags).contains("`n` is `static` inside a function"), "{:?}", l.diags);
}

#[test]
fn empty_parentheses_are_no_parameters() {
    let l = lowered("static int64_t ticks = 0;\n\nint64_t counter() {\n    return ++ticks;\n}\n");
    assert!(l.diags.is_empty(), "{:?}", l.diags);
    assert_eq!(l.program.unwrap().exts.len(), 1);
}

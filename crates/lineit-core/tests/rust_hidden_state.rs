//! A Rust code square that keeps state in a `static` (a function that looks
//! stateless to lineit) is an error on the square, not a program that does
//! nothing.

use lineit_core::file::read_canvas;
use lineit_core::lower;
use lineit_core::registry::Registry;

fn lowered(code: &str) -> lower::Lowered {
    let src = format!(
        "return {{ lineit = 1, squares = {{ {{ id = 1, module = \"flow/entry\" }}, {{ id = 2, module = \"code/function\", params = {{ language = \"rust\", code = [[\n{code}]] }} }} }}, links = {{ {{ 1, \"then\", 2, \"tick\" }} }} }}"
    );
    lower::lower(&read_canvas(&src, "test").unwrap(), &Registry::builtin())
}

fn about(l: &lower::Lowered) -> Vec<&str> {
    l.diags.iter().filter(|d| d.square == Some(2)).map(|d| d.message.as_str()).collect()
}

#[test]
fn a_static_is_an_error() {
    let l = lowered(
        "use std::sync::atomic::{AtomicI64, Ordering::SeqCst};\npub fn counter() -> i64 {\n    static N: AtomicI64 = AtomicI64::new(0);\n    N.fetch_add(1, SeqCst) + 1\n}\n",
    );
    assert!(l.program.is_none());
    let d = about(&l);
    assert!(d.iter().any(|m| m.contains("line 3: `N` is a `static` that changes inside a function")), "{d:?}");

    let l = lowered("pub fn counter() -> i64 {\n    static mut N: i64 = 0;\n    unsafe { N += 1; N }\n}\n");
    assert!(l.program.is_none());
    assert!(about(&l).iter().any(|m| m.contains("line 2: `N`")), "{:?}", l.diags);
}

#[test]
fn a_read_only_static_in_a_function_is_an_error() {
    let l = lowered("pub fn f() -> i64 {\n    static NAMES: [&str; 1] = [\"a\"];\n    NAMES.len() as i64\n}\n");
    assert!(l.program.is_none());
    let d = about(&l);
    assert!(d.iter().any(|m| m.contains("line 2: `NAMES` is a `static` inside a function: use `const`")), "{d:?}");
}

#[test]
fn a_struct_and_a_const_are_fine() {
    let l = lowered(
        "#[derive(Default)]\npub struct Counter {\n    ticks: i64,\n}\n\nimpl Counter {\n    pub fn counter(&mut self) -> i64 {\n        const K: i64 = 2;\n        const NAMES: [&str; 1] = [\"a\"];\n        self.ticks += K + NAMES.len() as i64;\n        self.ticks\n    }\n}\n",
    );
    assert!(l.diags.is_empty(), "{:?}", l.diags);
    assert!(l.program.is_some());
}

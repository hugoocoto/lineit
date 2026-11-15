//! What a code square's process wrote just before it ended (to stderr, which
//! is kept: `capture_stderr`) isn't lost: it's read, from the pipe, by a
//! thread that may be behind when the program ends or a module goes away.

use lineit_core::interp::{Interp, Status};
use lineit_core::lower::lower;
use lineit_core::registry::Registry;

/// Enough lines, in one write, for a reader to be behind when it's done
/// (more than a pipe holds, so what's left in it is thousands of lines).
const N: usize = 20000;

/// A canvas whose entry runs a Python square that writes N lines to stderr.
fn spewing() -> lineit_core::model::Canvas {
    let code = format!(
        "import sys\n\ndef spew():\n    sys.stderr.write(\"\".join(\"e%d\\n\" % i for i in range({N})))\n    sys.stderr.flush()\n"
    );
    let src = format!(
        r#"return {{ lineit = 1, squares = {{
        {{ id = 1, module = "flow/entry" }},
        {{ id = 2, module = "code/function", params = {{ language = "python", code = {} }} }},
        }}, links = {{ {{ 1, "then", 2, "tick" }} }} }}"#,
        lineit_core::file::quote(&code)
    );
    lineit_core::file::read_canvas(&src, "spewing").unwrap()
}

fn program(c: &lineit_core::model::Canvas) -> lineit_core::ir::Program {
    let l = lower(c, &Registry::builtin());
    assert!(l.diags.is_empty(), "{:?}", l.diags);
    l.program.unwrap()
}

fn wanted() -> Vec<String> {
    (0..N).map(|i| format!("e{i}")).collect()
}

#[test]
fn what_a_process_writes_right_before_the_program_ends_is_shown() {
    lineit_rt::ext::capture_stderr();
    let mut shown = (Vec::new(), Vec::new());
    lineit_core::interp::run_split(program(&spewing()), std::env::temp_dir(), None, |(mut o, mut e)| {
        shown.0.append(&mut o);
        shown.1.append(&mut e);
    })
    .unwrap();
    assert_eq!((shown.0.len(), shown.1.len()), (0, N));
    assert_eq!(shown.1, wanted());
}

#[test]
fn what_a_module_writes_right_before_an_edit_removes_it_is_kept() {
    lineit_rt::ext::capture_stderr();
    let mut it = Interp::new(program(&spewing()), std::env::temp_dir()).unwrap();
    it.start().unwrap();
    while it.run(1 << 20).unwrap() != Status::Idle {}
    // the edit has no code square: its module goes away
    let src = r#"return { lineit = 1, squares = { { id = 1, module = "flow/entry" } }, links = {} }"#;
    let gone = lineit_core::file::read_canvas(src, "gone").unwrap();
    it.swap(program(&gone), false).unwrap();
    assert_eq!(it.w.take_output(), wanted());
}

#[test]
fn a_process_that_holds_the_pipe_open_doesnt_hold_the_end_forever() {
    lineit_rt::ext::capture_stderr();
    // a process it started keeps the pipe after the square's own is gone
    let code = "import subprocess, sys\n\ndef hold():\n    print(\"before\", file=sys.stderr, flush=True)\n    subprocess.Popen([\"sleep\", \"6\"])\n";
    let src = format!(
        r#"return {{ lineit = 1, squares = {{
        {{ id = 1, module = "flow/entry" }},
        {{ id = 2, module = "code/function", params = {{ language = "python", code = {} }} }},
        }}, links = {{ {{ 1, "then", 2, "tick" }} }} }}"#,
        lineit_core::file::quote(code)
    );
    let canvas = lineit_core::file::read_canvas(&src, "holding").unwrap();
    let started = std::time::Instant::now();
    let mut err = Vec::new();
    lineit_core::interp::run_split(program(&canvas), std::env::temp_dir(), None, |(_, mut e)| err.append(&mut e))
        .unwrap();
    assert_eq!(err, ["before"]);
    let took = started.elapsed();
    assert!(took < std::time::Duration::from_secs(5), "{took:?}");
}

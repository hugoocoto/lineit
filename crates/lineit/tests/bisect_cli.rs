//! `lineit bisect`: piped answers, `--bad`, `--answers`, skips, bad input, EOF.
// tests start the programs as a user does
#![allow(clippy::disallowed_methods)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn tests_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests")
}

/// `lineit bisect ARGS`, with INPUT on its standard input.
fn bisect(args: &[&str], input: &str) -> Output {
    let dir = tests_dir();
    let mut child = Command::new(env!("CARGO_BIN_EXE_lineit"))
        .arg("bisect")
        .args(args)
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // it may be done before reading it all
    let _ = child.stdin.take().unwrap().write_all(input.as_bytes());
    child.wait_with_output().unwrap()
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// The last question said: (square, run).
fn question(text: &str) -> (u64, usize) {
    let line = text.lines().rev().find(|l| l.starts_with("square ")).unwrap_or_else(|| panic!("no question in {text}"));
    let square = line.split(' ').nth(1).unwrap().parse().unwrap();
    let run = line.split("run ").nth(1).unwrap().split(' ').next().unwrap().parse().unwrap();
    (square, run)
}

/// Answers every question with ORACLE until it ends, one `--answers` at a time:
/// the output of the end, and the answers.
fn drive(file: &str, extra: &[&str], oracle: impl Fn(u64, usize) -> bool) -> (String, String) {
    let mut answers = String::new();
    for _ in 0..100 {
        let mut args = vec![file];
        args.extend_from_slice(extra);
        args.extend(["--answers", &answers]);
        let o = bisect(&args, "");
        assert!(o.status.success(), "{}", err(&o));
        let text = out(&o);
        if !text.contains("\nstopped") {
            return (text, answers);
        }
        let (sq, run) = question(&text);
        answers.push(if oracle(sq, run) { 'b' } else { 'g' });
    }
    panic!("it never ended: {answers}");
}

#[test]
fn finds_the_iteration_that_went_wrong() {
    // the 4th iteration adds 100: square 8 sets it. What's after it is bad
    let oracle = |sq: u64, run: usize| match sq {
        5 => run >= 5,
        10 => run >= 4,
        8 | 11 => true,
        _ => false,
    };
    let (text, answers) = drive("bisect.lua", &[], oracle);
    // what it printed first, then the end
    assert!(text.starts_with("the run printed:\n  117\n15 square runs recorded\n"), "{text}");
    assert!(text.contains("the bug is in square 8 (var/set), run 1 of 1\n"), "{text}");
    assert!(text.contains("  variables changed: total = 106\n"), "{text}");
    // 15 events: four questions at most
    assert!(answers.len() <= 4, "{answers}");
    assert!(text.contains(&format!("to replay: lineit bisect bisect.lua --answers {answers}\n")), "{text}");
    // the same answers piped to the questions, one a line
    let lines: String = answers.chars().map(|c| format!("{c}\n")).collect();
    let piped = bisect(&["bisect.lua"], &lines);
    assert!(piped.status.success());
    let t = out(&piped);
    assert!(t.contains("the bug is in square 8 (var/set), run 1 of 1\n"), "{t}");
    assert!(t.contains(&format!("--answers {answers}\n")), "{t}");
    // a question has the square, its run, the variables, and the prompt
    assert!(t.contains("about 4 questions left\n"), "{t}");
    assert!(t.contains("variables changed: total = "), "{t}");
    // none changed: all of them, to judge by
    assert!(t.contains("variables (none changed): total = "), "{t}");
    assert!(t.contains("good, bad, skip, undo or quit? [g/b/s/u/q] "), "{t}");
}

#[test]
fn bad_says_where_it_is_known_to_be_wrong() {
    // square 8 ran once: what's after it isn't a candidate, so fewer questions than from the end
    let o = bisect(&["bisect.lua", "--bad", "8"], "q\n");
    let t = out(&o);
    assert!(t.contains("about 4 questions left"), "{t}");
    assert!(t.contains("stopped\n"), "{t}");
    // a run of a square, from 1: what comes after the 3rd ifelse isn't asked about
    let o = bisect(&["bisect.lua", "--bad", "5:3", "--answers", "b"], "");
    let t = out(&o);
    assert!(o.status.success(), "{t}{}", err(&o));
    assert!(t.contains("--bad 5:3 --answers b"), "{t}");
    let o = bisect(&["bisect.lua", "--bad", "5:3"], "q\n");
    let t = out(&o);
    assert!(t.contains("about 3 questions left") && !t.contains("square 11 ("), "{t}");
    // the last run, alone
    let a = out(&bisect(&["bisect.lua", "--bad", "5"], ""));
    let b = out(&bisect(&["bisect.lua", "--bad", "5:6"], ""));
    assert_eq!(a.replace("--bad 5", ""), b.replace("--bad 5:6", ""));
}

#[test]
fn bad_that_is_wrong() {
    for (bad, want) in [
        ("99", "square 99 didn't run"),
        ("5:7", "square 5 ran 6 times (runs 1 to 6)"),
        ("5:0", "square 5 ran 6 times"),
        ("x", "want a square id"),
        ("5:x", "isn't a run number"),
    ] {
        let o = bisect(&["bisect.lua", "--bad", bad], "");
        assert!(!o.status.success(), "{bad}");
        assert!(err(&o).contains(want), "{bad}: {}", err(&o));
    }
}

#[test]
fn a_fan_out_is_not_a_line() {
    // three pushes in parallel, then the meet. Square 4's push is the bug: what's after it is bad,
    // and that 3 and 5 are good says nothing about it
    let oracle = |sq: u64, _| matches!(sq, 4 | 7 | 8 | 9);
    let (text, answers) = drive("branches.lua", &[], oracle);
    assert!(text.contains("the bug is in square 4 (list/push), run 1 of 1\n"), "{text}");
    assert!(text.contains("  variables changed: l = "), "{text}");
    assert!(answers.len() <= 3, "{answers}");
}

#[test]
fn all_good_blames_no_square() {
    let o = bisect(&["branches.lua"], &"g\n".repeat(20));
    let t = out(&o);
    assert!(o.status.success());
    assert!(t.contains("every point was good: no square that ran is to blame"), "{t}");
    assert!(t.contains("to replay: lineit bisect branches.lua --answers g"), "{t}");
}

#[test]
fn skipping_leaves_a_range() {
    let o = bisect(&["branches.lua"], &"s\n".repeat(20));
    let t = out(&o);
    assert!(o.status.success());
    assert!(t.contains("can't tell: the bug is in one of these 7"), "{t}");
    assert!(t.contains("  square 7 (io/print), run 1 of 1\n"), "{t}");
    assert!(t.contains("--answers sssssss\n"), "{t}");
}

#[test]
fn words_letters_undo_and_input_that_is_not_an_answer() {
    let o = bisect(&["branches.lua"], "u\nmaybe\n\nGOOD\nUndo\nbad\nquit\n");
    let t = out(&o);
    assert!(o.status.success(), "{}", err(&o));
    assert!(t.contains("nothing to undo\n"), "{t}");
    assert!(t.contains("`maybe`: not an answer\n"), "{t}");
    assert!(t.contains("``: not an answer\n"), "{t}");
    // good, undone, bad: one answer in the end
    assert!(t.contains("stopped; to go on: lineit bisect branches.lua --answers b\n"), "{t}");
    // the same question comes again after the undo
    assert_eq!(t.matches("about 3 questions left").count(), 3, "{t}");
}

#[test]
fn the_end_of_the_input_is_quitting() {
    let o = bisect(&["branches.lua"], "");
    assert!(o.status.success());
    assert!(out(&o).ends_with("[g/b/s/u/q] \nstopped\n"), "{}", out(&o));
    let o = bisect(&["branches.lua"], "g\ns");
    assert!(out(&o).contains("stopped; to go on: lineit bisect branches.lua --answers gs\n"), "{}", out(&o));
}

#[test]
fn answers_to_start_with() {
    // an unknown letter, one too many, an undo of nothing
    let o = bisect(&["branches.lua", "--answers", "gx"], "");
    assert!(!o.status.success());
    assert!(err(&o).contains("`x` isn't g, b, s or u"), "{}", err(&o));
    let o = bisect(&["branches.lua", "--answers", "ssssssss"], "");
    assert!(err(&o).contains("answer 8 (`s`): it was done already"), "{}", err(&o));
    let o = bisect(&["branches.lua", "--answers", "u"], "");
    assert!(err(&o).contains("answer 1 (`u`): nothing to undo"), "{}", err(&o));
    // an undo counts, and spaces don't
    let o = bisect(&["branches.lua", "--answers", "g s u"], "q\n");
    assert!(out(&o).contains("--answers g\n"), "{}", out(&o));
}

#[test]
fn a_run_that_ends_in_an_error_says_so() {
    let dir = std::env::temp_dir().join(format!("lineit-bisect-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("boom.lua");
    std::fs::write(
        &file,
        r#"return { lineit = 1, squares = {
            { id = 1, module = "flow/entry" },
            { id = 2, module = "io/print", fields = { value = "before" } },
            { id = 3, module = "code/function", params = { code = "function boom()\n  error('no')\nend\n" } },
        }, links = { { 1, "then", 2, "tick" }, { 2, "then", 3, "tick" } } }"#,
    )
    .unwrap();
    let o = bisect(&[file.to_str().unwrap()], "b\nb\nb\n");
    let t = out(&o);
    assert!(o.status.success(), "{}", err(&o));
    assert!(t.starts_with("the run printed:\n  before\nthe run ended with an error: "), "{t}");
    assert!(t.contains("no"), "{t}");
    // the error is in the last square that ran
    assert!(t.contains("the bug is in square 1 (flow/entry), run 1 of 1\n"), "{t}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_limit_is_said() {
    let o = bisect(&["branches.lua", "--limit", "3"], "q\n");
    let t = out(&o);
    assert!(t.contains("3 square runs recorded\nthe run was cut at 3 events"), "{t}");
    assert!(!out(&bisect(&["branches.lua"], "q\n")).contains("cut"));
    let o = bisect(&["branches.lua", "--limit", "many"], "");
    assert!(err(&o).contains("--limit many: not a number"), "{}", err(&o));
    // the limit is in the line that replays
    let o = bisect(&["branches.lua", "--limit", "3"], "g\nq\n");
    assert!(out(&o).contains("lineit bisect branches.lua --limit 3 --answers g\n"), "{}", out(&o));
}

#[test]
fn a_canvas_that_does_not_check_fails_as_run_does() {
    let dir = std::env::temp_dir().join(format!("lineit-bisect-bad-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("bad.lua");
    std::fs::write(&file, r#"return { lineit = 1, squares = { { id = 1, module = "no/such" } }, links = {} }"#)
        .unwrap();
    let f = file.to_str().unwrap();
    let b = bisect(&[f], "");
    let r = Command::new(env!("CARGO_BIN_EXE_lineit")).args(["run", f]).output().unwrap();
    assert!(!b.status.success());
    assert_eq!(b.status.code(), r.status.code());
    assert_eq!(err(&b), err(&r));
    assert!(err(&b).contains("it can't run:"), "{}", err(&b));
    assert!(out(&b).is_empty());
    // other mistakes
    for (args, want) in [(&["--nope"][..], "--nope: no such option"), (&[][..], "which file?")] {
        let o = bisect(args, "");
        assert!(!o.status.success());
        assert!(err(&o).contains(want), "{}", err(&o));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

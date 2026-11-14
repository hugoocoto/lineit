//! `lineit diff` and `lineit text`: output and exit codes, and with git.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const A: &str = r#"return { lineit = 1, squares = {
 { id = 1, module = "flow/entry", x = 0, y = 0 },
 { id = 2, module = "io/print", x = 100, y = 0, fields = { value = "hi" } },
 { id = 3, module = "note", x = 0, y = 100, params = { text = "a\nb\nc\nd\ne\nf\ng\nh" } },
}, links = { { 1, "then", 2, "tick" } } }
"#;
// B: a moved, a field changed, a note line changed, a camera
const B: &str = r#"return { lineit = 1, camera = { x = 1, y = 1, zoom = 2 }, squares = {
 { id = 1, module = "flow/entry", x = 50, y = 0 },
 { id = 2, module = "io/print", x = 100, y = 0, fields = { value = "bye" } },
 { id = 3, module = "note", x = 0, y = 100, params = { text = "a\nb\nc\nd\ne\nf\nG\nh" } },
}, links = { { 1, "then", 2, "tick" } } }
"#;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("lineit-cli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn lineit(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lineit")).args(args).current_dir(cwd).output().unwrap()
}

fn text(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
fn diff_output_and_exit_codes() {
    let d = dir("exit");
    std::fs::write(d.join("a.lua"), A).unwrap();
    std::fs::write(d.join("b.lua"), B).unwrap();
    let o = lineit(&d, &["diff", "a.lua", "b.lua"]);
    assert_eq!(o.status.code(), Some(1));
    let t = text(&o);
    assert!(t.contains("changed square 2 io/print\n    field value: \"hi\" -> \"bye\""), "{t}");
    assert!(t.contains("      - g\n      + G\n"), "{t}");
    assert!(t.contains("moved only (1): 1 flow/entry"), "{t}");
    assert_eq!(lineit(&d, &["diff", "a.lua", "a.lua"]).status.code(), Some(0));
    assert_eq!(lineit(&d, &["diff", "a.lua", "nope.lua"]).status.code(), Some(2));
    assert_eq!(lineit(&d, &["diff", "a.lua"]).status.code(), Some(2));
    // moved only: the program is the same
    let mut moved = A.replace("x = 0, y = 0", "x = 9, y = 9");
    moved.push('\n');
    std::fs::write(d.join("m.lua"), moved).unwrap();
    let o = lineit(&d, &["diff", "a.lua", "m.lua"]);
    assert_eq!(o.status.code(), Some(0));
    assert!(text(&o).contains("moved only"));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn subgraph_says_where_it_is_used() {
    let d = dir("sub");
    std::fs::create_dir_all(d.join("modules")).unwrap();
    std::fs::write(
        d.join("modules/g.graph.lua"),
        "return { lineit = 1, squares = { { id = 1, module = 'graph/input' } } }",
    )
    .unwrap();
    std::fs::write(
        d.join("modules/h.graph.lua"),
        "return { lineit = 1, squares = { { id = 1, module = 'graph/output' } } }",
    )
    .unwrap();
    std::fs::write(
        d.join("main.lua"),
        r#"return { lineit = 1, squares = { { id = 4, module = "g" }, { id = 9, module = "g" } } }"#,
    )
    .unwrap();
    let o = lineit(&d, &["diff", "modules/g.graph.lua", "modules/h.graph.lua"]);
    assert_eq!(o.status.code(), Some(1));
    let t = text(&o);
    // the new file is h, which nothing uses
    assert!(t.contains("subgraph h: not used"), "{t}");
    let o = lineit(&d, &["diff", "modules/h.graph.lua", "modules/g.graph.lua"]);
    let t = text(&o);
    assert!(t.contains("subgraph g is used in:") && t.contains("main.lua: squares 4, 9"), "{t}");
    let _ = std::fs::remove_dir_all(&d);
}

fn git(cwd: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(cwd)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[test]
fn with_git() {
    let d = dir("git");
    if !git(&d, &["init", "-q"]) {
        eprintln!("no git: skipped");
        return;
    }
    std::fs::write(d.join("c.lua"), A).unwrap();
    assert!(git(&d, &["add", "c.lua"]));
    assert!(git(&d, &["commit", "-q", "-m", "a"]));
    std::fs::write(d.join("c.lua"), B).unwrap();
    // REV:path against the working tree
    let o = lineit(&d, &["diff", "HEAD:c.lua", "c.lua"]);
    assert_eq!(o.status.code(), Some(1), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(text(&o).contains("field value"));
    // a textconv driver: git diff prints the lines of `lineit text`
    std::fs::write(d.join(".gitattributes"), "*.lua diff=lineit\n").unwrap();
    let exe = env!("CARGO_BIN_EXE_lineit").replace('\\', "/");
    assert!(git(&d, &["config", "diff.lineit.textconv", &format!("{exe} text")]));
    let o = Command::new("git").args(["diff", "--no-color", "--no-ext-diff"]).current_dir(&d).output().unwrap();
    let t = text(&o);
    assert!(t.contains("-    field value = \"hi\"\n+    field value = \"bye\""), "{t}");
    assert!(!t.contains("camera") && !t.contains("x = 50"), "{t}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn rev_paths_from_a_subfolder() {
    let d = dir("sub-git");
    if !git(&d, &["init", "-q"]) {
        eprintln!("no git: skipped");
        return;
    }
    let sub = d.join("sub");
    std::fs::create_dir_all(sub.join("modules")).unwrap();
    let g = |m: &str| format!("return {{ lineit = 1, squares = {{ {{ id = 1, module = '{m}' }} }} }}");
    std::fs::write(sub.join("modules/g.graph.lua"), g("graph/input")).unwrap();
    std::fs::write(sub.join("main.lua"), r#"return { lineit = 1, squares = { { id = 4, module = "g" } } }"#).unwrap();
    assert!(git(&d, &["add", "."]));
    assert!(git(&d, &["commit", "-q", "-m", "a"]));
    std::fs::write(sub.join("modules/g.graph.lua"), g("graph/output")).unwrap();
    // from sub/: relative to the root of the repository, and with ./ relative to here
    for old in ["HEAD:sub/modules/g.graph.lua", "HEAD:./modules/g.graph.lua"] {
        let o = lineit(&sub, &["diff", old, "modules/g.graph.lua"]);
        assert_eq!(o.status.code(), Some(1), "{old}: {}", String::from_utf8_lossy(&o.stderr));
        let t = text(&o);
        assert!(t.contains("subgraph g is used in:") && t.contains("main.lua: square 4"), "{old}: {t}");
    }
    // the old one alone says where it is used too (the new one isn't a subgraph file)
    std::fs::copy(sub.join("modules/g.graph.lua"), sub.join("new.lua")).unwrap();
    let o = lineit(&sub, &["diff", "HEAD:sub/modules/g.graph.lua", "new.lua"]);
    assert!(text(&o).contains("main.lua: square 4"), "{}", text(&o));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn bad_options_are_errors() {
    let d = dir("flags");
    std::fs::write(d.join("a.lua"), A).unwrap();
    for args in [
        &["diff", "a.lua", "a.lua", "--nope"][..],
        &["diff", "a.lua", "-"],
        &["show", "-x", "a.lua"],
        &["text", "--x", "a.lua"],
    ] {
        let o = lineit(&d, args);
        assert_eq!(o.status.code(), Some(if args[0] == "diff" { 2 } else { 1 }), "{args:?}");
        assert!(String::from_utf8_lossy(&o.stderr).contains("no such option"), "{args:?}");
    }
    let _ = std::fs::remove_dir_all(&d);
}

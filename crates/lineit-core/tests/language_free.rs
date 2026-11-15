//! A language added with a file (`examples/languages/javascript.lua`, run by
//! node) frees a square when an edit while running deletes it, if its host
//! says (`{"drop": true}`) it can: it keeps the other's state. A host that
//! doesn't say it isn't asked. The hosts here are that file's, with a line
//! written to a log for each `drop` they get.

use lineit_core::interp::{Interp, Status};
use lineit_core::lower::lower;
use lineit_core::registry::Registry;
use lineit_rt::Value;

const CODE: &str = "var count = 0;\n\n/**\n * @returns {int}\n */\nexport function counter() {\n    count += 1;\n    return count;\n}\n";

fn node() -> bool {
    lineit_rt::ext::command("node").arg("--version").output().is_ok_and(|o| o.status.success())
}

/// The example's text as the language NAME, whose host writes `drop N` to
/// LOG when it's asked to free square N. OLD: it doesn't say it can.
fn language(name: &str, log: &std::path::Path, old: bool) -> String {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let text = std::fs::read_to_string(root.join("examples/languages/javascript.lua")).unwrap();
    let swaps: [(&str, String); 4] = [
        ("name = \"javascript\",", format!("name = \"{name}\",")),
        ("commands = { \"node\" },", format!("commands = {{ \"node\" }},\n        env = {{ DROPLOG = {:?} }},", log)),
        (
            "squares[req.sq] = null;",
            "squares[req.sq] = null; require(\"fs\").appendFileSync(process.env.DROPLOG, \"drop \" + req.sq + \"\\n\");"
                .to_string(),
        ),
        ("answer({ drop: true });", if old { "answer({});" } else { "answer({ drop: true });" }.to_string()),
    ];
    swaps.iter().fold(text, |t, (from, to)| {
        assert_eq!(t.matches(from).count(), 1, "the example changed: {from}");
        t.replace(from, to)
    })
}

/// A clock that ticks the squares 2 and 3 of LANGUAGE (the second only if TWO).
fn canvas(language: &str, two: bool) -> lineit_core::model::Canvas {
    let square = |id: u32| {
        format!(
            r#"{{ id = {id}, module = "code/function", params = {{ language = "{language}", code = {} }} }},"#,
            lineit_core::file::quote(CODE)
        )
    };
    let (second, link) = if two { (square(3), r#", { 1, "tick", 3, "tick" }"#) } else { (String::new(), "") };
    let src = format!(
        r#"return {{ lineit = 1, squares = {{ {{ id = 1, module = "flow/clock", fields = {{ interval = "100" }} }},
        {} {second} }}, links = {{ {{ 1, "tick", 2, "tick" }} {link} }} }}"#,
        square(2)
    );
    lineit_core::file::read_canvas(&src, "language_free").unwrap()
}

fn program(c: &lineit_core::model::Canvas) -> lineit_core::ir::Program {
    let l = lower(c, &Registry::builtin());
    assert!(l.diags.is_empty(), "{:?}", l.diags);
    l.program.unwrap()
}

fn tick(it: &mut Interp) {
    it.tick_clocks(true).unwrap();
    while it.run(1 << 20).unwrap() != Status::Idle {}
}

fn count(it: &Interp, sq: u64) -> Option<Value> {
    it.w.code_states().get(&sq)?.iter().find(|(n, _)| n == "count").map(|(_, v)| v.clone())
}

/// What the host was asked to free.
fn dropped(log: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(log).map(|s| s.lines().map(String::from).collect()).unwrap_or_default()
}

#[test]
fn a_language_from_a_file_frees_a_square_if_its_host_says_it_can() {
    if !node() {
        eprintln!("skipped: no node");
        return;
    }
    let dir = std::env::temp_dir().join(format!("lineit-language-free-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("languages")).unwrap();
    let (new_log, old_log) = (dir.join("new.log"), dir.join("old.log"));
    std::fs::write(dir.join("languages/jsdrops.lua"), language("jsdrops", &new_log, false)).unwrap();
    std::fs::write(dir.join("languages/jsold.lua"), language("jsold", &old_log, true)).unwrap();
    let errors = lineit_core::registry::code::file::load_dirs(&[dir.join("languages").as_path()]);
    assert!(errors.is_empty(), "{errors:?}");

    // the host that says it can: a module frees one square, and the other goes on
    let p = program(&canvas("jsdrops", true));
    // the host numbers the squares as they're made: square 3's number is its place here
    let third = p.ext_instances.iter().position(|i| i.square == 3).unwrap();
    let mut m = lineit_core::interp::open_ext(&p.exts[0].path, &p.exts[0].kind).unwrap();
    let (a, b) = (m.instance(&[]).unwrap(), m.instance(&[]).unwrap());
    let out = [("result", &lineit_rt::Type::Int)];
    m.eval(b, &[], &out).unwrap();
    m.drop_instance(a).unwrap();
    assert_eq!(dropped(&new_log), vec![format!("drop {a}")], "the host was asked to free that one");
    assert!(m.state(a).is_err(), "it's gone");
    assert!(
        m.state(b).is_ok_and(|s| s.iter().any(|(n, v)| n == "count" && *v == Value::Int(1))),
        "and the other has its state"
    );
    drop(m);
    let _ = std::fs::remove_file(&new_log);

    // an edit while running that deletes square 3 frees that one, and square 2 keeps its state
    let mut it = Interp::new(p, std::env::temp_dir()).unwrap();
    it.start().unwrap();
    tick(&mut it);
    tick(&mut it);
    assert_eq!((count(&it, 2), count(&it, 3)), (Some(Value::Int(2)), Some(Value::Int(2))));
    it.swap(program(&canvas("jsdrops", true)), false).unwrap();
    assert!(dropped(&new_log).is_empty(), "both stay");
    it.swap(program(&canvas("jsdrops", false)), false).unwrap();
    assert_eq!(dropped(&new_log), vec![format!("drop {third}")], "square 3's");
    tick(&mut it);
    assert_eq!(count(&it, 2), Some(Value::Int(3)), "square 2 kept its state");
    assert_eq!(count(&it, 3), None);
    it.swap(program(&canvas("jsdrops", false)), false).unwrap();
    assert_eq!(dropped(&new_log).len(), 1, "and square 2 stays");
    drop(it);

    // a host that doesn't say it can isn't asked, and the others go on
    let mut it = Interp::new(program(&canvas("jsold", true)), std::env::temp_dir()).unwrap();
    it.start().unwrap();
    tick(&mut it);
    it.swap(program(&canvas("jsold", false)), false).unwrap();
    tick(&mut it);
    assert_eq!(count(&it, 2), Some(Value::Int(2)), "square 2 kept its state");
    assert!(dropped(&old_log).is_empty(), "no drop message: {:?}", dropped(&old_log));
    drop(it);
    let _ = std::fs::remove_dir_all(&dir);
}

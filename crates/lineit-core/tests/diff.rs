//! The diff of two canvases: by id, the program only.

use lineit_core::diff::{self, Mark, Tag};
use lineit_core::file::read_canvas;
use lineit_core::model::{Canvas, Link, Square};

fn base() -> Canvas {
    let mut c = Canvas::default();
    c.add(Square::new(1, "flow/entry", 0.0, 0.0));
    c.add(Square::new(2, "io/print", 100.0, 0.0).with_field("value", "hi"));
    c.add(Square::new(3, "code/function", 0.0, 100.0).with_param("code", "a\nb\nc\nd\ne\nf\ng\nh\n"));
    c.connect(Link::new(1, "then", 2, "tick"), false);
    c
}

#[test]
fn same_program_is_empty_whatever_the_camera_and_positions() {
    let a = base();
    let mut b = base();
    b.camera.zoom = 3.0;
    b.no_camera = true;
    b.square_mut(2).unwrap().x = 500.0;
    b.squares.reverse();
    let d = diff::diff(&a, &b);
    assert!(d.is_empty());
    assert_eq!(d.moved.len(), 1);
    assert_eq!(d.moved[0].id, 2);
}

#[test]
fn squares_and_wires() {
    let a = base();
    let mut b = base();
    b.remove(2);
    b.add(Square::new(4, "io/print", 0.0, 0.0));
    b.connect(Link::new(1, "then", 4, "tick"), false);
    let d = diff::diff(&a, &b);
    assert_eq!(d.removed.iter().map(|s| s.id).collect::<Vec<_>>(), [2]);
    assert_eq!(d.added.iter().map(|s| s.id).collect::<Vec<_>>(), [4]);
    assert_eq!(d.links_removed, [Link::new(1, "then", 2, "tick")]);
    assert_eq!(d.links_added, [Link::new(1, "then", 4, "tick")]);
    assert!(d.changed.is_empty());
}

#[test]
fn changed_module_fields_and_params() {
    let a = base();
    let mut b = base();
    {
        let s = b.square_mut(2).unwrap();
        s.module = "io/println".into();
        s.fields.insert("value".into(), "bye".into());
        s.fields.insert("extra".into(), "1".into());
        s.params.insert("p".into(), "x".into());
        s.x = 9.0; // a change and a move: a change
    }
    let d = diff::diff(&a, &b);
    assert!(d.moved.is_empty());
    let c = &d.changed[0];
    assert!(c.module_changed());
    assert_eq!((c.old_module.as_str(), c.new_module.as_str()), ("io/print", "io/println"));
    assert_eq!(c.fields.len(), 2);
    assert_eq!(c.fields[1].key, "value");
    assert_eq!(c.fields[1].old.as_deref(), Some("hi"));
    assert_eq!(c.fields[1].new.as_deref(), Some("bye"));
    assert_eq!(c.fields[0].old, None);
    assert_eq!(c.params[0].new.as_deref(), Some("x"));
}

#[test]
fn files_round_trip_to_the_same_diff() {
    let a = base();
    let text = lineit_core::file::write_canvas(&a);
    let b = read_canvas(&text, "t").unwrap();
    assert!(diff::diff(&a, &b).is_empty());
}

#[test]
fn line_diffs_with_context() {
    let h = diff::lines("a\nb\nc\nd\ne\nf\ng\nh\n", "a\nB\nc\nd\ne\nf\ng\nH\n", 1);
    assert_eq!(h.len(), 2);
    assert_eq!(
        h[0],
        [(Tag::Same, "a".into()), (Tag::Removed, "b".into()), (Tag::Added, "B".into()), (Tag::Same, "c".into())]
    );
    assert_eq!(h[1].last(), Some(&(Tag::Added, "H".to_string())));
    assert!(diff::lines("x\n", "x\n", 2).is_empty());
}

#[test]
fn merged_marks_and_orders_changes() {
    let a = base();
    let mut b = base();
    b.remove(1);
    b.add(Square::new(5, "flow/entry", 50.0, 0.0));
    b.square_mut(3).unwrap().params.insert("code".into(), "z".into());
    b.square_mut(2).unwrap().y = 40.0;
    let m = diff::merge(&a, &b);
    assert_eq!(m.squares[&1], Mark::Removed);
    assert_eq!(m.squares[&5], Mark::Added);
    assert_eq!(m.squares[&3], Mark::Changed);
    assert!(m.canvas.square(1).is_some());
    assert!(m.links_removed.contains(&Link::new(1, "then", 2, "tick")));
    // 2 only moved, but its wire went: it's a change too, and not dimmed
    assert_eq!(m.squares[&2], Mark::Wired);
    assert_eq!(m.changes.iter().copied().collect::<Vec<_>>(), [1, 2, 3, 5]);
}

#[test]
fn outline_has_no_positions() {
    let a = base();
    let mut b = base();
    b.square_mut(1).unwrap().x = 77.0;
    assert_eq!(diff::outline(&a), diff::outline(&b));
    let t = diff::outline(&a);
    assert!(t.contains("square 2 io/print\n    field value = \"hi\"\n"), "{t}");
    assert!(t.contains("        | a\n"));
    assert!(t.contains("wire 1.then -> 2.tick"));
}

#[test]
fn subgraph_uses() {
    let dir = std::env::temp_dir().join(format!("lineit-diff-uses-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("modules/sub")).unwrap();
    let g = dir.join("modules/sub/twice.graph.lua");
    std::fs::write(&g, "return { lineit = 1 }").unwrap();
    let mut c = Canvas::default();
    c.add(Square::new(1, "flow/entry", 0.0, 0.0));
    c.add(Square::new(2, "sub/twice", 0.0, 0.0));
    c.add(Square::new(7, "sub/twice", 0.0, 0.0));
    std::fs::write(dir.join("main.lua"), lineit_core::file::write_canvas(&c)).unwrap();
    std::fs::write(dir.join("other.lua"), lineit_core::file::write_canvas(&base())).unwrap();
    let (name, root) = diff::subgraph_of(&g).unwrap();
    assert_eq!(name, "sub/twice");
    assert_eq!(root, dir);
    let u = diff::uses(&root, &name);
    assert_eq!(u.len(), 1);
    assert_eq!(u[0].file, dir.join("main.lua"));
    assert_eq!(u[0].squares, [2, 7]);
    assert!(diff::subgraph_of(&dir.join("main.lua")).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn moved_only_stays_dimmed() {
    let a = base();
    let mut b = base();
    b.square_mut(3).unwrap().x = 5.0;
    let m = diff::merge(&a, &b);
    assert_eq!(m.squares[&3], Mark::Moved);
    assert!(m.changes.is_empty());
}

#[test]
fn what_differs_shows_in_both_front_ends() {
    // a last newline and a CRLF are differences: the diff says so, and so does the text
    let mk = |code: &str| {
        let mut c = Canvas::default();
        c.add(Square::new(1, "code/function", 0.0, 0.0).with_param("code", code));
        c
    };
    for (x, y) in [("a\nb", "a\nb\n"), ("a\nb\n", "a\r\nb\n")] {
        let (a, b) = (mk(x), mk(y));
        assert!(!diff::diff(&a, &b).is_empty());
        assert_ne!(diff::outline(&a), diff::outline(&b), "{x:?} {y:?}");
        let h = diff::lines(x, y, 1);
        let shown: Vec<_> = h.iter().flatten().collect();
        assert!(shown.iter().any(|(t, _)| *t != Tag::Same));
        assert!(shown.iter().any(|(_, l)| l.contains("\\r") || l.contains("no newline")), "{shown:?}");
    }
}

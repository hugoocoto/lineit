//! What changed between two canvases, by square id: the program only
//! (squares, wires, fields, params), not where squares are drawn or where
//! the camera is. A square that only moved is listed apart.
//!
//! Matching by id is what makes a diff readable, and the editor never
//! renumbers a square it keeps: paste, duplicate and cut-and-paste give new
//! squares new ids (so a pasted square is an added one), and an id is only
//! reused when the square with the highest id is deleted and another added
//! (that shows as a changed module, not as a removal and an addition).

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use similar::{ChangeTag, TextDiff};

use crate::model::{Canvas, Link, Square, SquareId};

/// A field or a param that changed: `None` is "not there".
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub key: String,
    pub old: Option<String>,
    pub new: Option<String>,
}

/// A square in both canvases that isn't the same.
#[derive(Clone, Debug, PartialEq)]
pub struct Changed {
    pub id: SquareId,
    /// Its module before and after (the same if only its fields or params changed).
    pub old_module: String,
    pub new_module: String,
    pub fields: Vec<Entry>,
    pub params: Vec<Entry>,
}

impl Changed {
    pub fn module_changed(&self) -> bool {
        self.old_module != self.new_module
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Diff {
    pub added: Vec<Square>,
    pub removed: Vec<Square>,
    pub changed: Vec<Changed>,
    /// Squares that are the same but for their position: (the square, new).
    pub moved: Vec<Square>,
    pub links_added: Vec<Link>,
    pub links_removed: Vec<Link>,
}

impl Diff {
    /// Whether the programs are the same (positions don't count).
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.removed.is_empty()
            && self.changed.is_empty()
            && self.links_added.is_empty()
            && self.links_removed.is_empty()
    }
}

fn entries(old: &BTreeMap<String, String>, new: &BTreeMap<String, String>) -> Vec<Entry> {
    let keys: BTreeSet<&String> = old.keys().chain(new.keys()).collect();
    keys.into_iter()
        .filter(|k| old.get(*k) != new.get(*k))
        .map(|k| Entry { key: k.clone(), old: old.get(k).cloned(), new: new.get(k).cloned() })
        .collect()
}

/// What changed from OLD to NEW. Everything is in the order of the ids.
pub fn diff(old: &Canvas, new: &Canvas) -> Diff {
    let mut d = Diff::default();
    let mut olds: Vec<&Square> = old.squares.iter().collect();
    olds.sort_by_key(|s| s.id);
    let mut news: Vec<&Square> = new.squares.iter().collect();
    news.sort_by_key(|s| s.id);
    for o in &olds {
        match new.square(o.id) {
            None => d.removed.push((*o).clone()),
            Some(n) => {
                let fields = entries(&o.fields, &n.fields);
                let params = entries(&o.params, &n.params);
                if o.module != n.module || !fields.is_empty() || !params.is_empty() {
                    d.changed.push(Changed {
                        id: o.id,
                        old_module: o.module.clone(),
                        new_module: n.module.clone(),
                        fields,
                        params,
                    });
                } else if o.x != n.x || o.y != n.y {
                    d.moved.push(n.clone());
                }
            }
        }
    }
    d.added = news.iter().filter(|n| old.square(n.id).is_none()).map(|n| (*n).clone()).collect();
    let (ol, nl): (HashSet<&Link>, HashSet<&Link>) = (old.links.iter().collect(), new.links.iter().collect());
    d.links_removed = old.links.iter().filter(|l| !nl.contains(l)).cloned().collect();
    d.links_added = new.links.iter().filter(|l| !ol.contains(l)).cloned().collect();
    d.links_removed.sort();
    d.links_added.sort();
    d
}

/// How a line of a text changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    Same,
    Removed,
    Added,
}

/// The line changes from OLD to NEW, in groups of changes with CONTEXT lines
/// around them (two groups are apart if more lines than that are between).
pub fn lines(old: &str, new: &str, context: usize) -> Vec<Vec<(Tag, String)>> {
    let d = TextDiff::from_lines(old, new);
    d.grouped_ops(context)
        .iter()
        .map(|g| {
            g.iter()
                .flat_map(|op| d.iter_changes(op))
                .map(|c| {
                    let tag = match c.tag() {
                        ChangeTag::Equal => Tag::Same,
                        ChangeTag::Delete => Tag::Removed,
                        ChangeTag::Insert => Tag::Added,
                    };
                    // what differs has to show: a CRLF, or a last line with no newline
                    let mut line = c.value().strip_suffix('\n').unwrap_or(c.value()).replace('\r', "\\r");
                    if c.missing_newline() {
                        line.push_str(" (no newline at end)");
                    }
                    (tag, line)
                })
                .collect()
        })
        .collect()
}

/// What a square is in a diff.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Added,
    Removed,
    Changed,
    /// Only moved: drawn dimmed.
    Moved,
    /// The same square, but a wire to or from it was added or removed (even if it also moved).
    Wired,
}

/// OLD and NEW on one canvas, for the editor: the squares of NEW (where
/// they are in NEW), the squares and wires only OLD has, and what each is.
pub struct Merged {
    pub canvas: Canvas,
    pub squares: BTreeMap<SquareId, Mark>,
    pub links_added: HashSet<Link>,
    pub links_removed: HashSet<Link>,
    /// The squares the editor visits, one change to the next: everything marked
    /// but the squares that only moved.
    pub changes: BTreeSet<SquareId>,
}

pub fn merge(old: &Canvas, new: &Canvas) -> Merged {
    let d = diff(old, new);
    let mut canvas = new.clone();
    let mut squares = BTreeMap::new();
    for s in &d.moved {
        squares.insert(s.id, Mark::Moved);
    }
    for s in &d.added {
        squares.insert(s.id, Mark::Added);
    }
    for c in &d.changed {
        squares.insert(c.id, Mark::Changed);
    }
    for s in &d.removed {
        squares.insert(s.id, Mark::Removed);
        canvas.squares.push(s.clone());
    }
    canvas.links.extend(d.links_removed.iter().cloned());
    // a wire that changed is a change at both its squares, if they have no mark of their own
    // (a square that only moved loses its dimming: its wires are what's new)
    for l in d.links_added.iter().chain(&d.links_removed) {
        for id in [l.from, l.to] {
            let m = squares.entry(id).or_insert(Mark::Wired);
            if *m == Mark::Moved {
                *m = Mark::Wired;
            }
        }
    }
    let changes = squares.iter().filter(|(_, m)| **m != Mark::Moved).map(|(id, _)| *id).collect();
    Merged {
        canvas,
        squares,
        links_added: d.links_added.into_iter().collect(),
        links_removed: d.links_removed.into_iter().collect(),
        changes,
    }
}

/// A canvas as text, for `git diff` to compare lines of (a textconv driver):
/// one line per square and per wire in the order of the ids, no positions,
/// no camera. Multi-line values go on lines of their own.
pub fn outline(c: &Canvas) -> String {
    let mut squares: Vec<&Square> = c.squares.iter().collect();
    squares.sort_by_key(|s| s.id);
    let mut out = String::new();
    for s in squares {
        out.push_str(&format!("square {} {}\n", s.id, s.module));
        for (what, m) in [("field", &s.fields), ("param", &s.params)] {
            for (k, v) in m {
                if v.contains('\n') {
                    out.push_str(&format!("    {what} {k}:\n"));
                    // split, not lines(): a CRLF and a last newline are differences too
                    for l in v.split('\n') {
                        out.push_str(&format!("        | {}\n", l.replace('\r', "\\r")));
                    }
                } else {
                    out.push_str(&format!("    {what} {k} = {v:?}\n"));
                }
            }
        }
    }
    let mut links = c.links.clone();
    links.sort();
    for l in links {
        out.push_str(&format!("wire {}\n", wire(&l)));
    }
    out
}

/// `1.then -> 2.tick`
pub fn wire(l: &Link) -> String {
    format!("{}.{} -> {}.{}", l.from, l.output, l.to, l.input)
}

/// A square that uses a subgraph.
#[derive(Clone, Debug, PartialEq)]
pub struct Use {
    pub file: PathBuf,
    pub squares: Vec<SquareId>,
}

/// The module path of the subgraph at FILE (`a/b` for `.../modules/a/b.graph.lua`,
/// or just the name if it isn't in a `modules` folder), and the folder to look
/// for its uses in: the one that has that `modules` folder (or FILE's own).
pub fn subgraph_of(file: &Path) -> Option<(String, PathBuf)> {
    // a relative path's folder can be empty: that's here
    let here = |p: &Path| if p.as_os_str().is_empty() { PathBuf::from(".") } else { p.to_path_buf() };
    let name = file.file_name()?.to_str()?.strip_suffix(".graph.lua")?;
    let mut parts = vec![name.to_string()];
    let mut dir = file.parent()?;
    loop {
        if dir.file_name().is_some_and(|n| n == "modules") {
            parts.reverse();
            return Some((parts.join("/"), here(dir.parent().unwrap_or(dir))));
        }
        match (dir.file_name().and_then(|n| n.to_str()), dir.parent()) {
            (Some(n), Some(p)) => {
                parts.push(n.to_string());
                dir = p;
            }
            _ => break,
        }
    }
    Some((name.to_string(), here(file.parent()?)))
}

/// The canvases under ROOT (down to four folders) with squares of MODULE.
/// Files that aren't canvases, and folders `target` and `.git`, are skipped.
pub fn uses(root: &Path, module: &str) -> Vec<Use> {
    fn walk(dir: &Path, depth: usize, module: &str, out: &mut Vec<Use>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut paths: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        paths.sort();
        for p in paths {
            if p.is_dir() {
                let skip = p.file_name().is_some_and(|n| n == "target" || n == ".git" || n == "node_modules");
                if depth > 0 && !skip {
                    walk(&p, depth - 1, module, out);
                }
            } else if p.extension().is_some_and(|e| e == "lua")
                && !p.to_string_lossy().ends_with(".type.lua")
                && !p.to_string_lossy().ends_with(".native.lua")
                && let Ok(src) = std::fs::read_to_string(&p)
                && let Ok(c) = crate::file::read_canvas(&src, &p.display().to_string())
            {
                let mut ids: Vec<SquareId> = c.squares.iter().filter(|s| s.module == module).map(|s| s.id).collect();
                ids.sort();
                if !ids.is_empty() {
                    out.push(Use { file: p, squares: ids });
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(root, 4, module, &mut out);
    out
}

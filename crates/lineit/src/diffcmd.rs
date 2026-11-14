//! `lineit diff A B` and `lineit text FILE`: what changed between two
//! canvases, as text (see `lineit_core::diff`).

use std::path::{Path, PathBuf};

use lineit_core::diff::{self, Diff, Tag};
use lineit_core::file;
use lineit_core::model::Canvas;

/// Lines of context around a change in a text.
const CONTEXT: usize = 2;

/// A canvas to compare: a file, or `REV:path` for the file as git has it
/// (what `git show REV:path` prints). Gives the canvas and the path of the
/// file in the working tree, where it may be: git's path is from the root of
/// the repository, unless it starts with `./` or `../` (from here).
pub fn read(arg: &str) -> Result<(Canvas, PathBuf), String> {
    let path = Path::new(arg);
    let (src, at) = if path.is_file() || !arg.contains(':') {
        (std::fs::read_to_string(path).map_err(|e| format!("{arg}: {e}"))?, path.to_path_buf())
    } else {
        let src = git_show(arg)?;
        let rel = arg.split_once(':').map_or(arg, |(_, p)| p);
        let from = if rel.starts_with("./") || rel.starts_with("../") { None } else { repo_root() };
        let at = match from {
            Some(root) => root.join(rel),
            None => std::env::current_dir().map_err(|e| e.to_string())?.join(rel),
        };
        (src, at)
    };
    let mut c = file::read_canvas(&src, arg)?;
    c.dir = at.parent().map(Path::to_path_buf);
    Ok((c, at))
}

/// The top folder of the git repository here.
fn repo_root() -> Option<PathBuf> {
    let out = std::process::Command::new("git").args(["rev-parse", "--show-toplevel"]).output().ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    out.status.success().then(|| PathBuf::from(text.trim_end_matches(['\n', '\r'])))
}

/// What `git show SPEC` prints.
pub fn git_show(spec: &str) -> Result<String, String> {
    let out = std::process::Command::new("git")
        .args(["show", spec])
        .output()
        .map_err(|e| format!("{spec}: no such file, and git didn't run: {e}"))?;
    if !out.status.success() {
        return Err(format!("{spec}: no such file, and git says: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    String::from_utf8(out.stdout).map_err(|_| format!("{spec}: not text"))
}

/// A short description of a square: `4 math/add`.
fn what(id: u64, module: &str) -> String {
    format!("{id} {module}")
}

fn shown(v: &Option<String>) -> String {
    match v {
        Some(v) => format!("{v:?}"),
        None => "(none)".into(),
    }
}

fn hunks(out: &mut String, old: &str, new: &str, indent: &str) {
    for (i, h) in diff::lines(old, new, CONTEXT).iter().enumerate() {
        if i > 0 {
            out.push_str(&format!("{indent}...\n"));
        }
        for (tag, line) in h {
            let c = match tag {
                Tag::Same => ' ',
                Tag::Removed => '-',
                Tag::Added => '+',
            };
            out.push_str(&format!("{indent}{c} {line}\n"));
        }
    }
}

fn entries(out: &mut String, what: &str, es: &[diff::Entry]) {
    for e in es {
        let multi =
            e.old.as_deref().is_some_and(|v| v.contains('\n')) || e.new.as_deref().is_some_and(|v| v.contains('\n'));
        if multi {
            out.push_str(&format!("    {what} {} changed:\n", e.key));
            hunks(out, e.old.as_deref().unwrap_or(""), e.new.as_deref().unwrap_or(""), "      ");
        } else {
            out.push_str(&format!("    {what} {}: {} -> {}\n", e.key, shown(&e.old), shown(&e.new)));
        }
    }
}

/// A diff as text.
pub fn render(d: &Diff) -> String {
    let mut out = String::new();
    for s in &d.removed {
        out.push_str(&format!("removed square {}\n", what(s.id, &s.module)));
    }
    for s in &d.added {
        out.push_str(&format!("added square {}\n", what(s.id, &s.module)));
        for (w, m) in [("field", &s.fields), ("param", &s.params)] {
            for (k, v) in m {
                if v.contains('\n') {
                    out.push_str(&format!("    {w} {k}:\n"));
                    hunks(&mut out, "", v, "      ");
                } else {
                    out.push_str(&format!("    {w} {k} = {v:?}\n"));
                }
            }
        }
    }
    for c in &d.changed {
        if c.module_changed() {
            out.push_str(&format!("changed square {}: module {} -> {}\n", c.id, c.old_module, c.new_module));
        } else {
            out.push_str(&format!("changed square {}\n", what(c.id, &c.new_module)));
        }
        entries(&mut out, "field", &c.fields);
        entries(&mut out, "param", &c.params);
    }
    for l in &d.links_removed {
        out.push_str(&format!("removed wire {}\n", diff::wire(l)));
    }
    for l in &d.links_added {
        out.push_str(&format!("added wire {}\n", diff::wire(l)));
    }
    if !d.moved.is_empty() {
        let list: Vec<String> = d.moved.iter().map(|s| what(s.id, &s.module)).collect();
        out.push_str(&format!("moved only ({}): {}\n", list.len(), list.join(", ")));
    }
    out
}

/// `lineit diff A B`: prints the diff, and says whether the programs differ.
/// If B (or A) is a subgraph, says where it's used.
pub fn run(a: &str, b: &str) -> Result<bool, String> {
    let (old, old_at) = read(a)?;
    let (new, new_at) = read(b)?;
    let d = diff::diff(&old, &new);
    print!("{}", render(&d));
    if d.is_empty() && d.moved.is_empty() {
        return Ok(false);
    }
    if !d.is_empty() {
        // a subgraph that changed: where it's used, so a reviewer knows what else it touches
        for at in [&new_at, &old_at] {
            if let Some((module, root)) = diff::subgraph_of(at) {
                let uses = diff::uses(&root, &module);
                if uses.is_empty() {
                    println!("subgraph {module}: not used in {}", root.display());
                } else {
                    println!("subgraph {module} is used in:");
                    for u in uses {
                        let ids: Vec<String> = u.squares.iter().map(u64::to_string).collect();
                        println!(
                            "    {}: square{} {}",
                            u.file.display(),
                            if ids.len() == 1 { "" } else { "s" },
                            ids.join(", ")
                        );
                    }
                }
                break;
            }
        }
    }
    Ok(!d.is_empty())
}

/// `lineit diff --view A B`: opens what changed in the editor, read-only.
pub fn view(a: &str, b: &str) -> Result<(), String> {
    let (old, _) = read(a)?;
    let (new, _) = read(b)?;
    lineit_rt::ext::capture_stderr();
    crate::editor::views(vec![crate::editor::View { label: format!("diff {a} .. {b}"), old: Some(old), new }])
}

/// `lineit show SPEC...`: opens each canvas (`REV:path` for a file as git
/// has it) in a read-only tab.
pub fn show(specs: &[&String]) -> Result<(), String> {
    let mut views = Vec::new();
    for spec in specs {
        let (new, _) = read(spec)?;
        views.push(crate::editor::View { label: spec.to_string(), old: None, new });
    }
    lineit_rt::ext::capture_stderr();
    crate::editor::views(views)
}

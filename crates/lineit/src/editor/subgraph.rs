//! Make subgraph (Ctrl+G): the selected squares go to a canvas of their own,
//! a module in the user's modules folder, and a square of it takes their
//! place. Wires that went in or out of the selection go through ports:
//! one `graph/input` per output outside that something inside reads or is
//! ticked by, one `graph/output` per output inside that something outside
//! reads or is ticked by.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use lineit_core::model::{Canvas, Link, Square, SquareId};

use super::tab::Tab;

fn port_type(tab: &Tab, id: SquareId, output: &str) -> String {
    let Some(info) = tab.lowered.squares.get(&id) else { return "int".into() };
    match info.sig.output_index(output) {
        Some(o) => match &info.out_types[o] {
            Some(t) => t.to_string(),
            None => "tick".into(),
        },
        None => "int".into(),
    }
}

fn unique(name: &str, used: &mut BTreeSet<String>) -> String {
    let mut n = name.to_string();
    let mut i = 2;
    while !used.insert(n.clone()) {
        n = format!("{name}{i}");
        i += 1;
    }
    n
}

/// Makes the subgraph NAME of SELECTED, and returns its file.
pub fn make(tab: &mut Tab, selected: &BTreeSet<SquareId>, name: &str) -> Result<PathBuf, String> {
    if selected.is_empty() {
        return Err("select the squares first".into());
    }
    if name.is_empty()
        || !name
            .split('/')
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'))
    {
        return Err("a name is letters, digits, - and _, in folders with /".into());
    }
    let file = lineit_core::paths::modules_dir().join(format!("{name}.graph.lua"));
    if file.exists() {
        return Err(format!("{} is already there", file.display()));
    }
    let inside: Vec<Square> = tab.canvas.squares.iter().filter(|s| selected.contains(&s.id)).cloned().collect();
    let min_x = inside.iter().map(|s| s.x).fold(f32::INFINITY, f32::min);
    let min_y = inside.iter().map(|s| s.y).fold(f32::INFINITY, f32::min);
    let max_x = inside.iter().map(|s| s.x).fold(f32::NEG_INFINITY, f32::max);

    let mut sub = Canvas::default();
    for s in &inside {
        let mut s = s.clone();
        s.x -= min_x;
        s.y -= min_y;
        sub.squares.push(s);
    }
    let mut next = sub.next_id().max(tab.canvas.next_id());
    let mut used_in = BTreeSet::new();
    let mut used_out = BTreeSet::new();
    let mut ins: BTreeMap<(SquareId, String), (SquareId, String)> = BTreeMap::new(); // outside source -> port
    let mut outs: BTreeMap<(SquareId, String), (SquareId, String)> = BTreeMap::new(); // inside source -> port
    let mut outer_links = Vec::new();
    for l in &tab.canvas.links {
        let (a, b) = (selected.contains(&l.from), selected.contains(&l.to));
        match (a, b) {
            (true, true) => sub.links.push(l.clone()),
            (false, true) => {
                let key = (l.from, l.output.clone());
                let (port, port_name) = ins
                    .entry(key)
                    .or_insert_with(|| {
                        let id = next;
                        next += 1;
                        let pname = unique(&l.output, &mut used_in);
                        let target_y = tab.canvas.square(l.to).map(|s| s.y - min_y).unwrap_or(0.0);
                        sub.squares.push(
                            Square::new(id, "graph/input", -260.0, target_y)
                                .with_param("name", &pname)
                                .with_param("type", &port_type(tab, l.from, &l.output))
                                .with_param("default", ""),
                        );
                        outer_links.push(Link { from: l.from, output: l.output.clone(), to: 0, input: pname.clone() });
                        (id, pname)
                    })
                    .clone();
                sub.links.push(Link { from: port, output: "value".into(), to: l.to, input: l.input.clone() });
                let _ = port_name;
            }
            (true, false) => {
                let key = (l.from, l.output.clone());
                let (port, port_name) = outs
                    .entry(key)
                    .or_insert_with(|| {
                        let id = next;
                        next += 1;
                        let pname = unique(&l.output, &mut used_out);
                        let src_y = tab.canvas.square(l.from).map(|s| s.y - min_y).unwrap_or(0.0);
                        sub.squares.push(
                            Square::new(id, "graph/output", max_x - min_x + 260.0, src_y)
                                .with_param("name", &pname)
                                .with_param("type", &port_type(tab, l.from, &l.output)),
                        );
                        sub.links.push(Link { from: l.from, output: l.output.clone(), to: id, input: "value".into() });
                        (id, pname)
                    })
                    .clone();
                let _ = port;
                outer_links.push(Link { from: 0, output: port_name, to: l.to, input: l.input.clone() });
            }
            (false, false) => {}
        }
    }

    if let Some(p) = file.parent() {
        std::fs::create_dir_all(p).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    std::fs::write(&file, lineit_core::file::write_canvas(&sub)).map_err(|e| format!("{}: {e}", file.display()))?;

    // the square of it, where the squares were
    let id = tab.canvas.next_id();
    for s in selected {
        tab.canvas.remove(*s);
    }
    tab.canvas.add(Square::new(id, name, min_x, min_y));
    for mut l in outer_links {
        if l.to == 0 {
            l.to = id;
        } else {
            l.from = id;
        }
        tab.canvas.links.push(l);
    }
    tab.selected.clear();
    tab.selected.insert(id);
    Ok(file)
}

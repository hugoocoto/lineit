//! The canvas: what the editor draws and the user changes, and what the
//! files keep. It's the program's syntax tree: lowering ([`crate::lower`])
//! resolves it into the IR.
//!
//! Pins are named, not numbered, so a canvas still opens after a module
//! gains or reorders pins.

use std::collections::BTreeMap;

pub type SquareId = u64;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Canvas {
    /// What the canvas does, for the person reading it. For a subgraph, the
    /// text of its module in the search.
    pub doc: String,
    pub camera: Camera,
    pub squares: Vec<Square>,
    pub links: Vec<Link>,
    /// The folder its file is in, for the paths in its squares (not
    /// saved: whoever opens it sets it).
    pub dir: Option<std::path::PathBuf>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Camera {
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Camera { x: 0.0, y: 0.0, zoom: 1.0 }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Square {
    pub id: SquareId,
    /// The module's path: `math/add`, `mypackage/reverse`...
    pub module: String,
    /// Where its top left corner is, in canvas units.
    pub x: f32,
    pub y: f32,
    /// Its params, by name, as typed.
    pub params: BTreeMap<String, String>,
    /// What's typed in the fields of its inputs, by input name. Used while
    /// nothing is wired to the input.
    pub fields: BTreeMap<String, String>,
}

impl Square {
    pub fn new(id: SquareId, module: impl Into<String>, x: f32, y: f32) -> Square {
        Square { id, module: module.into(), x, y, params: BTreeMap::new(), fields: BTreeMap::new() }
    }

    pub fn with_param(mut self, k: &str, v: &str) -> Square {
        self.params.insert(k.into(), v.into());
        self
    }

    pub fn with_field(mut self, k: &str, v: &str) -> Square {
        self.fields.insert(k.into(), v.into());
        self
    }
}

/// A wire, from an output to an input.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Link {
    pub from: SquareId,
    pub output: String,
    pub to: SquareId,
    pub input: String,
}

impl Link {
    pub fn new(from: SquareId, output: &str, to: SquareId, input: &str) -> Link {
        Link { from, output: output.into(), to, input: input.into() }
    }
}

impl Canvas {
    pub fn square(&self, id: SquareId) -> Option<&Square> {
        self.squares.iter().find(|s| s.id == id)
    }

    pub fn square_mut(&mut self, id: SquareId) -> Option<&mut Square> {
        self.squares.iter_mut().find(|s| s.id == id)
    }

    /// An id no square has.
    pub fn next_id(&self) -> SquareId {
        self.squares.iter().map(|s| s.id).max().map_or(1, |m| m + 1)
    }

    pub fn add(&mut self, s: Square) -> SquareId {
        let id = s.id;
        self.squares.push(s);
        id
    }

    /// Removes the square and its wires.
    pub fn remove(&mut self, id: SquareId) {
        self.squares.retain(|s| s.id != id);
        self.links.retain(|l| l.from != id && l.to != id);
    }

    /// Adds a wire, unless it's already there. A data input takes one wire:
    /// with EXCLUSIVE, the one it had goes.
    pub fn connect(&mut self, link: Link, exclusive: bool) {
        if exclusive {
            self.links.retain(|l| !(l.to == link.to && l.input == link.input));
        }
        if !self.links.contains(&link) {
            self.links.push(link);
        }
    }

    /// The wires into an input.
    pub fn links_into<'a>(&'a self, id: SquareId, input: &'a str) -> impl Iterator<Item = &'a Link> + 'a {
        self.links.iter().filter(move |l| l.to == id && l.input == input)
    }

    /// The wires out of an output.
    pub fn links_from<'a>(&'a self, id: SquareId, output: &'a str) -> impl Iterator<Item = &'a Link> + 'a {
        self.links.iter().filter(move |l| l.from == id && l.output == output)
    }

    /// Whether every square is in one place (a canvas written by hand
    /// without positions): [`Canvas::arrange`] spreads them.
    pub fn needs_arranging(&self) -> bool {
        self.squares.len() > 1 && self.squares.windows(2).all(|w| w[0].x == w[1].x && w[0].y == w[1].y)
    }

    /// Puts the squares in columns, each after the ones wired into it, one
    /// below the other by their HEIGHT, each column as wide as its widest
    /// square's WIDTH (where they're known).
    pub fn arrange(&mut self, height: &dyn Fn(&Square) -> Option<f32>, width: &dyn Fn(&Square) -> Option<f32>) {
        let mut col: std::collections::HashMap<SquareId, usize> = self.squares.iter().map(|s| (s.id, 0)).collect();
        // longest path from the squares nothing goes into, with a limit for loops
        for _ in 0..self.squares.len().min(64) {
            let mut changed = false;
            for l in &self.links {
                if l.from == l.to {
                    continue;
                }
                let c = col[&l.from] + 1;
                if c > col[&l.to] && c < 64 {
                    col.insert(l.to, c);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        // where each column starts: after the widest square of the one before
        let n = col.values().max().map_or(0, |m| m + 1);
        let mut widths = vec![200.0f32; n];
        for s in &self.squares {
            let w = &mut widths[col[&s.id]];
            *w = w.max(width(s).unwrap_or(200.0));
        }
        let lefts: Vec<f32> = widths.iter().scan(0.0, |x, w| Some(std::mem::replace(x, *x + w + 80.0))).collect();
        let mut rows: std::collections::HashMap<usize, f32> = std::collections::HashMap::new();
        for s in &mut self.squares {
            let c = col[&s.id];
            let y = rows.entry(c).or_insert(0.0);
            s.x = lefts[c];
            s.y = *y;
            *y += height(s).unwrap_or(120.0) + 48.0;
        }
    }
}

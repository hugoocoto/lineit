//! Where vim mode goes: the square nearest in a direction, along the wires,
//! in order, or matching a search; and where the camera has to be to show
//! one. All of it over the rectangles the canvas drew last frame (in world
//! units) and the wires, so it needs no window.

use std::collections::{BTreeSet, HashMap};

use eframe::egui::{Pos2, Rect, Vec2, vec2};
use lineit_core::model::{Canvas, Link, SquareId};

use super::vim::Dir;

pub type Rects = HashMap<SquareId, Rect>;

/// Squares left to right and, in a column, top to bottom: squares whose
/// left sides are within a column's width of each other are in one.
pub fn order(rects: &Rects) -> Vec<SquareId> {
    let mut v: Vec<(i32, i32, SquareId)> =
        rects.iter().map(|(id, r)| ((r.min.x / 100.0).round() as i32, r.min.y.round() as i32, *id)).collect();
    v.sort();
    v.into_iter().map(|x| x.2).collect()
}

/// The square nearest to FROM in direction DIR: along that axis it has to
/// be past FROM's center, and being off the axis counts double.
pub fn nearest(rects: &Rects, from: SquareId, dir: Dir) -> Option<SquareId> {
    let a = rects.get(&from)?.center();
    let mut best: Option<(f32, SquareId)> = None;
    for (&id, r) in rects {
        if id == from {
            continue;
        }
        let d = r.center() - a;
        let (along, across) = match dir {
            Dir::Right => (d.x, d.y),
            Dir::Left => (-d.x, d.y),
            Dir::Down => (d.y, d.x),
            Dir::Up => (-d.y, d.x),
        };
        if along <= 1.0 {
            continue;
        }
        let score = along + 2.0 * across.abs();
        if best.is_none_or(|(b, bid)| score < b || (score == b && id < bid)) {
            best = Some((score, id));
        }
    }
    best.map(|b| b.1)
}

fn y_of(rects: &Rects, id: SquareId) -> f32 {
    rects.get(&id).map_or(0.0, |r| r.center().y)
}

/// The squares wired from FROM (DOWN), or into it, top to bottom.
fn neighbours(links: &[Link], rects: &Rects, from: SquareId, down: bool) -> Vec<SquareId> {
    let mut v: Vec<SquareId> = links
        .iter()
        .filter_map(|l| match down {
            true if l.from == from => Some(l.to),
            false if l.to == from => Some(l.from),
            _ => None,
        })
        .filter(|id| *id != from && rects.contains_key(id))
        .collect();
    v.sort_by(|a, b| y_of(rects, *a).total_cmp(&y_of(rects, *b)).then(a.cmp(b)));
    v.dedup();
    v
}

/// One step along the wires: of the squares there, the one level with FROM.
fn step(links: &[Link], rects: &Rects, from: SquareId, down: bool) -> Option<SquareId> {
    let y = y_of(rects, from);
    neighbours(links, rects, from, down)
        .into_iter()
        .min_by(|a, b| (y_of(rects, *a) - y).abs().total_cmp(&(y_of(rects, *b) - y).abs()).then(a.cmp(b)))
}

/// N steps downstream (or upstream), as far as they go.
pub fn follow(links: &[Link], rects: &Rects, from: SquareId, down: bool, n: usize) -> Option<SquareId> {
    let mut at = from;
    for _ in 0..n {
        match step(links, rects, at, down) {
            Some(next) => at = next,
            None => break,
        }
    }
    (at != from).then_some(at)
}

/// The start (or the end) of the chain FROM is in.
pub fn chain_end(links: &[Link], rects: &Rects, from: SquareId, end: bool) -> SquareId {
    let mut seen = BTreeSet::from([from]);
    let mut at = from;
    while let Some(next) = step(links, rects, at, end) {
        if !seen.insert(next) {
            break;
        }
        at = next;
    }
    at
}

/// The square DELTA places along among those wired from the same square
/// (or, with nothing wired into FROM, among the squares nothing is wired
/// into), round.
pub fn sibling(links: &[Link], rects: &Rects, from: SquareId, delta: i32) -> Option<SquareId> {
    let list = match neighbours(links, rects, from, false).first() {
        Some(parent) => neighbours(links, rects, *parent, true),
        None => order(rects).into_iter().filter(|id| neighbours(links, rects, *id, false).is_empty()).collect(),
    };
    let i = list.iter().position(|id| *id == from)?;
    let next = list[(i as i32 + delta).rem_euclid(list.len() as i32) as usize];
    (next != from).then_some(next)
}

/// The square DELTA places on in [`order`], round. From nothing: the first.
pub fn reading(rects: &Rects, from: Option<SquareId>, delta: i32) -> Option<SquareId> {
    let list = order(rects);
    if list.is_empty() {
        return None;
    }
    match from.and_then(|f| list.iter().position(|id| *id == f)) {
        Some(i) => Some(list[(i as i32 + delta).rem_euclid(list.len() as i32) as usize]),
        None => Some(list[0]),
    }
}

/// The DELTA-th change after (or, negative, before) FROM in [`order`],
/// round: CHANGES are the squares a diff marks. From nothing: the first
/// (or last).
pub fn change(rects: &Rects, changes: &BTreeSet<SquareId>, from: Option<SquareId>, delta: i32) -> Option<SquareId> {
    let mut list = order(rects);
    if delta < 0 {
        list.reverse();
    }
    // from just after FROM, all the way round, FROM last
    if let Some(i) = from.and_then(|f| list.iter().position(|id| *id == f)) {
        list.rotate_left(i + 1);
    }
    let found: Vec<SquareId> = list.into_iter().filter(|id| changes.contains(id)).collect();
    if found.is_empty() {
        return None;
    }
    Some(found[((delta.unsigned_abs() as usize).max(1) - 1) % found.len()])
}

/// Every square wired to FROM, by any number of wires either way.
pub fn connected(links: &[Link], from: SquareId) -> BTreeSet<SquareId> {
    let mut seen = BTreeSet::from([from]);
    let mut todo = vec![from];
    while let Some(at) = todo.pop() {
        for l in links {
            let other = if l.from == at {
                l.to
            } else if l.to == at {
                l.from
            } else {
                continue;
            };
            if seen.insert(other) {
                todo.push(other);
            }
        }
    }
    seen
}

/// The squares with QUERY in their module, a field, a param or a note, in
/// [`order`].
pub fn search(canvas: &Canvas, rects: &Rects, query: &str) -> Vec<SquareId> {
    let q = query.to_lowercase();
    let hit = |id: SquareId| {
        canvas.square(id).is_some_and(|s| {
            s.module.to_lowercase().contains(&q)
                || s.fields.values().chain(s.params.values()).any(|v| v.to_lowercase().contains(&q))
        })
    };
    order(rects).into_iter().filter(|id| hit(*id)).collect()
}

/// Where the focus goes when REMOVED are deleted: the square closest to it.
pub fn after_delete(rects: &Rects, removed: &BTreeSet<SquareId>, focus: SquareId) -> Option<SquareId> {
    let a = rects.get(&focus)?.center();
    rects
        .iter()
        .filter(|(id, _)| !removed.contains(id))
        .min_by(|(ia, a_), (ib, b_)| a.distance(a_.center()).total_cmp(&a.distance(b_.center())).then(ia.cmp(ib)))
        .map(|(id, _)| *id)
}

/// The camera (the world point at the center of the view) that shows RECT
/// in a view VIEW screen points big at ZOOM, with MARGIN points around
/// it: CAM itself if it's shown already.
pub fn reveal(cam: Pos2, zoom: f32, view: Vec2, rect: Rect, margin: f32) -> Pos2 {
    let half = view / (2.0 * zoom);
    let m = margin / zoom;
    let mut c = cam;
    for axis in 0..2 {
        let (lo, hi, r0, r1, h) = if axis == 0 {
            (c.x - half.x + m, c.x + half.x - m, rect.min.x, rect.max.x, half.x)
        } else {
            (c.y - half.y + m, c.y + half.y - m, rect.min.y, rect.max.y, half.y)
        };
        let shift = if r1 - r0 > hi - lo {
            // bigger than the view: its center
            (r0 + r1) / 2.0 - (lo + hi) / 2.0
        } else if r0 < lo {
            r0 - lo
        } else if r1 > hi {
            r1 - hi
        } else {
            0.0
        };
        let _ = h;
        if axis == 0 {
            c.x += shift;
        } else {
            c.y += shift;
        }
    }
    c
}

/// The camera and zoom that show every rectangle in a VIEW, with MARGIN.
pub fn fit(rects: &Rects, view: Vec2, margin: f32, zoom_range: (f32, f32)) -> Option<(Pos2, f32)> {
    let mut all = rects.values();
    let first = *all.next()?;
    let bounds = all.fold(first, |b, r| b.union(*r));
    let room = vec2((view.x - 2.0 * margin).max(1.0), (view.y - 2.0 * margin).max(1.0));
    let zoom = (room.x / bounds.width().max(1.0)).min(room.y / bounds.height().max(1.0));
    Some((bounds.center(), zoom.clamp(zoom_range.0, zoom_range.1)))
}

/// A point of the world to the screen, for a canvas drawn in SCREEN with
/// the camera CAM at ZOOM.
pub fn to_screen(screen: Rect, cam: Pos2, zoom: f32, p: Pos2) -> Pos2 {
    screen.center() + (p - cam) * zoom
}

#[cfg(test)]
mod tests {
    use eframe::egui::pos2;

    use super::*;

    fn rect(x: f32, y: f32) -> Rect {
        Rect::from_min_size(pos2(x, y), vec2(200.0, 100.0))
    }

    fn link(from: SquareId, to: SquareId) -> Link {
        Link { from, output: "out".into(), to, input: "in".into() }
    }

    /// 1 -> 2 and 3 (3 below 2) -> 4; 5 alone, far below.
    fn sample() -> (Rects, Vec<Link>) {
        let rects: Rects = [
            (1, rect(0.0, 0.0)),
            (2, rect(300.0, -100.0)),
            (3, rect(300.0, 100.0)),
            (4, rect(600.0, 0.0)),
            (5, rect(0.0, 500.0)),
        ]
        .into();
        (rects, vec![link(1, 2), link(1, 3), link(2, 4), link(3, 4)])
    }

    #[test]
    fn directions() {
        let (r, _) = sample();
        assert_eq!(nearest(&r, 1, Dir::Right), Some(2));
        assert_eq!(nearest(&r, 2, Dir::Down), Some(3));
        assert_eq!(nearest(&r, 3, Dir::Up), Some(2));
        assert_eq!(nearest(&r, 4, Dir::Left), Some(2));
        assert_eq!(nearest(&r, 1, Dir::Down), Some(5));
        // nothing that way
        assert_eq!(nearest(&r, 1, Dir::Left), None);
        assert_eq!(nearest(&r, 4, Dir::Right), None);
        assert_eq!(nearest(&r, 99, Dir::Right), None);
    }

    #[test]
    fn order_is_by_column() {
        let (r, _) = sample();
        assert_eq!(order(&r), vec![1, 5, 2, 3, 4]);
        assert_eq!(reading(&r, Some(4), 1), Some(1));
        assert_eq!(reading(&r, Some(1), -1), Some(4));
        assert_eq!(reading(&r, None, 1), Some(1));
        assert_eq!(reading(&Rects::new(), None, 1), None);
    }

    #[test]
    fn along_the_wires() {
        let (r, l) = sample();
        // 1 is level with neither: the nearer in height is 2 (center y -50 vs 150, from 50)
        assert_eq!(follow(&l, &r, 1, true, 1), Some(2));
        assert_eq!(follow(&l, &r, 1, true, 2), Some(4));
        // more steps than there are: as far as it goes
        assert_eq!(follow(&l, &r, 1, true, 9), Some(4));
        assert_eq!(follow(&l, &r, 4, true, 1), None);
        assert_eq!(follow(&l, &r, 4, false, 1), Some(2));
        assert_eq!(follow(&l, &r, 5, true, 1), None);
        assert_eq!(chain_end(&l, &r, 1, true), 4);
        assert_eq!(chain_end(&l, &r, 4, false), 1);
        assert_eq!(chain_end(&l, &r, 5, true), 5);
    }

    #[test]
    fn siblings_and_loops() {
        let (r, l) = sample();
        assert_eq!(sibling(&l, &r, 2, 1), Some(3));
        assert_eq!(sibling(&l, &r, 3, 1), Some(2));
        assert_eq!(sibling(&l, &r, 2, -1), Some(3));
        // the roots, with nothing wired into them
        assert_eq!(sibling(&l, &r, 1, 1), Some(5));
        // 4 is the only one of its parent
        assert_eq!(sibling(&l, &r, 4, 1), None);
        // a loop doesn't go on for ever
        let (r, mut l) = sample();
        l.push(link(4, 1));
        let end = chain_end(&l, &r, 1, true);
        assert!([1, 2, 3, 4].contains(&end));
    }

    #[test]
    fn connected_chain() {
        let (_, l) = sample();
        assert_eq!(connected(&l, 4), BTreeSet::from([1, 2, 3, 4]));
        assert_eq!(connected(&l, 5), BTreeSet::from([5]));
    }

    #[test]
    fn focus_after_delete() {
        let (r, _) = sample();
        assert_eq!(after_delete(&r, &BTreeSet::from([2]), 2), Some(3));
        assert_eq!(after_delete(&r, &BTreeSet::from([1, 2, 3, 4, 5]), 2), None);
    }

    #[test]
    fn changes_in_order() {
        let r = |x: f32, y: f32| Rect::from_min_size(pos2(x, y), vec2(100.0, 50.0));
        // reading order: 1 (0,0), 2 (0,200), 3 (300,0), 4 (300,200), 5 (600,0)
        let rects: Rects = HashMap::from([
            (1, r(0.0, 0.0)),
            (2, r(0.0, 200.0)),
            (3, r(300.0, 0.0)),
            (4, r(300.0, 200.0)),
            (5, r(600.0, 0.0)),
        ]);
        let changes = BTreeSet::from([2, 4, 5]);
        assert_eq!(change(&rects, &changes, Some(1), 1), Some(2));
        assert_eq!(change(&rects, &changes, Some(2), 1), Some(4));
        assert_eq!(change(&rects, &changes, Some(3), 1), Some(4));
        // round
        assert_eq!(change(&rects, &changes, Some(5), 1), Some(2));
        assert_eq!(change(&rects, &changes, Some(2), -1), Some(5));
        assert_eq!(change(&rects, &changes, Some(3), -1), Some(2));
        assert_eq!(change(&rects, &changes, Some(1), 2), Some(4));
        assert_eq!(change(&rects, &changes, None, 1), Some(2));
        assert_eq!(change(&rects, &BTreeSet::new(), Some(1), 1), None);
    }

    #[test]
    fn camera_reveals() {
        let view = vec2(1000.0, 600.0);
        let cam = pos2(0.0, 0.0);
        // in view: stays
        assert_eq!(reveal(cam, 1.0, view, Rect::from_min_size(pos2(-100.0, -50.0), vec2(200.0, 100.0)), 20.0), cam);
        // off to the right: moves just enough, margin kept
        let c = reveal(cam, 1.0, view, Rect::from_min_size(pos2(600.0, 0.0), vec2(200.0, 100.0)), 20.0);
        assert_eq!(c, pos2(320.0, 0.0));
        // above
        let c = reveal(cam, 1.0, view, Rect::from_min_size(pos2(0.0, -500.0), vec2(200.0, 100.0)), 20.0);
        assert_eq!(c, pos2(0.0, -220.0));
        // zoomed out, the same world rect is nearer
        assert_eq!(reveal(cam, 0.5, view, Rect::from_min_size(pos2(600.0, 0.0), vec2(200.0, 100.0)), 20.0), cam);
    }

    #[test]
    fn fits_everything() {
        let (r, _) = sample();
        let (c, z) = fit(&r, vec2(1000.0, 600.0), 40.0, (0.2, 3.0)).unwrap();
        // 800 wide, 700 tall of squares: the height decides
        assert_eq!(c, pos2(400.0, 250.0));
        assert!((z - 520.0 / 700.0).abs() < 1e-4);
        assert!(fit(&Rects::new(), vec2(10.0, 10.0), 1.0, (0.2, 3.0)).is_none());
        let (_, z) = fit(&r, vec2(1000.0, 600.0), 40.0, (0.9, 3.0)).unwrap();
        assert_eq!(z, 0.9);
    }

    #[test]
    fn world_to_screen() {
        let screen = Rect::from_min_size(pos2(10.0, 20.0), vec2(1000.0, 600.0));
        assert_eq!(to_screen(screen, pos2(0.0, 0.0), 1.0, pos2(0.0, 0.0)), screen.center());
        assert_eq!(to_screen(screen, pos2(100.0, 0.0), 2.0, pos2(150.0, 0.0)), screen.center() + vec2(100.0, 0.0));
    }
}

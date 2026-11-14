//! The canvas: drawing the squares and wires of a tab, and what the mouse
//! does to them.
//!
//! Positions in the canvas file are world units; the camera turns them
//! into screen points ([`View`]). Squares, pins, fields and the canvas
//! itself are egui widgets, made in that order each frame, so egui gives a
//! click to the topmost one: a field over its square over the canvas.

use std::collections::{BTreeSet, HashMap};

use eframe::egui::emath::GuiRounding as _;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2, pos2, vec2,
};
use lineit_core::config::Config;
use lineit_core::lower::SquareInfo;
use lineit_core::model::{Link, Square, SquareId};
use lineit_core::registry::{ParamKind, PinTy, Registry};
use lineit_rt::{Type, Value};

use super::tab::Tab;
use super::theme::Theme;
use lineit_core::diff::Mark;

/// The least zoom at which a field is a text box that can be typed in.
pub const EDIT_ZOOM: f32 = 0.55;
pub const HEADER: f32 = 26.0;
pub const ROW: f32 = 22.0;
pub const PIN: f32 = 7.0;

#[derive(Clone, Copy)]
pub struct View {
    /// The screen point the camera looks at.
    pub origin: Pos2,
    /// The world point at the origin.
    pub cam: Vec2,
    pub zoom: f32,
}

impl View {
    pub fn to_screen(&self, p: Pos2) -> Pos2 {
        self.origin + (p.to_vec2() - self.cam) * self.zoom
    }
    pub fn to_world(&self, s: Pos2) -> Pos2 {
        (self.cam + (s - self.origin) / self.zoom).to_pos2()
    }
    pub fn rect(&self, r: Rect) -> Rect {
        Rect::from_min_max(self.to_screen(r.min), self.to_screen(r.max))
    }
}

/// Where a square's parts are, in world units.
pub struct Geo {
    pub rect: Rect,
    pub ins: Vec<Pos2>,
    pub outs: Vec<Pos2>,
    /// Top of the rows of params.
    pub params_top: f32,
    /// Top of what it shows (a value, an image), if anything.
    pub extra_top: f32,
}

pub fn geometry(sq: &Square, info: Option<&SquareInfo>, params: usize, width: f32, extra: f32) -> Geo {
    let (ni, no) = info.map(|i| (i.sig.inputs.len(), i.sig.outputs.len())).unwrap_or((0, 0));
    let rows = ni.max(no) as f32;
    let top = pos2(sq.x, sq.y);
    let pins_top = top.y + HEADER;
    let params_top = pins_top + rows * ROW + if rows > 0.0 { 4.0 } else { 0.0 };
    let extra_top = params_top + params as f32 * ROW;
    let h = (extra_top + extra + 6.0 - top.y).max(HEADER + ROW);
    let rect = Rect::from_min_size(top, vec2(width, h));
    let pin_y = |i: usize| pins_top + ROW * i as f32 + ROW / 2.0;
    Geo {
        rect,
        ins: (0..ni).map(|i| pos2(top.x, pin_y(i))).collect(),
        outs: (0..no).map(|i| pos2(top.x + width, pin_y(i))).collect(),
        params_top,
        extra_top,
    }
}

pub enum Drag {
    /// A wire from an output, to an input. PICKED: taken from an input it
    /// was plugged into, so dropped on nothing it's gone.
    Wire { from: (SquareId, String), ty: Option<Type>, picked: bool },
    /// A wire from an input, to an output.
    WireBack { to: (SquareId, String), ty: Option<Type> },
    /// A selection box, from this world point.
    Select { start: Pos2 },
}

/// A wire waiting for a square to be added at its end.
#[derive(Clone)]
pub enum Pending {
    From(SquareId, String, Option<Type>),
    To(SquareId, String, Option<Type>),
}

/// What the canvas wants the editor to do.
pub enum Action {
    Search {
        at: Pos2,
        screen: Pos2,
        pending: Option<Pending>,
    },
    OpenModule(String),
    MakeSubgraph,
    Copy,
    Delete,
    Duplicate,
    /// Open the report of the tests.
    ShowReport,
}

#[derive(Default)]
pub struct CanvasState {
    pub drag: Option<Drag>,
    /// Images on squares, by square and output, with the image they show.
    pub previews: HashMap<(SquareId, String), (egui::TextureHandle, lineit_rt::Image)>,
    /// The field being typed in, if any.
    pub edit: Option<FieldEdit>,
    /// Where the strip that shows what's typed is (see `App::editor_ui`).
    pub strip: Option<Rect>,
    /// Where the canvas was drawn, on the screen: the strip goes over its
    /// bottom.
    pub rect: Option<Rect>,
    /// Where each square was drawn last frame, in world units: vim mode
    /// moves over them.
    pub rects: HashMap<SquareId, Rect>,
    pub vim: VimState,
    /// The tests of code squares: their table, how they went.
    pub tests: super::tests::TestsUi,
}

/// What vim mode keeps for a tab.
#[derive(Default)]
pub struct VimState {
    /// Vim mode is on: the focus is drawn.
    pub on: bool,
    /// The square the keys act on.
    pub focus: Option<SquareId>,
    /// The selection as vim mode left it: if it's another, the mouse
    /// changed it.
    pub sel: BTreeSet<SquareId>,
    /// The squares Visual went through, to shrink back along.
    pub trail: Vec<SquareId>,
    /// Wiring from the keyboard, if it's going on.
    pub pin: Option<PinState>,
    /// The pins numbered on the focused square.
    pub hints: Vec<Hint>,
    /// A square to show: its rectangle isn't known yet.
    pub reveal: Option<SquareId>,
    /// A square just added by `a`, `o` or `O`: once drawn, and so measured,
    /// it goes with its right edge (true) or its left edge at this x.
    pub settle: Option<(SquareId, f32, bool)>,
}

/// A pin of a square, for wiring from the keyboard.
#[derive(Clone, Debug, PartialEq)]
pub struct PinRef {
    pub sq: SquareId,
    pub name: String,
    pub output: bool,
    pub tick: bool,
    pub ty: Option<Type>,
}

pub struct Hint {
    pub pin: PinRef,
    pub label: String,
    /// The one Enter takes.
    pub cursor: bool,
}

/// Wiring from the keyboard: first a pin of the focused square, then the
/// pin of another that it goes to.
pub struct PinState {
    pub from: Option<PinRef>,
    /// Which candidate Enter takes, once Tab moved it.
    pub cursor: Option<usize>,
}

/// Whether a wire can go between pins of squares A and B: ticks to ticks and
/// data to data, and not into the same square. The mouse and the keyboard
/// both ask.
pub fn can_wire(source_tick: bool, target_tick: bool, a: SquareId, b: SquareId) -> bool {
    source_tick == target_tick && a != b
}

/// The pins of square ID: its outputs, then its inputs.
pub fn pins_of(tab: &Tab, id: SquareId) -> Vec<PinRef> {
    let Some(info) = tab.lowered.squares.get(&id) else { return Vec::new() };
    let outs = info.sig.outputs.iter().enumerate().map(|(o, d)| PinRef {
        sq: id,
        name: d.name.clone(),
        output: true,
        tick: d.ty == PinTy::Tick,
        ty: info.out_types[o].clone(),
    });
    let ins = info.sig.inputs.iter().enumerate().map(|(i, d)| PinRef {
        sq: id,
        name: d.name.clone(),
        output: false,
        tick: d.ty == PinTy::Tick,
        ty: info.in_types[i].clone(),
    });
    outs.chain(ins).collect()
}

/// Removes every wire of the selected squares.
pub fn disconnect(tab: &mut Tab) {
    if tab.label.is_some() {
        return;
    }
    let sel = tab.selected.clone();
    tab.canvas.links.retain(|l| !sel.contains(&l.from) && !sel.contains(&l.to));
}

/// A field of a square: an input's (by its name) or a param's.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldKey {
    pub sq: SquareId,
    pub name: String,
    pub param: bool,
}

/// What a field's text has to be.
#[derive(Clone, Debug)]
enum Check {
    /// Anything: inputs that take any type get it from what's typed.
    Any,
    Is(Type),
    /// Of the type, or nothing (a field that starts empty: a test/case's).
    IsOrEmpty(Type),
    Param(ParamKind),
}

#[derive(Clone, Debug, PartialEq)]
enum FieldKind {
    Typed,
    Bool,
    Choice(Vec<String>),
}

/// Typing in a field: what's typed applies on Enter, on Esc, on Tab (which
/// goes on to the square's next field) or on a click away. If it isn't
/// valid, Enter and Tab keep the field open, in red; Esc and a click away
/// leave the value as it was. A checkbox or a list that Tab gets to is CHOOSE: Space or Enter
/// toggles it, or goes to its next option.
pub struct FieldEdit {
    pub key: FieldKey,
    pub text: String,
    pub error: bool,
    /// Just opened: it takes the keyboard, its text selected.
    pub fresh: bool,
    pub choose: bool,
    /// "math/add > b (int)", for the strip.
    pub title: String,
    /// A note's text: Enter is a new line, and it's typed on the note itself.
    pub multiline: bool,
    check: Check,
    /// Where it was drawn, on the screen.
    pub rect: Option<Rect>,
}

/// The fields of square ID, in Tab order: its inputs that take one and
/// have nothing wired, then its params, with what each is, its check, its
/// title and its value now.
fn fields(tab: &Tab, reg: &Registry, id: SquareId) -> Vec<(FieldKey, FieldKind, Check, String, String)> {
    let mut v = Vec::new();
    let Some(sq) = tab.canvas.square(id) else { return v };
    if let Some(info) = tab.lowered.squares.get(&id) {
        for (i, pin) in info.sig.inputs.iter().enumerate() {
            let Some(default) = &pin.field else { continue };
            if tab.canvas.links_into(id, &pin.name).next().is_some() {
                continue;
            }
            let ty = info.in_types[i].clone();
            let kind = if ty == Some(Type::Bool) { FieldKind::Bool } else { FieldKind::Typed };
            let check = match &pin.ty {
                PinTy::Data(lineit_core::registry::TExpr::T(t)) if default.is_empty() => Check::IsOrEmpty(t.clone()),
                PinTy::Data(lineit_core::registry::TExpr::T(t)) => Check::Is(t.clone()),
                _ => Check::Any,
            };
            let ty_name = ty.map(|t| t.to_string()).unwrap_or_else(|| "any".into());
            let title = format!("{}  >  {} ({ty_name})", sq.module, pin.name);
            let now = sq.fields.get(&pin.name).cloned().unwrap_or_else(|| default.clone());
            v.push((FieldKey { sq: id, name: pin.name.clone(), param: false }, kind, check, title, now));
        }
    }
    if let Some(m) = reg.get(&sq.module) {
        for def in m.params() {
            // code in a file is typed in the file; tests in their table
            if sq.module == CODE && (def.name == "tests" || (def.name == "code" && code_file(sq).is_some())) {
                continue;
            }
            let kind = match &def.kind {
                ParamKind::Bool => FieldKind::Bool,
                ParamKind::Choice(c) => FieldKind::Choice(c.clone()),
                ParamKind::Function { args } => FieldKind::Choice(reg.functions(args.len())),
                _ => FieldKind::Typed,
            };
            let title = format!("{}  >  {} (param)", sq.module, def.name);
            let now = sq.params.get(&def.name).cloned().unwrap_or_else(|| def.default.clone());
            v.push((
                FieldKey { sq: id, name: def.name.clone(), param: true },
                kind,
                Check::Param(def.kind.clone()),
                title,
                now,
            ));
        }
    }
    v
}

/// Starts typing in field KEY (or choosing, for a checkbox or a list).
pub fn start_edit(tab: &Tab, reg: &Registry, state: &mut CanvasState, key: &FieldKey) {
    if tab.label.is_some() {
        return;
    }
    let Some((key, kind, check, title, now)) = fields(tab, reg, key.sq).into_iter().find(|f| f.0 == *key) else {
        return;
    };
    let choose = kind != FieldKind::Typed;
    let multiline =
        tab.canvas.square(key.sq).is_some_and(|s| s.module == NOTE || (s.module == CODE && key.name == "code"));
    state.edit =
        Some(FieldEdit { key, text: now, error: false, fresh: true, choose, title, check, multiline, rect: None });
}

/// A square just added: typing goes to its first field to type in, its
/// value selected, so what's typed replaces it.
pub fn start_first(tab: &Tab, reg: &Registry, state: &mut CanvasState, id: SquareId) {
    if let Some((key, ..)) = fields(tab, reg, id).into_iter().find(|f| f.1 == FieldKind::Typed) {
        start_edit(tab, reg, state, &key);
    }
}

/// Typing starts in square ID's Nth field (counting from 1), or in the
/// first one to type in (the first, if it has none). False: it has no fields.
pub fn edit_nth(tab: &Tab, reg: &Registry, state: &mut CanvasState, id: SquareId, n: Option<usize>) -> bool {
    let list = fields(tab, reg, id);
    let pick = match n {
        Some(n) => list.get(n.saturating_sub(1)),
        None => list.iter().find(|f| f.1 == FieldKind::Typed).or(list.first()),
    };
    match pick.map(|f| f.0.clone()) {
        Some(key) => {
            start_edit(tab, reg, state, &key);
            true
        }
        None => false,
    }
}

fn valid(check: &Check, text: &str, reg: &Registry) -> bool {
    match check {
        Check::Any => true,
        Check::Is(Type::Str) => true,
        Check::Is(t) => Value::parse(text, t).is_some(),
        Check::IsOrEmpty(t) => text.trim().is_empty() || Value::parse(text, t).is_some(),
        Check::Param(ParamKind::Int) => lineit_rt::value::parse_int(text.trim()).is_some(),
        Check::Param(ParamKind::Float) => lineit_rt::value::parse_float(text.trim()).is_some(),
        Check::Param(ParamKind::Type { allow_tick }) => {
            (*allow_tick && text.trim() == "tick") || Type::parse(text, &|n| reg.records.get(n).cloned()).is_some()
        }
        Check::Param(_) => true,
    }
}

fn write_field(tab: &mut Tab, key: &FieldKey, text: String) {
    if let Some(sq) = tab.canvas.square_mut(key.sq) {
        if key.param {
            set_param(sq, &key.name, text);
        } else {
            sq.fields.insert(key.name.clone(), text);
        }
    }
}

/// Sets param NAME of SQ. A code square whose language changes while its
/// code is the old language's template gets the new one's.
fn set_param(sq: &mut Square, name: &str, value: String) {
    if sq.module == CODE && name == "language" {
        let (from, code) = (sq.params.get("language").map(String::as_str), sq.params.get("code").map(String::as_str));
        if let Some(t) = lineit_core::registry::code::switched_template(from, &value, code) {
            sq.params.insert("code".into(), t);
        }
    }
    sq.params.insert(name.into(), value);
}

/// Applies what's typed: true if it was valid (then the field closes).
fn apply(tab: &mut Tab, reg: &Registry, state: &mut CanvasState) -> bool {
    let Some(e) = &mut state.edit else { return true };
    if e.choose {
        state.edit = None;
        return true;
    }
    if !valid(&e.check, &e.text, reg) {
        e.error = true;
        return false;
    }
    let (key, text) = (e.key.clone(), e.text.clone());
    state.edit = None;
    write_field(tab, &key, text);
    true
}

/// Typing goes on in the square's next field (DIR -1: the one before),
/// round.
fn edit_next(tab: &Tab, reg: &Registry, state: &mut CanvasState, key: &FieldKey, dir: i32) {
    let list = fields(tab, reg, key.sq);
    let Some(i) = list.iter().position(|f| f.0 == *key) else { return };
    let n = list.len() as i32;
    let next = list[((i as i32 + dir).rem_euclid(n)) as usize].0.clone();
    start_edit(tab, reg, state, &next);
}

/// The keys of the field being typed in. Before anything else reads the
/// keyboard, so Enter, Tab and Esc are the field's.
pub fn edit_keys(ctx: &egui::Context, tab: &mut Tab, reg: &Registry, state: &mut CanvasState) {
    let Some(e) = &state.edit else { return };
    let (key, choose, multiline) = (e.key.clone(), e.choose, e.multiline);
    let k = |m: egui::Modifiers, k: egui::Key| ctx.input_mut(|i| i.consume_key(m, k));
    let none = egui::Modifiers::NONE;
    // in code, Tab is a tab
    let code = multiline && tab.canvas.square(key.sq).is_some_and(|s| s.module == CODE);
    let back = !code && k(egui::Modifiers::SHIFT, egui::Key::Tab);
    let tab_key = !code && !back && k(none, egui::Key::Tab);
    // Esc applies what's typed, as Enter does; if it isn't valid the field
    // goes back to what it was
    if k(none, egui::Key::Escape) {
        if !apply(tab, reg, state) {
            state.edit = None;
        }
        return;
    }
    if choose {
        if k(none, egui::Key::Space) || k(none, egui::Key::Enter) {
            let list = fields(tab, reg, key.sq);
            if let Some((_, kind, _, _, now)) = list.into_iter().find(|f| f.0 == key) {
                let next = match kind {
                    FieldKind::Bool => (now.trim() != "true").to_string(),
                    FieldKind::Choice(c) => {
                        let i = c.iter().position(|x| *x == now).map_or(0, |i| (i + 1) % c.len().max(1));
                        c.get(i).cloned().unwrap_or(now)
                    }
                    FieldKind::Typed => now,
                };
                write_field(tab, &key, next);
            }
        }
        if tab_key || back {
            edit_next(tab, reg, state, &key, if back { -1 } else { 1 });
        }
        return;
    }
    // a note's Enter is a new line: Esc or a click away keeps it
    if multiline {
        return;
    }
    if k(none, egui::Key::Enter) {
        apply(tab, reg, state);
    } else if (tab_key || back) && apply(tab, reg, state) {
        edit_next(tab, reg, state, &key, if back { -1 } else { 1 });
    }
}

/// A field that isn't being typed in: a box with its text.
fn draw_field_box(painter: &egui::Painter, r: Rect, text: &str, font: FontId, theme: &Theme, chosen: bool) {
    painter.rect_filled(r, 0.0, theme.field);
    let stroke = if chosen { Stroke::new(2.0, theme.step) } else { Stroke::new(1.0, theme.border) };
    painter.rect_stroke(r, 0.0, stroke, StrokeKind::Inside);
    let clip = painter.with_clip_rect(r.shrink(2.0).intersect(painter.clip_rect()));
    clip.text(r.left_center() + vec2(4.0, 0.0), Align2::LEFT_CENTER, text, font, theme.text);
}

/// A checkbox: a box, filled when ON.
fn draw_check(painter: &egui::Painter, r: Rect, on: bool, theme: &Theme, chosen: bool) {
    let b = Rect::from_center_size(pos2(r.left() + r.height() / 2.0, r.center().y), Vec2::splat(r.height() * 0.8));
    painter.rect_filled(b, 0.0, theme.field);
    let stroke = if chosen { Stroke::new(2.0, theme.step) } else { Stroke::new(1.0, theme.border) };
    painter.rect_stroke(b, 0.0, stroke, StrokeKind::Inside);
    if on {
        painter.rect_filled(b.shrink(b.width() * 0.25), 0.0, theme.accent);
    }
}

pub struct Ctx<'a> {
    pub tab: &'a mut Tab,
    pub tab_id: u64,
    pub reg: &'a Registry,
    pub theme: &'a Theme,
    pub cfg: &'a Config,
    pub state: &'a mut CanvasState,
    pub actions: Vec<Action>,
    /// Values of the run, if there's one.
    pub values: HashMap<(SquareId, String), Value>,
    /// Squares the run is stopped before.
    pub current: BTreeSet<SquareId>,
    /// What code squares keep, when the run last paused or ended.
    pub states: HashMap<SquareId, Vec<(String, Value)>>,
    /// How each `test/expect` went, the last time the run got to it.
    pub checks: HashMap<SquareId, lineit_core::interp::CheckResult>,
    /// The variables of the run, and their values when it last paused or
    /// ended (none: no run).
    pub vars: Vec<(String, Value)>,
    /// The wire the module search was opened with, by dropping it on the
    /// canvas, and where it was dropped: drawn until the search closes.
    pub pending: Option<(Pending, Pos2)>,
}

/// The curve of a wire from A to B, as the C editor drew it: its control
/// points half the larger of the two distances out, along the pins.
fn bezier(a: Pos2, b: Pos2, _zoom: f32) -> [Pos2; 4] {
    let d = (b.x - a.x).abs().max((b.y - a.y).abs()) * 0.5;
    [a, a + vec2(d, 0.0), b - vec2(d, 0.0), b]
}

/// A pin as the C editor drew it: a dark circle, with a dot of its color if
/// it's wired and a ring if not. Ticks too.
fn draw_pin(painter: &egui::Painter, p: Pos2, r: f32, color: Color32, border: Color32, wired: bool) {
    painter.circle_filled(p, r, border);
    if wired {
        painter.circle_filled(p, r * 0.7, color);
    } else {
        painter.circle_stroke(p, r * 0.575, Stroke::new(r * 0.25, color));
    }
}

fn bezier_point(p: &[Pos2; 4], t: f32) -> Pos2 {
    let u = 1.0 - t;
    let v = p[0].to_vec2() * (u * u * u)
        + p[1].to_vec2() * (3.0 * u * u * t)
        + p[2].to_vec2() * (3.0 * u * t * t)
        + p[3].to_vec2() * (t * t * t);
    v.to_pos2()
}

fn distance_to_bezier(p: &[Pos2; 4], q: Pos2) -> f32 {
    let mut best = f32::INFINITY;
    let mut prev = p[0];
    for i in 1..=32 {
        let cur = bezier_point(p, i as f32 / 32.0);
        let seg = cur - prev;
        let t = ((q - prev).dot(seg) / seg.length_sq().max(1e-6)).clamp(0.0, 1.0);
        best = best.min((prev + seg * t).distance(q));
        prev = cur;
    }
    best
}

fn draw_wire(painter: &egui::Painter, a: Pos2, b: Pos2, color: Color32, width: f32, zoom: f32) {
    let pts = bezier(a, b, zoom);
    painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
        pts,
        false,
        Color32::TRANSPARENT,
        Stroke::new(width, color),
    ));
}

/// What a square shows below its params: a value or an image, and how tall.
fn extra_height(painter: &egui::Painter, sq: &Square, info: Option<&SquareInfo>, cx: &Ctx, width: f32) -> f32 {
    if sq.module == "io/display" {
        return ROW;
    }
    if sq.module == CODE {
        // and a line for its tests
        return code_galley(painter, &code_now(sq, cx), 1.0, Color32::WHITE).size().y.max(ROW) + 12.0 + ROW;
    }
    if !cx.cfg.bool("canvas.previews") {
        return 0.0;
    }
    if let Some(info) = info {
        for (o, t) in info.out_types.iter().enumerate() {
            if t.as_ref() == Some(&Type::Image)
                && let Some((_, img)) = cx.state.previews.get(&(sq.id, info.sig.outputs[o].name.clone()))
                && img.width > 0
            {
                let w = width - 16.0;
                return (w * img.height as f32 / img.width as f32).min(160.0) + 6.0;
            }
        }
    }
    0.0
}

/// How wide a square is: as its title, its rows (an input's label and
/// field, an output's label) and its params need, at most MAX. Squares
/// that show something (images, a value) take MAX.
fn square_width(painter: &egui::Painter, sq: &Square, info: Option<&SquareInfo>, params: usize, max: f32) -> f32 {
    let w =
        |t: &str, size: f32| painter.layout_no_wrap(t.to_string(), FontId::proportional(size), Color32::WHITE).size().x;
    let mut need = w(&short(&sq.module, 30), 13.0) + 16.0;
    if let Some(info) = info {
        if sq.module == "io/display" || info.out_types.iter().any(|t| t.as_ref() == Some(&Type::Image)) {
            return max;
        }
        for i in 0..info.sig.inputs.len().max(info.sig.outputs.len()) {
            // fields are counted wired or not, so wiring doesn't resize it
            let left = info.sig.inputs.get(i).map_or(0.0, |p| {
                // a field as wide as its value, so it shows
                let field = p.field.as_ref().map_or(0.0, |d| {
                    let value = sq.fields.get(&p.name).unwrap_or(d);
                    let mono = painter.layout_no_wrap(value.clone(), FontId::monospace(12.0), Color32::WHITE).size().x;
                    6.0 + (mono + 12.0).max(FIELD_MIN)
                });
                10.0 + w(&p.name, 12.0) + field
            });
            let right = info.sig.outputs.get(i).map_or(0.0, |p| 10.0 + w(&p.name, 12.0));
            need = need.max(left + right + 16.0);
        }
    }
    if params > 0 {
        need = need.max(PARAM_LABEL + 90.0);
    }
    need.clamp(80.0, max)
}

/// The module of notes: post-its on the canvas, for people.
pub const NOTE: &str = "note";
const NOTE_FILL: Color32 = Color32::from_rgb(255, 236, 140);
const NOTE_TEXT: Color32 = Color32::from_rgb(52, 44, 20);

/// The fonts of the text that zooms with the canvas (notes and code),
/// without hinting (see `apply_style`).
pub const ZOOMED_TEXT: &str = "zoomed";
pub const ZOOMED_MONO: &str = "zoomed-mono";

fn zoomed_text(size: f32) -> FontId {
    FontId::new(size, egui::FontFamily::Name(ZOOMED_TEXT.into()))
}

fn zoomed_mono(size: f32) -> FontId {
    FontId::new(size, egui::FontFamily::Name(ZOOMED_MONO.into()))
}

/// The size the text of zoom Z is laid out at: a power of two at or above
/// it, scaled to Z when drawn ([`paint_scaled`]). Laid out at Z itself,
/// egui rounds glyphs and rows to pixels at each zoom, and the text shakes
/// and grows in steps against its box.
fn laid_at(z: f32) -> f32 {
    2f32.powf(z.log2().ceil()).clamp(0.25, 8.0)
}

/// Draws G (laid out at [`laid_at`]) at POS, S times as big.
fn paint_scaled(painter: &egui::Painter, pos: Pos2, g: &egui::Galley, s: f32) {
    let [w, h] = painter.ctx().fonts(|f| f.font_image_size());
    let mut mesh = egui::Mesh::default();
    for row in &g.rows {
        let at = row.pos.to_vec2();
        let first = mesh.vertices.len() as u32;
        mesh.indices.extend(row.visuals.mesh.indices.iter().map(|i| i + first));
        mesh.vertices.extend(row.visuals.mesh.vertices.iter().map(|v| egui::epaint::Vertex {
            pos: pos + (v.pos.to_vec2() + at) * s,
            // in texels in the galley; the atlas's 0 to 1 in a mesh
            uv: pos2(v.uv.x / w as f32, v.uv.y / h as f32),
            color: v.color,
        }));
    }
    painter.add(mesh);
}

/// G with everything in it S times as big: rows, glyphs, mesh. What a
/// layout at a bigger size, drawn smaller, looks like, with the cursor and
/// the clicks of a text box right at the new size.
fn scaled(g: std::sync::Arc<egui::Galley>, s: f32) -> std::sync::Arc<egui::Galley> {
    let mut shape = egui::epaint::TextShape::new(Pos2::ZERO, g, Color32::PLACEHOLDER);
    shape.transform(egui::emath::TSTransform::from_scaling(s));
    let mut g = shape.galley;
    for placed in &mut std::sync::Arc::make_mut(&mut g).rows {
        for gl in &mut std::sync::Arc::make_mut(&mut placed.row).glyphs {
            gl.pos *= s;
            gl.advance_width *= s;
            gl.line_height *= s;
            gl.font_ascent *= s;
            gl.font_height *= s;
            gl.font_face_ascent *= s;
            gl.font_face_height *= s;
        }
    }
    g
}

/// G moved so that, when egui rounds its position POS to a pixel (as it
/// does to all text), it ends up at POS itself: where [`paint_scaled`]
/// draws, which doesn't round. The rows move (what the cursor, the
/// selection and the clicks go by), and the bounds with them; the size
/// stays. The text box puts the galley at POS (see the test).
fn unrounded(g: std::sync::Arc<egui::Galley>, pos: Pos2, ppp: f32) -> std::sync::Arc<egui::Galley> {
    let by = pos - pos.round_to_pixels(ppp);
    let mut g = g;
    let inner = std::sync::Arc::make_mut(&mut g);
    for placed in &mut inner.rows {
        placed.pos += by;
    }
    inner.mesh_bounds = inner.mesh_bounds.translate(by);
    g
}

/// Galleys laid out lately (in egui's memory): a frame like the last one,
/// and each time the text box asks again, doesn't lay out, color and scale
/// again. What a galley is of: its kind, its text, its language, and the
/// numbers it depends on (sizes, positions, colors).
#[derive(Clone, Default)]
struct Laid(Vec<(u8, String, String, Vec<u32>, std::sync::Arc<egui::Galley>)>);

/// The most galleys [`Laid`] keeps.
const LAID_KEPT: usize = 24;

fn laid(
    ctx: &egui::Context,
    (kind, text, lang): (u8, &str, &str),
    nums: &[u32],
    make: impl FnOnce() -> std::sync::Arc<egui::Galley>,
) -> std::sync::Arc<egui::Galley> {
    let id = Id::unique("laid");
    let hit = ctx.data_mut(|d| {
        let all = d.get_temp_mut_or_default::<Laid>(id);
        all.0.iter().find(|(k, t, l, n, _)| *k == kind && t == text && l == lang && n == nums).map(|e| e.4.clone())
    });
    if let Some(g) = hit {
        return g;
    }
    let g = make();
    ctx.data_mut(|d| {
        let all = d.get_temp_mut_or_default::<Laid>(id);
        if all.0.len() >= LAID_KEPT {
            all.0.remove(0);
        }
        all.0.push((kind, text.to_string(), lang.to_string(), nums.to_vec(), g.clone()));
    });
    g
}

/// The colors of a theme that code is colored with, as numbers for [`laid`].
fn theme_nums(t: &Theme) -> Vec<u32> {
    [t.text, t.dim, t.accent, t.str, t.int, t.float, t.record, t.bool]
        .iter()
        .map(|c| u32::from_le_bytes(c.to_array()))
        .collect()
}

/// A note's text TEXT, laid out at B times the size of zoom 1 with the
/// lines it has at zoom 1 in a note WIDTH wide, and that galley is of TEXT
/// (soft breaks are rows that don't end with a newline, and its job is
/// TEXT in one section). Drawn scaled by Z / B, whether shown or typed in.
fn note_laid(painter: &egui::Painter, text: &str, width: f32, b: f32, color: Color32) -> std::sync::Arc<egui::Galley> {
    let nums = [width.to_bits(), b.to_bits(), u32::from_le_bytes(color.to_array())];
    laid(painter.ctx(), (0, text, ""), &nums, || {
        let one = painter.layout(text.to_string(), zoomed_text(14.0), color, width - 20.0);
        let lines: Vec<String> = one.rows.iter().map(|r| r.row.text()).collect();
        let mut g = painter.layout_no_wrap(lines.join("\n"), zoomed_text(14.0 * b), color);
        let inner = std::sync::Arc::make_mut(&mut g);
        for (row, at) in inner.rows.iter_mut().zip(&one.rows) {
            row.ends_with_newline = at.ends_with_newline;
        }
        // the job of the text, not of the lines joined: its sections
        // must be of the text the galley is of
        let job = std::sync::Arc::make_mut(&mut inner.job);
        job.text = text.to_string();
        job.sections.truncate(1);
        if let Some(s) = job.sections.first_mut() {
            s.byte_range = egui::text::ByteIndex(0)..egui::text::ByteIndex(text.len());
        }
        g
    })
}

/// A note's text, as laid out at zoom 1 in a note WIDTH wide.
fn note_galley(painter: &egui::Painter, text: &str, width: f32, z: f32) -> std::sync::Arc<egui::Galley> {
    let (shown, color) = note_shown(text);
    let nums = [width.to_bits(), z.to_bits()];
    laid(painter.ctx(), (1, shown, ""), &nums, || {
        painter.layout(shown.to_string(), zoomed_text(14.0 * z), color, (width - 20.0) * z)
    })
}

/// What a note shows for its text TEXT, and in what color: a hint if empty.
fn note_shown(text: &str) -> (&str, Color32) {
    if text.is_empty() { ("Double click to write", NOTE_TEXT.gamma_multiply(0.5)) } else { (text, NOTE_TEXT) }
}

/// The module of code squares: their code is drawn below their params,
/// and typed there.
pub const CODE: &str = lineit_core::registry::code::PATH;

/// A code square's code, as laid out: a line each, as long as it is.
fn code_galley(painter: &egui::Painter, text: &str, z: f32, color: Color32) -> std::sync::Arc<egui::Galley> {
    painter.layout_no_wrap(text.trim_end().to_string(), zoomed_mono(12.0 * z), color)
}

/// A code square's code in LANG, colored (and, from a file, the line that
/// says which first, dim).
fn code_colored(
    painter: &egui::Painter,
    text: &str,
    lang: &str,
    from_file: bool,
    z: f32,
    theme: &Theme,
) -> std::sync::Arc<egui::Galley> {
    let mut nums = vec![from_file as u32, z.to_bits()];
    nums.extend(theme_nums(theme));
    laid(painter.ctx(), (2, text.trim_end(), lang), &nums, || {
        painter.layout_job(code_job(text.trim_end(), lang, from_file, z, theme))
    })
}

/// The job of [`code_colored`], of TEXT as it is (it's what's typed, when
/// it's typed in).
fn code_job(text: &str, lang: &str, from_file: bool, z: f32, theme: &Theme) -> egui::text::LayoutJob {
    use lineit_core::registry::code::{Paint, highlight};
    let font = zoomed_mono(12.0 * z);
    let mut job = egui::text::LayoutJob::default();
    let fmt = |color: Color32| egui::TextFormat { font_id: font.clone(), color, ..Default::default() };
    let (head, code) = match text.find("\n\n") {
        Some(i) if from_file => text.split_at(i + 2),
        _ => ("", text),
    };
    job.append(head, 0.0, fmt(theme.dim));
    let paint = |p: Paint| match p {
        Paint::Keyword => theme.accent,
        Paint::String => theme.str,
        Paint::Comment => theme.dim,
        Paint::Number => theme.int,
        Paint::Function => theme.float,
        Paint::Type => theme.record,
        Paint::Constant => theme.bool,
        Paint::Operator => theme.text,
    };
    let mut at = 0;
    for (range, p) in highlight(lang, code) {
        if range.start < at || range.end > code.len() {
            continue;
        }
        job.append(&code[at..range.start], 0.0, fmt(theme.text));
        job.append(&code[range.clone()], 0.0, fmt(paint(p)));
        at = range.end;
    }
    job.append(&code[at..], 0.0, fmt(theme.text));
    job
}

/// The line of the code a problem is on, if it says (`line 5: ...`).
fn problem_line(message: &str) -> Option<usize> {
    let (_, after) = message.split_once("line ")?;
    let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    if after[digits.len()..].starts_with(':') { digits.parse().ok().filter(|n| *n > 0) } else { None }
}

/// The widest a code square gets: longer lines are cut.
const CODE_MAX: f32 = 640.0;

/// The file a code square's code is in, if it's in one.
fn code_file(sq: &Square) -> Option<&str> {
    sq.params.get("file").map(|f| f.trim()).filter(|f| !f.is_empty())
}

/// The language of a code square.
fn code_lang(sq: &Square) -> String {
    sq.params.get("language").cloned().unwrap_or_else(lineit_core::registry::code::default_language)
}

/// What a code square shows: what's being typed in it, its code, or the
/// code of its file (as lowering read it), under a line that says which.
fn code_now(sq: &Square, cx: &Ctx) -> String {
    if let Some(file) = code_file(sq) {
        let info = cx.tab.lowered.squares.get(&sq.id);
        let code = info.and_then(|i| i.sig.code.as_ref()).map(|f| f.source.clone()).unwrap_or_default();
        return format!("from {file} (edited there)\n\n{code}");
    }
    match &cx.state.edit {
        Some(e) if e.key.sq == sq.id && e.key.name == "code" => e.text.clone(),
        _ => code_of(sq, cx.reg),
    }
}

/// The params of SQ drawn as rows: a code square's code isn't one.
fn param_rows<'m>(sq: &Square, m: &'m dyn lineit_core::registry::Module) -> Vec<&'m lineit_core::registry::ParamDef> {
    m.params().iter().filter(|d| !(sq.module == CODE && (d.name == "code" || d.name == "tests"))).collect()
}

/// The least width of a field next to its input.
const FIELD_MIN: f32 = 56.0;
/// Where the field of a param starts, from the left of its square.
const PARAM_LABEL: f32 = 70.0;

fn short(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

pub fn show(ui: &mut egui::Ui, cx: &mut Ctx) {
    let (rect, bg) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    cx.state.rect = Some(rect);
    let theme = cx.theme.clone();
    cx.state.tests.poll(ui.ctx());
    cx.state.tests.sync(cx.tab);
    let painter = ui.painter_at(rect);
    let zoom_min = cx.cfg.num("canvas.zoom_min") as f32;
    let zoom_max = cx.cfg.num("canvas.zoom_max") as f32;
    let cam = &mut cx.tab.canvas.camera;
    cam.zoom = cam.zoom.clamp(zoom_min, zoom_max);
    let mut view = View { origin: rect.center(), cam: vec2(cam.x, cam.y), zoom: cam.zoom };

    // zoom where the mouse is, pan with the right or middle button
    if let Some(p) = ui.input(|i| i.pointer.hover_pos()).filter(|p| rect.contains(*p) && bg.hovered() || bg.dragged()) {
        let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
        let factor = pinch * (scroll.y * 0.0015).exp();
        if (factor - 1.0).abs() > 1e-4 {
            let before = view.to_world(p);
            view.zoom = (view.zoom * factor).clamp(zoom_min, zoom_max);
            view.cam = before.to_vec2() - (p - view.origin) / view.zoom;
        }
        if scroll.x != 0.0 {
            view.cam.x -= scroll.x / view.zoom;
        }
    }
    if bg.dragged_by(egui::PointerButton::Secondary) || bg.dragged_by(egui::PointerButton::Middle) {
        view.cam -= bg.drag_delta() / view.zoom;
    }
    cam.x = view.cam.x;
    cam.y = view.cam.y;
    cam.zoom = view.zoom;
    let z = view.zoom;
    // a read-only tab (git's file, a diff) can't be changed
    let ro = cx.tab.label.is_some();

    painter.rect_filled(rect, 0.0, theme.background);
    if cx.cfg.bool("canvas.grid") {
        let step = cx.cfg.num("canvas.grid_step") as f32;
        let mut step_screen = step * z;
        let mut every = 8;
        while step_screen < 8.0 {
            step_screen *= 2.0;
            every = (every / 2).max(1);
        }
        let w0 = view.to_world(rect.min);
        let world_step = step_screen / z;
        let start_x = (w0.x / world_step).floor() as i64;
        let start_y = (w0.y / world_step).floor() as i64;
        let nx = (rect.width() / step_screen) as i64 + 2;
        let ny = (rect.height() / step_screen) as i64 + 2;
        for i in start_x..start_x + nx {
            let x = view.to_screen(pos2(i as f32 * world_step, 0.0)).x;
            let c = if i.rem_euclid(every) == 0 { theme.grid_major } else { theme.grid };
            painter.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(1.0, c));
        }
        for j in start_y..start_y + ny {
            let y = view.to_screen(pos2(0.0, j as f32 * world_step)).y;
            let c = if j.rem_euclid(every) == 0 { theme.grid_major } else { theme.grid };
            painter.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], Stroke::new(1.0, c));
        }
    }

    // what the canvas is about, in its corner
    if !cx.tab.canvas.doc.is_empty() {
        let galley = painter.layout(cx.tab.canvas.doc.clone(), FontId::proportional(13.0), theme.dim, 520.0);
        painter.galley(rect.left_top() + vec2(12.0, 10.0), galley, theme.dim);
    }

    let width = cx.cfg.num("canvas.square_width") as f32;
    let font = |size: f32| FontId::proportional(size * z);
    let mono = |size: f32| FontId::monospace(size * z);

    // geometry
    let mut geos: HashMap<SquareId, Geo> = HashMap::new();
    for sq in &cx.tab.canvas.squares {
        let info = cx.tab.lowered.squares.get(&sq.id);
        let params = cx.reg.get(&sq.module).map(|m| param_rows(sq, m.as_ref()).len()).unwrap_or(0);
        if sq.module == NOTE {
            // as tall as its text (what's being typed, while it's typed)
            let text = match &cx.state.edit {
                Some(e) if e.key.sq == sq.id => e.text.clone(),
                _ => sq.params.get("text").cloned().unwrap_or_default(),
            };
            let h = (note_galley(&painter, &text, width, 1.0).size().y + 20.0).max(60.0);
            let r = Rect::from_min_size(pos2(sq.x, sq.y), vec2(width, h));
            geos.insert(
                sq.id,
                Geo { rect: r, ins: vec![], outs: vec![], params_top: r.bottom(), extra_top: r.bottom() },
            );
            continue;
        }
        let mut w = square_width(&painter, sq, info, params, width);
        if sq.module == CODE {
            let code = code_galley(&painter, &code_now(sq, cx), 1.0, Color32::WHITE).size().x;
            w = w.max(code + 24.0).min(CODE_MAX);
        }
        let extra = extra_height(&painter, sq, info, cx, w);
        geos.insert(sq.id, geometry(sq, info, params, w, extra));
    }
    cx.state.rects = geos.iter().map(|(id, g)| (*id, g.rect)).collect();
    // a copy, so the canvas can change while the pins are looked up
    let infos = cx.tab.lowered.squares.clone();
    let pin_pos = |id: SquareId, name: &str, output: bool| -> Option<(Pos2, Option<Type>)> {
        let info = infos.get(&id)?;
        let g = geos.get(&id)?;
        if output {
            let i = info.sig.output_index(name)?;
            Some((g.outs[i], info.out_types[i].clone()))
        } else {
            let i = info.sig.input_index(name)?;
            Some((g.ins[i], info.in_types[i].clone()))
        }
    };

    // wires
    let wire_w = cx.cfg.num("canvas.wire_width") as f32 * z.max(0.5);
    let mut wire_curves: Vec<(Link, [Pos2; 4])> = Vec::new();
    for l in &cx.tab.canvas.links {
        let (Some((a, ty)), Some((b, _))) = (pin_pos(l.from, &l.output, true), pin_pos(l.to, &l.input, false)) else {
            continue;
        };
        let (a, b) = (view.to_screen(a), view.to_screen(b));
        let selected = cx.tab.selected_links.contains(l);
        let mut color = theme.of(ty.as_ref());
        // in a diff: a wire that's new or gone
        if let Some(d) = &cx.tab.diff {
            if d.links_added.contains(l) {
                color = theme.added;
            } else if d.links_removed.contains(l) {
                color = theme.removed;
            }
        }
        // two wires into one data input: a red halo, so a diff's colors still show
        if cx.tab.lowered.double.contains(l) {
            draw_wire(&painter, a, b, theme.error, wire_w + 7.0, z);
        }
        if selected {
            draw_wire(&painter, a, b, theme.selection, wire_w + 4.0, z);
        }
        draw_wire(&painter, a, b, color, wire_w, z);
        wire_curves.push((l.clone(), bezier(a, b, z)));
    }

    // what's wrong, by square
    let mut problems: HashMap<SquareId, Vec<String>> = HashMap::new();
    for d in &cx.tab.lowered.diags {
        if let Some(s) = d.square {
            problems.entry(s).or_default().push(d.message.clone());
        }
    }
    let wired: BTreeSet<(SquareId, String)> = cx.tab.canvas.links.iter().map(|l| (l.to, l.input.clone())).collect();
    let wired_from: BTreeSet<(SquareId, String)> =
        cx.tab.canvas.links.iter().map(|l| (l.from, l.output.clone())).collect();
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let shift = ui.input(|i| i.modifiers.shift);

    // squares
    let ids: Vec<SquareId> = cx.tab.canvas.squares.iter().map(|s| s.id).collect();
    let mut move_by: Option<Vec2> = None;
    let mut snap = false;
    let mut field_changes: Vec<(SquareId, String, String, bool)> = Vec::new();
    let mut field_clicks: Vec<FieldKey> = Vec::new();
    for id in ids {
        let sq = cx.tab.canvas.square(id).unwrap().clone();
        let g = &geos[&id];
        let srect = view.rect(g.rect);
        if !srect.expand(40.0).intersects(rect) {
            continue;
        }
        let info = cx.tab.lowered.squares.get(&id).cloned();
        let module = cx.reg.get(&sq.module).cloned();
        let selected = cx.tab.selected.contains(&id);
        let r = CornerRadius::ZERO;
        let note = sq.module == NOTE;
        let editing_note = note && cx.state.edit.as_ref().is_some_and(|e| e.key.sq == id);
        let header = Rect::from_min_size(srect.min, vec2(srect.width(), HEADER * z));
        if note {
            // a post-it: yellow, a shadow, its text
            painter.rect_filled(srect.translate(vec2(3.0, 4.0) * z), r, Color32::from_black_alpha(70));
            painter.rect_filled(srect, r, NOTE_FILL);
            if !editing_note {
                let text = sq.params.get("text").cloned().unwrap_or_default();
                // its lines as at zoom 1, so they don't change with the zoom
                let (shown, color) = note_shown(&text);
                let b = laid_at(z);
                let g = note_laid(&painter, shown, srect.width() / z, b, color);
                paint_scaled(&painter, srect.min + vec2(10.0, 10.0) * z, &g, z / b);
            }
        } else {
            painter.rect_filled(srect, r, theme.square);
            painter.rect_filled(header, r, theme.header);
        }
        let border = if cx.current.contains(&id) {
            Stroke::new(3.0, theme.step)
        } else if problems.contains_key(&id) {
            Stroke::new(2.0, theme.error)
        } else if selected {
            Stroke::new(2.0, theme.selection)
        } else if note {
            Stroke::new(1.0, NOTE_FILL.gamma_multiply(0.7))
        } else {
            Stroke::new(2.0, theme.border)
        };
        painter.rect_stroke(srect, r, border, StrokeKind::Outside);
        // a code square is its function: its name, and its doc on hover
        let function = if sq.module == CODE { info.as_ref().and_then(|i| i.sig.code.clone()) } else { None };
        let title = match (&module, &function) {
            (_, Some(f)) => {
                let lang =
                    sq.params.get("language").cloned().unwrap_or_else(lineit_core::registry::code::default_language);
                format!("{} ({lang})", f.name)
            }
            (Some(_), None) => sq.module.clone(),
            (None, _) => format!("? {}", sq.module),
        };
        // what its header says on hover: the module's doc, or a code
        // square's function's, and what it keeps after a run
        let doc = match &function {
            Some(f) => {
                let mut tip = f.doc.clone();
                if let Some(st) = cx.states.get(&id) {
                    let kept: Vec<String> = st.iter().map(|(k, v)| format!("{k} = {}", short(&v.text(), 80))).collect();
                    tip = format!("{tip}\n\nIt keeps: {}", kept.join(", ")).trim().to_string();
                }
                tip
            }
            None => module.as_ref().map(|m| m.doc().to_string()).unwrap_or_default(),
        };
        if !note {
            painter.text(
                header.left_center() + vec2(8.0 * z, 0.0),
                Align2::LEFT_CENTER,
                short(&title, 30),
                font(13.0),
                theme.text,
            );
        }

        // the square itself: select, move, its menu
        let resp = ui.interact(srect, Id::unique(("square", cx.tab_id, id)), Sense::click_and_drag());
        if resp.drag_started_by(egui::PointerButton::Primary) || resp.clicked() {
            if shift {
                if resp.clicked() && selected {
                    cx.tab.selected.remove(&id);
                } else {
                    cx.tab.selected.insert(id);
                }
            } else if !selected || resp.clicked() {
                cx.tab.selected.clear();
                cx.tab.selected.insert(id);
                cx.tab.selected_links.clear();
            }
        }
        if resp.dragged_by(egui::PointerButton::Primary) {
            move_by = Some(resp.drag_delta() / z);
        }
        if resp.drag_stopped() {
            snap = true;
        }
        if resp.secondary_clicked() && !selected {
            cx.tab.selected.clear();
            cx.tab.selected.insert(id);
        }
        let resp = if !doc.is_empty() && header.contains(pointer.unwrap_or_default()) {
            resp.on_hover_text(doc)
        } else {
            resp
        };
        resp.context_menu(|ui| {
            // a test from now: its fields from the run, or from how the variables start
            if sq.module == "test/case"
                && let Some(info) = &info
            {
                let typed: Vec<(String, Type)> = info
                    .sig
                    .inputs
                    .iter()
                    .zip(&info.in_types)
                    .filter(|(p, _)| p.field.is_some())
                    .filter_map(|(p, t)| Some((p.name.clone(), t.clone()?)))
                    .collect();
                let from_run = !cx.vars.is_empty();
                let b = ui
                    .add_enabled(from_run, egui::Button::new("Fill from the run"))
                    .on_hover_text("Its fields get the values the variables have now in the run (paused, or ended)")
                    .on_disabled_hover_text(
                        "Run or step the canvas first: its fields get the values the variables have then",
                    );
                if b.clicked() {
                    for (name, _) in &typed {
                        if let Some((_, v)) = cx.vars.iter().find(|(n, _)| n == name) {
                            field_changes.push((id, name.clone(), v.text(), false));
                        }
                    }
                    ui.close();
                }
                if ui
                    .button("Fill from how they start")
                    .on_hover_text("Its fields get what the variables start with (their var/var)")
                    .clicked()
                {
                    for (name, ty) in &typed {
                        let decl = cx
                            .tab
                            .canvas
                            .squares
                            .iter()
                            .find(|s| s.module == "var/var" && s.params.get("name") == Some(name));
                        let initial =
                            decl.and_then(|d| d.params.get("initial")).filter(|t| !t.trim().is_empty()).cloned();
                        let text =
                            initial.unwrap_or_else(|| lineit_core::registry::builtin::default_text(ty).to_string());
                        field_changes.push((id, name.clone(), text, false));
                    }
                    ui.close();
                }
                ui.separator();
            }
            if ui.button("Duplicate (Ctrl+D)").clicked() {
                cx.actions.push(Action::Duplicate);
                ui.close();
            }
            if ui.button("Copy (Ctrl+C)").clicked() {
                cx.actions.push(Action::Copy);
                ui.close();
            }
            if ui.button("Disconnect").clicked() {
                disconnect(cx.tab);
                ui.close();
            }
            if ui.button("Make subgraph (Ctrl+G)").clicked() {
                cx.actions.push(Action::MakeSubgraph);
                ui.close();
            }
            if module.as_ref().is_some_and(|m| m.subgraph().is_some()) && ui.button("Open subgraph").clicked() {
                cx.actions.push(Action::OpenModule(sq.module.clone()));
                ui.close();
            }
            ui.separator();
            if ui.button("Delete (Del)").clicked() {
                cx.actions.push(Action::Delete);
                ui.close();
            }
        });

        if note {
            if resp.double_clicked() && !editing_note {
                if !apply(cx.tab, cx.reg, cx.state) {
                    cx.state.edit = None;
                }
                start_edit(cx.tab, cx.reg, cx.state, &FieldKey { sq: id, name: "text".into(), param: true });
            }
            if let Some(e) = cx.state.edit.as_mut().filter(|e| e.key.sq == id) {
                note_edit(ui, e, srect, z, cx.tab_id);
            }
            continue;
        }

        // pins, and the fields of unwired inputs
        let show_widgets = z >= EDIT_ZOOM;
        if let Some(info) = &info {
            for (i, pin) in info.sig.inputs.iter().enumerate() {
                let p = view.to_screen(g.ins[i]);
                let ty = info.in_types[i].clone();
                let color = theme.of(ty.as_ref());
                let is_wired = wired.contains(&(id, pin.name.clone()));
                draw_pin(&painter, p, PIN * z, color, theme.pin_border, is_wired);
                let label = painter.layout_no_wrap(pin.name.clone(), font(12.0), theme.dim);
                let label_w = label.size().x;
                painter.galley(p + vec2(10.0 * z, -label.size().y / 2.0), label, theme.dim);
                let pin_resp = ui.interact(
                    Rect::from_center_size(p, Vec2::splat(PIN * z * 3.0)),
                    Id::unique(("in", cx.tab_id, id, i)),
                    Sense::drag(),
                );
                let mut tip =
                    format!("{}: {}", pin.name, ty.as_ref().map(|t| t.to_string()).unwrap_or_else(|| "tick".into()));
                if let Some(v) = cx.values.get(&(id, pin.name.clone())) {
                    tip.push_str(&format!(" = {}", short(&v.text(), 200)));
                }
                let pin_resp = pin_resp.on_hover_text(tip);
                if pin_resp.drag_started() && !ro {
                    // a plugged input gives its wire back, to put elsewhere
                    let link = cx.tab.canvas.links.iter().rev().find(|l| l.to == id && l.input == pin.name).cloned();
                    match link {
                        Some(l) => {
                            cx.tab.canvas.links.retain(|x| *x != l);
                            let ty = pin_pos(l.from, &l.output, true).and_then(|x| x.1);
                            cx.state.drag = Some(Drag::Wire { from: (l.from, l.output), ty, picked: true });
                        }
                        None => cx.state.drag = Some(Drag::WireBack { to: (id, pin.name.clone()), ty: ty.clone() }),
                    }
                }
                // its field
                if let (Some(default), false) = (&pin.field, is_wired) {
                    let left = p.x + 10.0 * z + label_w + 6.0 * z;
                    // up to the label of the output on its row, if there's one
                    let right = match info.sig.outputs.get(i) {
                        Some(o) => {
                            let ow = painter.layout_no_wrap(o.name.clone(), font(12.0), theme.dim).size().x;
                            srect.right() - 10.0 * z - ow - 10.0 * z
                        }
                        None => srect.right() - 10.0 * z,
                    };
                    if right - left < 16.0 * z {
                        continue;
                    }
                    let frect = Rect::from_min_max(pos2(left, p.y - 9.0 * z), pos2(right, p.y + 9.0 * z));
                    let current = sq.fields.get(&pin.name).cloned().unwrap_or_else(|| default.clone());
                    let key = FieldKey { sq: id, name: pin.name.clone(), param: false };
                    let kind = if ty == Some(Type::Bool) { FieldKind::Bool } else { FieldKind::Typed };
                    field(
                        ui,
                        &painter,
                        (&mut cx.state.edit, &theme, cx.tab_id),
                        key,
                        kind,
                        frect,
                        &current,
                        mono(12.0),
                        z,
                        &mut field_changes,
                        &mut field_clicks,
                    );
                }
            }
            for (o, pin) in info.sig.outputs.iter().enumerate() {
                let p = view.to_screen(g.outs[o]);
                let ty = info.out_types[o].clone();
                let color = theme.of(ty.as_ref());
                let out_wired = wired_from.contains(&(id, pin.name.clone()));
                draw_pin(&painter, p, PIN * z, color, theme.pin_border, out_wired);
                painter.text(p - vec2(10.0 * z, 0.0), Align2::RIGHT_CENTER, &pin.name, font(12.0), theme.dim);
                let pin_resp = ui.interact(
                    Rect::from_center_size(p, Vec2::splat(PIN * z * 3.0)),
                    Id::unique(("out", cx.tab_id, id, o)),
                    Sense::drag(),
                );
                let mut tip =
                    format!("{}: {}", pin.name, ty.as_ref().map(|t| t.to_string()).unwrap_or_else(|| "tick".into()));
                if let Some(v) = cx.values.get(&(id, pin.name.clone())) {
                    tip.push_str(&format!(" = {}", short(&v.text(), 200)));
                }
                let pin_resp = pin_resp.on_hover_text(tip);
                if pin_resp.drag_started() {
                    cx.state.drag = Some(Drag::Wire { from: (id, pin.name.clone()), ty: ty.clone(), picked: false });
                }
            }
        }
        // its params and what it shows, also when its pins can't be said:
        // they're how what's wrong with it gets fixed
        {
            // params
            if let Some(m) = &module {
                for (k, def) in param_rows(&sq, m.as_ref()).into_iter().enumerate() {
                    let y = view.to_screen(pos2(0.0, g.params_top + ROW * k as f32 + ROW / 2.0)).y;
                    let x0 = srect.left() + 8.0 * z;
                    painter.text(pos2(x0, y), Align2::LEFT_CENTER, &def.name, font(12.0), theme.dim);
                    let frect = Rect::from_min_max(
                        pos2(srect.left() + 70.0 * z, y - 9.0 * z),
                        pos2(srect.right() - 8.0 * z, y + 9.0 * z),
                    );
                    let current = sq.params.get(&def.name).cloned().unwrap_or_else(|| def.default.clone());
                    if !show_widgets {
                        painter.rect_filled(frect, 2.0, theme.field);
                        continue;
                    }
                    let choices: Option<Vec<String>> = match &def.kind {
                        ParamKind::Choice(c) => Some(c.clone()),
                        ParamKind::Function { args } => Some(cx.reg.functions(args.len())),
                        ParamKind::Bool => None,
                        _ => None,
                    };
                    if let Some(choices) = choices {
                        let mut chosen = current.clone();
                        ui.put(frect, |ui: &mut egui::Ui| {
                            egui::ComboBox::from_id_salt(("param", cx.tab_id, id, k))
                                .selected_text(
                                    egui::RichText::new(if chosen.is_empty() { "(pick one)" } else { &chosen })
                                        .font(mono(12.0)),
                                )
                                .width(frect.width())
                                .show_ui(ui, |ui| {
                                    for c in &choices {
                                        ui.selectable_value(&mut chosen, c.clone(), c);
                                    }
                                })
                                .response
                        });
                        if chosen != current {
                            field_changes.push((id, def.name.clone(), chosen, true));
                        }
                    } else {
                        let key = FieldKey { sq: id, name: def.name.clone(), param: true };
                        let kind = if def.kind == ParamKind::Bool { FieldKind::Bool } else { FieldKind::Typed };
                        field(
                            ui,
                            &painter,
                            (&mut cx.state.edit, &theme, cx.tab_id),
                            key,
                            kind,
                            frect,
                            &current,
                            mono(12.0),
                            z,
                            &mut field_changes,
                            &mut field_clicks,
                        );
                    }
                }
            }

            // what it shows
            let extra_top = view.to_screen(pos2(0.0, g.extra_top)).y;
            if sq.module == CODE {
                let r = Rect::from_min_max(
                    pos2(srect.left() + 6.0 * z, extra_top + 2.0 * z),
                    pos2(srect.right() - 6.0 * z, srect.bottom() - (6.0 + ROW) * z),
                );
                // its tests: how they went; a click opens their table
                use super::tests::Badge;
                let badge = cx.state.tests.badge(cx.tab, id);
                let why = if let Badge::Failed(e) = &badge { Some(e.clone()) } else { None };
                let (label, color) = match badge {
                    Badge::None => ("add tests".to_string(), theme.dim),
                    Badge::Running => ("tests running...".to_string(), theme.dim),
                    Badge::NotRun(n) => (format!("{n} test{}, not run", if n == 1 { "" } else { "s" }), theme.dim),
                    Badge::Ran(p, n) if p == n => {
                        (format!("tests: {p} of {n} passed"), Color32::from_rgb(110, 190, 110))
                    }
                    Badge::Ran(p, n) => (format!("tests: {} of {n} failed", n - p), theme.error),
                    Badge::Failed(_) => ("tests can't run".to_string(), theme.error),
                };
                let line = Rect::from_min_max(pos2(r.left(), r.bottom()), pos2(r.right(), srect.bottom() - 2.0 * z));
                painter.text(line.left_center() + vec2(2.0 * z, 0.0), Align2::LEFT_CENTER, label, font(12.0), color);
                let mut resp = ui.interact(line, Id::unique(("tests", cx.tab_id, id)), Sense::click());
                if let Some(why) = why {
                    resp = resp.on_hover_text(why);
                }
                if resp.clicked() {
                    if cx.state.tests.open == Some(id) {
                        cx.state.tests.open = None;
                    } else {
                        cx.state.tests.open = Some(id);
                        if matches!(cx.state.tests.badge(cx.tab, id), Badge::NotRun(_)) {
                            cx.state.tests.run(cx.tab, id);
                        }
                    }
                }
                painter.rect_filled(r, 0.0, theme.field);
                match cx.state.edit.as_mut().filter(|e| e.key.sq == id && e.key.name == "code") {
                    Some(e) if show_widgets => {
                        let lang = code_lang(&sq);
                        code_edit(ui, e, r, z, cx.tab_id, &lang, &theme)
                    }
                    _ => {
                        let from_file = code_file(&sq).is_some();
                        let lang = code_lang(&sq);
                        let b = laid_at(z);
                        let g = code_colored(&painter, &code_now(&sq, cx), &lang, from_file, b, &theme);
                        let clip = painter.with_clip_rect(r.intersect(painter.clip_rect()));
                        // the line a problem is on: a bar beside it
                        let lines = g.rows.len().max(1) as f32;
                        let line_h = g.size().y * z / b / lines;
                        let skip = if from_file { 2.0 } else { 0.0 };
                        for line in problems.get(&id).into_iter().flatten().filter_map(|m| problem_line(m)) {
                            let y = r.min.y + 4.0 * z + (line as f32 - 1.0 + skip) * line_h;
                            clip.rect_filled(
                                Rect::from_min_size(pos2(r.min.x, y), vec2(r.width(), line_h)),
                                0.0,
                                theme.error.gamma_multiply(0.25),
                            );
                        }
                        paint_scaled(&clip, r.min + vec2(2.0, 4.0) * z, &g, z / b);
                        let resp = ui.interact(r, Id::unique(("code", cx.tab_id, id)), Sense::click());
                        // code in a file is typed in the file
                        if resp.clicked() && show_widgets && !from_file {
                            field_clicks.push(FieldKey { sq: id, name: "code".into(), param: true });
                        }
                    }
                }
            } else if sq.module == "io/display" {
                let text = cx.values.get(&(id, "value".to_string())).map(|v| v.text()).unwrap_or_else(|| "—".into());
                painter.text(
                    pos2(srect.left() + 8.0 * z, extra_top + ROW * z / 2.0),
                    Align2::LEFT_CENTER,
                    short(&text.replace('\n', " "), 40),
                    mono(13.0),
                    theme.text,
                );
            } else if let Some(info) = &info {
                for (o, pin) in info.sig.outputs.iter().enumerate() {
                    if info.out_types[o].as_ref() != Some(&Type::Image) {
                        continue;
                    }
                    if let Some((tex, img)) = cx.state.previews.get(&(id, pin.name.clone())) {
                        if img.width == 0 {
                            continue;
                        }
                        let w = srect.width() - 16.0 * z;
                        let h = (w * img.height as f32 / img.width as f32).min(160.0 * z);
                        let w = h * img.width as f32 / img.height as f32;
                        let r = Rect::from_min_size(pos2(srect.center().x - w / 2.0, extra_top + 3.0 * z), vec2(w, h));
                        painter.image(tex.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                        break;
                    }
                }
            }
        }

        // how its test went, the last time the tests ran: a click shows the report
        if sq.module == "test/case"
            && let Some(ok) = cx.state.tests.case_result(id)
            && !problems.contains_key(&id)
        {
            let (text, color) =
                if ok { ("passed", Color32::from_rgb(110, 190, 110)) } else { ("failed: see the report", theme.error) };
            let r =
                painter.text(srect.left_bottom() + vec2(2.0 * z, 4.0 * z), Align2::LEFT_TOP, text, font(12.0), color);
            if ui
                .interact(r, Id::unique(("case result", cx.tab_id, id)), Sense::click())
                .on_hover_text("Open the tests' report")
                .clicked()
            {
                cx.actions.push(Action::ShowReport);
            }
        }

        // a variable's value, while it runs
        if sq.module == "var/var"
            && let Some(v) = cx.values.get(&(id, "value".to_string()))
            && !problems.contains_key(&id)
        {
            painter.text(
                srect.left_bottom() + vec2(2.0 * z, 4.0 * z),
                Align2::LEFT_TOP,
                format!("= {}", short(&v.text(), 60)),
                font(12.0),
                theme.dim,
            );
        }

        // how its check went, in a test's run
        if let (Some(c), None) = (cx.checks.get(&id), problems.get(&id)) {
            let (text, color) = if c.ok {
                (format!("ok: {}", short(&c.got.text(), 60)), Color32::from_rgb(110, 190, 110))
            } else {
                (format!("expected {}, got {}", short(&c.expected.text(), 40), short(&c.got.text(), 40)), theme.error)
            };
            let galley = painter.layout(text, font(12.0), color, srect.width().max(200.0 * z));
            painter.galley(srect.left_bottom() + vec2(0.0, 4.0 * z), galley, color);
            if !c.ok {
                painter.rect_stroke(srect, 0.0, Stroke::new(2.0, theme.error), StrokeKind::Outside);
            }
        }

        // what's wrong with it
        if let Some(msgs) = problems.get(&id) {
            let text = msgs.join("\n");
            let galley = painter.layout(text, font(12.0), theme.error, srect.width());
            painter.galley(srect.left_bottom() + vec2(0.0, 4.0 * z), galley, theme.error);
        }
    }
    if cx.tab.label.is_none() {
        super::tests::window(ui.ctx(), cx.tab, &mut cx.state.tests, &theme);
    }

    // vim mode: the focus, and the pins to pick
    if cx.state.vim.on {
        let zz = z.max(0.7);
        if let Some(g) = cx.state.vim.focus.and_then(|f| geos.get(&f)) {
            let r = view.rect(g.rect).expand(5.0 * zz);
            painter.rect_stroke(r, 0.0, Stroke::new(2.0, theme.accent), StrokeKind::Outside);
            painter.rect_stroke(
                r.expand(3.0),
                0.0,
                Stroke::new(1.0, theme.accent.gamma_multiply(0.45)),
                StrokeKind::Outside,
            );
        }
        for h in &cx.state.vim.hints {
            let Some((p, _)) = pin_pos(h.pin.sq, &h.pin.name, h.pin.output) else { continue };
            let p = view.to_screen(p);
            let c = p + vec2(if h.pin.output { 18.0 } else { -18.0 } * zz, 0.0);
            let (fill, text) = if h.cursor { (theme.step, Color32::BLACK) } else { (theme.accent, Color32::BLACK) };
            painter.circle_filled(c, 9.0 * zz, fill);
            painter.text(c, Align2::CENTER_CENTER, &h.label, FontId::monospace(12.0 * zz), text);
        }
    }
    for (id, name, value, is_param) in field_changes {
        if let Some(sq) = cx.tab.canvas.square_mut(id) {
            if is_param {
                set_param(sq, &name, value);
            } else {
                sq.fields.insert(name, value);
            }
        }
    }
    // a field clicked: typing starts there (what was typed elsewhere applies)
    if let Some(key) = field_clicks.pop() {
        if !apply(cx.tab, cx.reg, cx.state) {
            cx.state.edit = None;
        }
        start_edit(cx.tab, cx.reg, cx.state, &key);
        if let Some(e) = &mut cx.state.edit
            && e.choose
        {
            // a click on a checkbox toggles it at once
            e.fresh = false;
        }
    } else if let Some(e) = &cx.state.edit {
        // a click anywhere else applies what's valid, and the click goes on
        let away = ui.input(|i| i.pointer.any_pressed())
            && ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| {
                !e.rect.is_some_and(|r| r.contains(p)) && !cx.state.strip.is_some_and(|r| r.contains(p))
            });
        if away && !apply(cx.tab, cx.reg, cx.state) {
            cx.state.edit = None;
        }
    }
    // a diff: what each square is, in the theme's colors; the ones that only moved fade
    if let Some(d) = &cx.tab.diff {
        for (id, mark) in &d.squares {
            let Some(g) = geos.get(id) else { continue };
            let r = view.rect(g.rect);
            if !r.expand(40.0).intersects(rect) {
                continue;
            }
            let (color, tag) = match mark {
                Mark::Added => (theme.added, "added"),
                Mark::Removed => (theme.removed, "removed"),
                Mark::Changed => (theme.changed, "changed"),
                Mark::Wired => (theme.changed.gamma_multiply(0.6), "wires changed"),
                Mark::Moved => {
                    painter.rect_filled(r, 0.0, theme.background.gamma_multiply(0.6));
                    continue;
                }
            };
            painter.rect_filled(r, 0.0, color.gamma_multiply(0.14));
            painter.rect_stroke(
                r,
                0.0,
                Stroke::new(if *mark == Mark::Wired { 2.0 } else { 3.0 }, color),
                StrokeKind::Outside,
            );
            let at = r.left_top() - vec2(0.0, 4.0 * z);
            painter.text(at, Align2::LEFT_BOTTOM, tag, font(12.0), color);
        }
    }
    if let Some(d) = move_by.filter(|_| !ro) {
        let sel = cx.tab.selected.clone();
        for sq in cx.tab.canvas.squares.iter_mut().filter(|s| sel.contains(&s.id)) {
            sq.x += d.x;
            sq.y += d.y;
        }
    }
    if snap && !ro && cx.cfg.bool("canvas.snap") {
        let step = cx.cfg.num("canvas.grid_step") as f32;
        let sel = cx.tab.selected.clone();
        for sq in cx.tab.canvas.squares.iter_mut().filter(|s| sel.contains(&s.id)) {
            sq.x = (sq.x / step).round() * step;
            sq.y = (sq.y / step).round() * step;
        }
    }

    // the canvas itself: select with a box, or a wire with a click
    if bg.drag_started_by(egui::PointerButton::Primary)
        && let Some(p) = bg.interact_pointer_pos()
    {
        cx.state.drag = Some(Drag::Select { start: view.to_world(p) });
        if !shift {
            cx.tab.selected.clear();
            cx.tab.selected_links.clear();
        }
    }
    if bg.clicked() {
        let p = bg.interact_pointer_pos().unwrap_or_default();
        let hit = wire_curves
            .iter()
            .map(|(l, c)| (l, distance_to_bezier(c, p)))
            .filter(|(_, d)| *d < 6.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(l, _)| l.clone());
        if !shift {
            cx.tab.selected.clear();
            cx.tab.selected_links.clear();
        }
        if let Some(l) = hit
            && !cx.tab.selected_links.remove(&l)
        {
            cx.tab.selected_links.insert(l);
        }
    }
    if bg.secondary_clicked()
        && let Some(p) = bg.interact_pointer_pos()
    {
        cx.actions.push(Action::Search { at: view.to_world(p), screen: p, pending: None });
    }

    // the wire the search is open for, to where it was dropped
    if let Some((p, at)) = &cx.pending {
        let at = view.to_screen(*at);
        match p {
            Pending::From(sq, out, ty) => {
                if let Some((a, _)) = pin_pos(*sq, out, true) {
                    draw_wire(&painter, view.to_screen(a), at, theme.of(ty.as_ref()), wire_w, z);
                }
            }
            Pending::To(sq, input, ty) => {
                if let Some((b, _)) = pin_pos(*sq, input, false) {
                    draw_wire(&painter, at, view.to_screen(b), theme.of(ty.as_ref()), wire_w, z);
                }
            }
        }
    }

    // what's being dragged
    let released = ui.input(|i| i.pointer.any_released());
    if let (Some(drag), Some(p)) = (&cx.state.drag, pointer.or_else(|| ui.input(|i| i.pointer.latest_pos()))) {
        match drag {
            Drag::Select { start } => {
                let r = Rect::from_two_pos(view.to_screen(*start), p);
                painter.rect_filled(r, 0.0, theme.selection.gamma_multiply(0.12));
                painter.rect_stroke(r, 0.0, Stroke::new(1.0, theme.selection), StrokeKind::Inside);
                if released {
                    let wr = Rect::from_two_pos(*start, view.to_world(p));
                    for (id, g) in &geos {
                        if wr.intersects(g.rect) {
                            cx.tab.selected.insert(*id);
                        }
                    }
                }
            }
            Drag::Wire { from, ty, .. } => {
                if let Some((a, _)) = pin_pos(from.0, &from.1, true) {
                    draw_wire(&painter, view.to_screen(a), p, theme.of(ty.as_ref()), wire_w, z);
                }
            }
            Drag::WireBack { to, ty } => {
                if let Some((b, _)) = pin_pos(to.0, &to.1, false) {
                    draw_wire(&painter, p, view.to_screen(b), theme.of(ty.as_ref()), wire_w, z);
                }
            }
        }
        if released {
            let drag = cx.state.drag.take().unwrap();
            // the pin under the mouse, if any
            let near = |output: bool| -> Option<(SquareId, String, bool)> {
                let mut best: Option<(f32, SquareId, String, bool)> = None;
                for sq in &cx.tab.canvas.squares {
                    let Some(info) = cx.tab.lowered.squares.get(&sq.id) else { continue };
                    let g = &geos[&sq.id];
                    let (pins, defs) = if output { (&g.outs, &info.sig.outputs) } else { (&g.ins, &info.sig.inputs) };
                    for (pp, def) in pins.iter().zip(defs) {
                        let d = view.to_screen(*pp).distance(p);
                        if d < 14.0 && best.as_ref().is_none_or(|b| d < b.0) {
                            best = Some((d, sq.id, def.name.clone(), def.ty == PinTy::Tick));
                        }
                    }
                }
                best.map(|b| (b.1, b.2, b.3))
            };
            match drag {
                Drag::Wire { from, ty, picked } if !ro => match near(false) {
                    Some((to, input, tick)) => {
                        if can_wire(ty.is_none(), tick, from.0, to) {
                            cx.tab.canvas.connect(Link { from: from.0, output: from.1, to, input }, !tick);
                        }
                    }
                    None if rect.contains(p) && !picked => cx.actions.push(Action::Search {
                        at: view.to_world(p),
                        screen: p,
                        pending: Some(Pending::From(from.0, from.1, ty)),
                    }),
                    None => {}
                },
                Drag::WireBack { to, ty } if !ro => match near(true) {
                    Some((from, output, tick)) => {
                        if can_wire(ty.is_none(), tick, from, to.0) {
                            cx.tab.canvas.connect(Link { from, output, to: to.0, input: to.1 }, !tick);
                        }
                    }
                    None if rect.contains(p) => cx.actions.push(Action::Search {
                        at: view.to_world(p),
                        screen: p,
                        pending: Some(Pending::To(to.0, to.1, ty)),
                    }),
                    None => {}
                },
                _ => {}
            }
        }
    }
    if cx.state.drag.is_some() {
        ui.ctx().request_repaint();
    }
}

/// The first input (or output, with OUTPUT) of the square that a wire of
/// type TY (None: a tick) can plug into.
pub fn first_fit(info: &SquareInfo, ty: &Option<Type>, output: bool) -> Option<String> {
    let (defs, types) = if output { (&info.sig.outputs, &info.out_types) } else { (&info.sig.inputs, &info.in_types) };
    let mut fallback = None;
    for (d, t) in defs.iter().zip(types) {
        match (ty, &d.ty) {
            (None, PinTy::Tick) => return Some(d.name.clone()),
            (Some(want), PinTy::Data(_)) => {
                let fits = if output {
                    t.as_ref().is_some_and(|t| lineit_core::types::fits(t, want))
                } else {
                    t.as_ref().is_some_and(|t| lineit_core::types::fits(want, t))
                };
                if fits {
                    return Some(d.name.clone());
                }
                fallback.get_or_insert(d.name.clone());
            }
            _ => {}
        }
    }
    if ty.is_some() { fallback } else { None }
}

/// The code of a code square: its param, or the module's default.
fn code_of(sq: &Square, reg: &Registry) -> String {
    sq.params.get("code").cloned().unwrap_or_else(|| {
        reg.get(CODE)
            .and_then(|m| m.params().iter().find(|d| d.name == "code").map(|d| d.default.clone()))
            .unwrap_or_default()
    })
}

/// A code square's code being typed, in R: Enter is a new line, Tab a tab,
/// Esc or a click away keeps it.
fn code_edit(ui: &mut egui::Ui, e: &mut FieldEdit, r: Rect, z: f32, tab_id: u64, lang: &str, theme: &Theme) {
    e.rect = Some(r);
    let id = Id::unique(("code-edit", tab_id, e.key.sq));
    // laid out and colored as when not typed in, at a power of two
    let b = laid_at(z);
    let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, _: f32| {
        let at = r.min + vec2(2.0, 4.0) * z;
        let ppp = ui.ctx().pixels_per_point();
        let mut nums = vec![b.to_bits(), z.to_bits(), at.x.to_bits(), at.y.to_bits(), ppp.to_bits()];
        nums.extend(theme_nums(theme));
        laid(ui.ctx(), (3, text.as_str(), lang), &nums, || {
            let g = scaled(ui.painter().layout_job(code_job(text.as_str(), lang, false, b, theme)), z / b);
            unrounded(g, at, ppp)
        })
    };
    // (the text box has no margin of its own with a frame set, even NONE:
    // it's put in the rect inside the margin)
    let inner = r.shrink2(vec2(2.0, 4.0) * z);
    let resp = ui.put(inner, |ui: &mut egui::Ui| {
        ui.set_clip_rect(ui.clip_rect().intersect(r));
        ui.add(
            egui::TextEdit::multiline(&mut e.text)
                .id(id)
                .code_editor()
                .lock_focus(true)
                .font(zoomed_mono(12.0 * z))
                .layouter(&mut layouter)
                .frame(egui::Frame::NONE)
                .margin(Vec2::ZERO)
                .desired_width(inner.width())
                .desired_rows(1)
                .text_color(theme.text),
        )
    });
    if e.fresh {
        e.fresh = false;
        resp.request_focus();
    }
    ui.painter().rect_stroke(r, 0.0, Stroke::new(2.0, theme.step), StrokeKind::Outside);
}

/// A note being typed on: its text, on the note, a new line with Enter.
fn note_edit(ui: &mut egui::Ui, e: &mut FieldEdit, r: Rect, z: f32, tab_id: u64) {
    e.rect = Some(r);
    let id = Id::unique(("note", tab_id, e.key.sq));
    let inner = r.shrink(10.0 * z);
    // laid out as when not typed in, with the lines of zoom 1
    let b = laid_at(z);
    let width = r.width() / z;
    let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, _: f32| {
        let at = r.min + vec2(10.0, 10.0) * z;
        let ppp = ui.ctx().pixels_per_point();
        let nums = [width.to_bits(), b.to_bits(), z.to_bits(), at.x.to_bits(), at.y.to_bits(), ppp.to_bits()];
        laid(ui.ctx(), (4, text.as_str(), ""), &nums, || {
            let g = scaled(note_laid(ui.painter(), text.as_str(), width, b, NOTE_TEXT), z / b);
            unrounded(g, at, ppp)
        })
    };
    let resp = ui.put(inner, |ui: &mut egui::Ui| {
        // like the shown note: nothing outside it
        ui.set_clip_rect(ui.clip_rect().intersect(r));
        ui.add(
            egui::TextEdit::multiline(&mut e.text)
                .id(id)
                .font(zoomed_text(14.0 * z))
                .layouter(&mut layouter)
                .frame(egui::Frame::NONE)
                .margin(Vec2::ZERO)
                .desired_width(inner.width())
                .desired_rows(1)
                .text_color(NOTE_TEXT),
        )
    });
    if e.fresh {
        e.fresh = false;
        resp.request_focus();
        // the cursor at the end, to go on writing
        if let Some(mut st) = egui::TextEdit::load_state(ui.ctx(), id) {
            let end = egui::text::CCursor::new(e.text.chars().count());
            st.cursor.set_char_range(Some(egui::text::CCursorRange::one(end)));
            st.store(ui.ctx(), id);
        }
    }
}

/// A field of a square: typed in only while it's being edited (then a text
/// box, which has the keyboard), a box with its value otherwise. A click on
/// one starts typing in it; on a checkbox, toggles it.
#[allow(clippy::too_many_arguments)]
fn field(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    (edit, theme, tab_id): (&mut Option<FieldEdit>, &Theme, u64),
    key: FieldKey,
    kind: FieldKind,
    r: Rect,
    current: &str,
    font: FontId,
    z: f32,
    changes: &mut Vec<(SquareId, String, String, bool)>,
    clicks: &mut Vec<FieldKey>,
) {
    let editing = edit.as_ref().is_some_and(|e| e.key == key);
    let chosen = editing && edit.as_ref().is_some_and(|e| e.choose);
    if kind == FieldKind::Bool {
        draw_check(painter, r, current.trim() == "true", theme, chosen);
        let resp = ui.interact(r, Id::unique(("field", tab_id, &key.sq, &key.name, key.param)), Sense::click());
        if resp.clicked() {
            changes.push((key.sq, key.name.clone(), (current.trim() != "true").to_string(), key.param));
        }
        return;
    }
    if !editing || chosen || z < EDIT_ZOOM {
        draw_field_box(painter, r, current, font, theme, chosen);
        let resp = ui.interact(r, Id::unique(("field", tab_id, &key.sq, &key.name, key.param)), Sense::click());
        if resp.clicked() {
            clicks.push(key);
        }
        return;
    }
    let e = edit.as_mut().unwrap();
    e.rect = Some(r);
    let id = Id::unique(("editing", tab_id, &key.sq, &key.name, key.param));
    // the box drawn here, the text box in it without its own (whose
    // least height is more than a field's when zoomed out)
    painter.rect_filled(r, 0.0, theme.field);
    let text = &mut e.text;
    let resp = ui.put(r, |ui: &mut egui::Ui| {
        ui.spacing_mut().interact_size.y = r.height();
        ui.add(
            egui::TextEdit::singleline(text)
                .id(id)
                .font(font)
                .frame(egui::Frame::NONE)
                .margin(vec2(3.0 * z, 0.0))
                .desired_width(r.width())
                .text_color(theme.text),
        )
    });
    if e.fresh {
        e.fresh = false;
        resp.request_focus();
        if let Some(mut st) = egui::TextEdit::load_state(ui.ctx(), id) {
            let all = egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(e.text.chars().count()),
            );
            st.cursor.set_char_range(Some(all));
            st.store(ui.ctx(), id);
        }
    }
    if resp.changed() {
        e.error = false;
    }
    let edge = if e.error { theme.error } else { theme.step };
    painter.rect_stroke(r, 0.0, Stroke::new(2.0, edge), StrokeKind::Outside);
}

#[cfg(test)]
mod code_tests {
    use super::problem_line;

    #[test]
    fn lines_of_problems() {
        assert_eq!(problem_line("bad.lua: line 5: this doesn't read as Lua, near `end`"), Some(5));
        assert_eq!(problem_line("line 2: `f` returns a value with no type"), Some(2));
        assert_eq!(problem_line("`x` has no type: add a line 3 above"), None);
        assert_eq!(problem_line("there's no function"), None);
    }
}

#[cfg(test)]
mod text_tests {
    use super::*;

    /// Runs F in a frame with the zoomed fonts, as the app sets them.
    fn frame_ctx(f: impl FnOnce(&egui::Context)) {
        let ctx = egui::Context::default();
        let mut fonts = egui::FontDefinitions::default();
        for (from, name) in [(egui::FontFamily::Proportional, ZOOMED_TEXT), (egui::FontFamily::Monospace, ZOOMED_MONO)]
        {
            let list = fonts.families[&from].clone();
            fonts.families.insert(egui::FontFamily::Name(name.into()), list);
        }
        ctx.set_fonts(fonts);
        let mut f = Some(f);
        for _ in 0..2 {
            let out = ctx.run_pass(egui::RawInput::default(), |ctx| {
                if let Some(f) = f.take_if(|_| ctx.cumulative_pass_nr() > 0) {
                    f(ctx);
                }
            });
            out.drop_without_applying_deltas();
        }
    }

    fn frame(f: impl FnOnce(&egui::Painter)) {
        frame_ctx(|ctx| f(&ctx.layer_painter(egui::LayerId::background())));
    }

    /// The text of a galley's rows, with the newlines that ended them.
    fn rebuilt(g: &egui::Galley) -> String {
        g.rows.iter().map(|r| format!("{}{}", r.row.text(), if r.ends_with_newline { "\n" } else { "" })).collect()
    }

    const NOTES: [&str; 6] = [
        "",
        "short",
        "a long note that has to be broken into some lines when it is laid out",
        "lines\nof its own\n\nand an empty one, and a long one that goes on and on and on",
        "ends with a newline\n",
        "trailing spaces   \nin the middle    of  words  and a long tail that wraps   ",
    ];

    #[test]
    fn typed_note_has_the_rows_of_the_shown_one() {
        frame(|p| {
            for text in NOTES {
                let one = p.layout(text.to_string(), zoomed_text(14.0), NOTE_TEXT, 150.0 - 20.0);
                for z in [0.3, 0.7, 1.0, 1.9, 3.0] {
                    let b = laid_at(z);
                    let g = scaled(note_laid(p, text, 150.0, b, NOTE_TEXT), z / b);
                    // the same text and the same breaks as at zoom 1
                    assert_eq!(rebuilt(&g), text, "text at {z}");
                    assert_eq!(rebuilt(&one), text);
                    let rows = |g: &egui::Galley| g.rows.iter().map(|r| r.row.text()).collect::<Vec<_>>();
                    assert_eq!(rows(&g), rows(&one), "rows at {z}");
                    // every char is in a row, so the cursor is where it's typed
                    assert_eq!(g.end().index.0, text.chars().count(), "end at {z}");
                    // scaled, it's as big as laid out at Z / B times
                    let h = note_laid(p, text, 150.0, b, NOTE_TEXT).size() * (z / b);
                    assert!((g.size() - h).abs().max_elem() < 0.01, "size at {z}");
                }
            }
        });
    }

    #[test]
    fn typed_note_sits_where_the_shown_one_is_drawn() {
        frame(|p| {
            // what paint_scaled draws: a row at its pos, scaled
            let (z, b) = (1.3, laid_at(1.3));
            let text = "some words to put\non a few lines of a note";
            let shown = note_laid(p, text, 150.0, b, NOTE_TEXT);
            let typed = scaled(shown.clone(), z / b);
            for (a, t) in shown.rows.iter().zip(&typed.rows) {
                assert!((a.pos.to_vec2() * (z / b) - t.pos.to_vec2()).abs().max_elem() < 1e-3);
                for (ga, gt) in a.row.glyphs.iter().zip(&t.row.glyphs) {
                    assert!((ga.pos.to_vec2() * (z / b) - gt.pos.to_vec2()).abs().max_elem() < 1e-3);
                }
                for (va, vt) in a.row.visuals.mesh.vertices.iter().zip(&t.row.visuals.mesh.vertices) {
                    assert!((va.pos.to_vec2() * (z / b) - vt.pos.to_vec2()).abs().max_elem() < 1e-3);
                    assert_eq!(va.uv, vt.uv);
                }
            }
        });
    }

    #[test]
    fn galley_of_a_note_is_of_its_text() {
        frame(|p| {
            for text in NOTES {
                let g = note_laid(p, text, 150.0, 2.0, NOTE_TEXT);
                // its sections cover exactly its text, in order
                assert_eq!(g.job.text, text);
                let mut at = 0;
                for s in &g.job.sections {
                    assert_eq!(s.byte_range.start.0, at);
                    at = s.byte_range.end.0;
                }
                assert_eq!(at, text.len(), "sections of {text:?}");
                // select across the soft breaks and slice what's selected
                let all = egui::text::CCursorRange::two(egui::text::CCursor::new(0), g.end());
                assert_eq!(all.slice_str(text), text);
                let from = egui::text::CCursor::new(text.chars().count() / 3);
                let part = egui::text::CCursorRange::two(from, g.end());
                let want: String = text.chars().skip(from.index.0).collect();
                assert_eq!(part.slice_str(text), want);
                let mut sel = scaled(g, 0.7);
                egui::text_selection::visuals::paint_text_selection(&mut sel, &egui::Visuals::dark(), &part, None);
                // a cursor on each char is on the galley
                for k in 0..=text.chars().count() {
                    let c = sel.pos_from_cursor(egui::text::CCursor::new(k));
                    assert!(sel.rect.expand(1.0).contains(c.center()), "cursor {k} of {text:?}");
                }
            }
        });
    }

    #[test]
    fn galleys_are_kept_while_nothing_changes() {
        frame(|p| {
            let a = note_laid(p, "some words", 150.0, 2.0, NOTE_TEXT);
            assert!(std::sync::Arc::ptr_eq(&a, &note_laid(p, "some words", 150.0, 2.0, NOTE_TEXT)));
            assert!(!std::sync::Arc::ptr_eq(&a, &note_laid(p, "some words!", 150.0, 2.0, NOTE_TEXT)));
            assert!(!std::sync::Arc::ptr_eq(&a, &note_laid(p, "some words", 150.0, 4.0, NOTE_TEXT)));
        });
    }

    #[test]
    fn a_tab_is_four_spaces_wide() {
        frame(|p| {
            for z in [0.25, 0.5, 1.0, 1.7, 2.0, 4.0, 8.0] {
                let w = |t: &str| p.layout_no_wrap(t.to_string(), zoomed_mono(12.0 * z), Color32::WHITE).size().x;
                assert!((w("\tx") - w("    x")).abs() < 1e-3, "tab at {z}");
                assert!((w("a\tx") - w("a    x")).abs() < 1e-3, "tab after a at {z}");
            }
        });
    }

    /// The text box puts the galley at the position the layouters assume.
    #[test]
    fn text_box_puts_the_galley_where_it_is_assumed() {
        frame_ctx(|ctx| {
            let out = ctx.run_pass(egui::RawInput::default(), |ctx| {
                ctx.root_ui(|ui| {
                    let ppp = ui.ctx().pixels_per_point();
                    let (z, b) = (1.37, laid_at(1.37));
                    // as note_edit and code_edit set it up
                    let r = Rect::from_min_size(pos2(103.3, 57.9), vec2(220.0, 90.0));
                    for (rect, margin, at) in [
                        (r.shrink(10.0 * z), Vec2::ZERO, r.min + vec2(10.0, 10.0) * z),
                        (r.shrink2(vec2(2.0, 4.0) * z), Vec2::ZERO, r.min + vec2(2.0, 4.0) * z),
                    ] {
                        let mut text = "some words that go on\nand on".to_string();
                        let mut layouter = |ui: &egui::Ui, t: &dyn egui::TextBuffer, _: f32| {
                            let g = scaled(note_laid(ui.painter(), t.as_str(), 160.0, b, NOTE_TEXT), z / b);
                            unrounded(g, at, ppp)
                        };
                        let out = ui
                            .scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                                egui::TextEdit::multiline(&mut text)
                                    .font(zoomed_text(14.0 * z))
                                    .layouter(&mut layouter)
                                    .frame(egui::Frame::NONE)
                                    .margin(margin)
                                    .desired_width(rect.width())
                                    .show(ui)
                            })
                            .inner;
                        // (egui puts the rect of a widget on a grid of a 64th of a point)
                        assert!(
                            (out.galley_pos - at).abs().max_elem() < 1.0 / 32.0,
                            "{:?} vs {at:?} (rect {rect:?})",
                            out.galley_pos
                        );
                    }
                });
            });
            out.drop_without_applying_deltas();
        });
    }

    #[test]
    fn typed_code_has_the_rows_of_the_shown_one() {
        frame(|p| {
            let theme = super::super::theme::dark();
            for text in ["fn main() {\n\tlet x = 1;\n}", "a\n\nb c d  \n", ""] {
                for z in [0.5, 1.0, 2.5] {
                    let b = laid_at(z);
                    let shown = code_colored(p, text, "rust", false, b, &theme);
                    let typed = scaled(p.layout_job(code_job(text, "rust", false, b, &theme)), z / b);
                    // typed text keeps its tabs and the end it has
                    assert_eq!(typed.job.text, text);
                    assert_eq!(typed.end().index.0, text.chars().count());
                    // the rows shown are there, at the same places (the end, trimmed, isn't)
                    for (a, t) in shown.rows.iter().zip(&typed.rows) {
                        assert_eq!(a.row.text().trim_end(), t.row.text().trim_end());
                        assert!((a.pos.to_vec2() * (z / b) - t.pos.to_vec2()).abs().max_elem() < 1e-3);
                    }
                }
            }
        });
    }
}

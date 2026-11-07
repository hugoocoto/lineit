//! The canvas: drawing the squares and wires of a tab, and what the mouse
//! does to them.
//!
//! Positions in the canvas file are world units; the camera turns them
//! into screen points ([`View`]). Squares, pins, fields and the canvas
//! itself are egui widgets, made in that order each frame, so egui gives a
//! click to the topmost one: a field over its square over the canvas.

use std::collections::{BTreeSet, HashMap};

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

pub const HEADER: f32 = 26.0;
pub const ROW: f32 = 22.0;
pub const PIN: f32 = 5.5;

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
    Search { at: Pos2, screen: Pos2, pending: Option<Pending> },
    OpenModule(String),
    MakeSubgraph,
    Copy,
    Delete,
    Duplicate,
}

#[derive(Default)]
pub struct CanvasState {
    pub drag: Option<Drag>,
    /// Images on squares, by square and output, with the image they show.
    pub previews: HashMap<(SquareId, String), (egui::TextureHandle, lineit_rt::Image)>,
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
}

fn bezier(a: Pos2, b: Pos2, zoom: f32) -> [Pos2; 4] {
    let d = ((b.x - a.x).abs() * 0.5).clamp(30.0 * zoom, 200.0 * zoom);
    [a, a + vec2(d, 0.0), b - vec2(d, 0.0), b]
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
fn extra_height(sq: &Square, info: Option<&SquareInfo>, cx: &Ctx, width: f32) -> f32 {
    if sq.module == "io/display" {
        return ROW;
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
    let theme = cx.theme.clone();
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
        let params = cx.reg.get(&sq.module).map(|m| m.params().len()).unwrap_or(0);
        let extra = extra_height(sq, info, cx, width);
        geos.insert(sq.id, geometry(sq, info, params, width, extra));
    }
    let pin_pos = |id: SquareId, name: &str, output: bool| -> Option<(Pos2, Option<Type>)> {
        let info = cx.tab.lowered.squares.get(&id)?;
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
        let color = theme.of(ty.as_ref());
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
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let shift = ui.input(|i| i.modifiers.shift);

    // squares
    let ids: Vec<SquareId> = cx.tab.canvas.squares.iter().map(|s| s.id).collect();
    let mut move_by: Option<Vec2> = None;
    let mut snap = false;
    let mut field_changes: Vec<(SquareId, String, String, bool)> = Vec::new();
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
        let r = CornerRadius::same((6.0 * z).clamp(0.0, 255.0) as u8);
        painter.rect_filled(srect, r, theme.square);
        let header = Rect::from_min_size(srect.min, vec2(srect.width(), HEADER * z));
        painter.rect_filled(header, CornerRadius { nw: r.nw, ne: r.ne, sw: 0, se: 0 }, theme.header);
        let border = if cx.current.contains(&id) {
            Stroke::new(3.0, theme.step)
        } else if problems.contains_key(&id) {
            Stroke::new(2.0, theme.error)
        } else if selected {
            Stroke::new(2.0, theme.selection)
        } else {
            Stroke::new(1.0, theme.border)
        };
        painter.rect_stroke(srect, r, border, StrokeKind::Outside);
        let title = match &module {
            Some(_) => sq.module.clone(),
            None => format!("? {}", sq.module),
        };
        painter.text(
            header.left_center() + vec2(8.0 * z, 0.0),
            Align2::LEFT_CENTER,
            short(&title, 30),
            font(13.0),
            theme.text,
        );

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
        let doc = module.as_ref().map(|m| m.doc().to_string()).unwrap_or_default();
        let resp = if !doc.is_empty() && header.contains(pointer.unwrap_or_default()) {
            resp.on_hover_text(doc)
        } else {
            resp
        };
        resp.context_menu(|ui| {
            if ui.button("Duplicate (Ctrl+D)").clicked() {
                cx.actions.push(Action::Duplicate);
                ui.close();
            }
            if ui.button("Copy (Ctrl+C)").clicked() {
                cx.actions.push(Action::Copy);
                ui.close();
            }
            if ui.button("Disconnect").clicked() {
                let sel = cx.tab.selected.clone();
                cx.tab.canvas.links.retain(|l| !sel.contains(&l.from) && !sel.contains(&l.to));
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

        // pins, and the fields of unwired inputs
        if let Some(info) = &info {
            let show_widgets = z >= 0.55;
            for (i, pin) in info.sig.inputs.iter().enumerate() {
                let p = view.to_screen(g.ins[i]);
                let ty = info.in_types[i].clone();
                let color = theme.of(ty.as_ref());
                let is_wired = wired.contains(&(id, pin.name.clone()));
                if pin.ty == PinTy::Tick {
                    let s = PIN * z * 1.1;
                    let tri = vec![p + vec2(-s, -s), p + vec2(s, 0.0), p + vec2(-s, s)];
                    painter.add(egui::Shape::convex_polygon(
                        tri,
                        if is_wired { color } else { theme.square },
                        Stroke::new(1.5, color),
                    ));
                } else if is_wired {
                    painter.circle_filled(p, PIN * z, color);
                } else {
                    painter.circle(p, PIN * z, theme.square, Stroke::new(1.5, color));
                }
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
                if pin_resp.drag_started() {
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
                    let has_out = i < info.sig.outputs.len();
                    let left = p.x + 10.0 * z + label_w + 6.0 * z;
                    let right = if has_out { srect.center().x + 18.0 * z } else { srect.right() - 10.0 * z };
                    if right - left < 16.0 * z {
                        continue;
                    }
                    let frect = Rect::from_min_max(pos2(left, p.y - 9.0 * z), pos2(right, p.y + 9.0 * z));
                    let current = sq.fields.get(&pin.name).cloned().unwrap_or_else(|| default.clone());
                    if !show_widgets {
                        painter.rect_filled(frect, 2.0, theme.field);
                        continue;
                    }
                    if ty == Some(Type::Bool) {
                        let mut b = current.trim() == "true";
                        if ui.put(frect, egui::Checkbox::without_text(&mut b)).changed() {
                            field_changes.push((id, pin.name.clone(), b.to_string(), false));
                        }
                    } else {
                        let mut text = current.clone();
                        let resp = ui.put(
                            frect,
                            egui::TextEdit::singleline(&mut text)
                                .id(Id::unique(("field", cx.tab_id, id, i)))
                                .font(mono(12.0))
                                .margin(vec2(3.0 * z, 1.0))
                                .background_color(theme.field),
                        );
                        if text != current {
                            field_changes.push((id, pin.name.clone(), text, false));
                        }
                        let _ = resp;
                    }
                }
            }
            for (o, pin) in info.sig.outputs.iter().enumerate() {
                let p = view.to_screen(g.outs[o]);
                let ty = info.out_types[o].clone();
                let color = theme.of(ty.as_ref());
                if pin.ty == PinTy::Tick {
                    let s = PIN * z * 1.1;
                    let tri = vec![p + vec2(-s, -s), p + vec2(s, 0.0), p + vec2(-s, s)];
                    painter.add(egui::Shape::convex_polygon(tri, color, Stroke::new(1.5, color)));
                } else {
                    painter.circle_filled(p, PIN * z, color);
                }
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

            // params
            if let Some(m) = &module {
                for (k, def) in m.params().iter().enumerate() {
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
                        ParamKind::Bool => None,
                        _ => None,
                    };
                    if let Some(choices) = choices {
                        let mut chosen = current.clone();
                        ui.put(frect, |ui: &mut egui::Ui| {
                            egui::ComboBox::from_id_salt(("param", cx.tab_id, id, k))
                                .selected_text(egui::RichText::new(&chosen).font(mono(12.0)))
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
                    } else if def.kind == ParamKind::Bool {
                        let mut b = current.trim() == "true";
                        if ui.put(frect, egui::Checkbox::without_text(&mut b)).changed() {
                            field_changes.push((id, def.name.clone(), b.to_string(), true));
                        }
                    } else {
                        let mut text = current.clone();
                        let hint = match def.kind {
                            ParamKind::Type { .. } => "int, str, list<float>...",
                            ParamKind::Var => "a variable",
                            _ => "",
                        };
                        ui.put(
                            frect,
                            egui::TextEdit::singleline(&mut text)
                                .id(Id::unique(("param", cx.tab_id, id, k)))
                                .font(mono(12.0))
                                .hint_text(hint)
                                .margin(vec2(3.0 * z, 1.0))
                                .background_color(theme.field),
                        );
                        if text != current {
                            field_changes.push((id, def.name.clone(), text, true));
                        }
                    }
                }
            }

            // what it shows
            let extra_top = view.to_screen(pos2(0.0, g.extra_top)).y;
            if sq.module == "io/display" {
                let text = cx.values.get(&(id, "value".to_string())).map(|v| v.text()).unwrap_or_else(|| "—".into());
                painter.text(
                    pos2(srect.left() + 8.0 * z, extra_top + ROW * z / 2.0),
                    Align2::LEFT_CENTER,
                    short(&text.replace('\n', " "), 40),
                    mono(13.0),
                    theme.text,
                );
            } else {
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

        // what's wrong with it
        if let Some(msgs) = problems.get(&id) {
            let text = msgs.join("\n");
            let galley = painter.layout(text, font(12.0), theme.error, srect.width());
            painter.galley(srect.left_bottom() + vec2(0.0, 4.0 * z), galley, theme.error);
        }
    }
    for (id, name, value, is_param) in field_changes {
        if let Some(sq) = cx.tab.canvas.square_mut(id) {
            if is_param {
                sq.params.insert(name, value);
            } else {
                sq.fields.insert(name, value);
            }
        }
    }
    if let Some(d) = move_by {
        let sel = cx.tab.selected.clone();
        for sq in cx.tab.canvas.squares.iter_mut().filter(|s| sel.contains(&s.id)) {
            sq.x += d.x;
            sq.y += d.y;
        }
    }
    if snap && cx.cfg.bool("canvas.snap") {
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
                Drag::Wire { from, ty, picked } => match near(false) {
                    Some((to, input, tick)) => {
                        if tick == ty.is_none() && to != from.0 {
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
                Drag::WireBack { to, ty } => match near(true) {
                    Some((from, output, tick)) => {
                        if tick == ty.is_none() && from != to.0 {
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
                Drag::Select { .. } => {}
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

//! Vim mode in the editor: egui's key events become [`Input`]s for the
//! state machine in `vim.rs`, and what it answers is done here. It's a part
//! of `app` because it works on the editor's own state: tabs, search,
//! prompts and settings.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Instant;

use eframe::egui::{self, Color32, Event, Key, Pos2, pos2, vec2};
use lineit_core::config::{Kind as SKind, SETTINGS, SValue, SettingDef};
use lineit_core::model::{Link, SquareId};

use super::{App, PromptKind, RunMode, Search, complete, completions, search_score};
use crate::editor::canvas::{self, Hint, Pending, PinRef, PinState};
use crate::editor::nav;
use crate::editor::tab::Tab;
use crate::editor::theme::{self, Theme};
use crate::editor::vim::{self, Act, Dir, Ex, Input, Mode, SAct, SetOp, Wire};

/// What the vim mode does with an event.
enum Class {
    /// Not its: egui or the shortcuts have it.
    Pass,
    /// Its, and nothing to do: the key of a letter whose text comes as
    /// another event.
    Drop,
    Key(Input),
}

fn classify(e: &Event, mode: Mode) -> Class {
    let line = matches!(mode, Mode::Line(_));
    match e {
        Event::Text(t) => {
            let mut cs = t.chars();
            match (cs.next(), cs.next()) {
                (Some(c), None) if !c.is_control() => Class::Key(Input::Char(c)),
                _ => Class::Pass,
            }
        }
        Event::Key { key, pressed, modifiers, .. } => {
            let down = |k: Input| if *pressed { Class::Key(k) } else { Class::Drop };
            if modifiers.ctrl && !modifiers.alt {
                let c = match key {
                    Key::E => 'e',
                    Key::Y => 'y',
                    Key::D => 'd',
                    Key::U => 'u',
                    Key::R => 'r',
                    Key::N => 'n',
                    Key::P => 'p',
                    Key::W => 'w',
                    Key::C => 'c',
                    Key::OpenBracket => '[',
                    _ => return Class::Pass,
                };
                // the Ctrl keys that are the editor's stay its
                let ours = if line { "upwnc[" } else { "eydur[" };
                return if ours.contains(c) { down(Input::Ctrl(c)) } else { Class::Pass };
            }
            if modifiers.ctrl || modifiers.alt || modifiers.command || modifiers.mac_cmd {
                return Class::Pass;
            }
            match key {
                Key::Escape => down(Input::Esc),
                Key::Enter => down(Input::Enter),
                Key::Tab if modifiers.shift => down(Input::BackTab),
                Key::Tab => down(Input::Tab),
                Key::Backspace if line => down(Input::Backspace),
                Key::ArrowUp => down(Input::Up),
                Key::ArrowDown => down(Input::Down),
                Key::ArrowLeft => down(Input::Left),
                Key::ArrowRight => down(Input::Right),
                // the keys that type nothing stay with whoever uses them: F5 runs,
                // Delete deletes
                Key::Backspace | Key::Delete | Key::Insert | Key::Home | Key::End | Key::PageUp | Key::PageDown => {
                    Class::Pass
                }
                Key::Copy | Key::Cut | Key::Paste => Class::Pass,
                k if k.name().len() > 1
                    && k.name().starts_with('F')
                    && k.name()[1..].chars().all(|c| c.is_ascii_digit()) =>
                {
                    Class::Pass
                }
                _ => Class::Drop,
            }
        }
        _ => Class::Pass,
    }
}

/// Which way a setting's text means true or false.
fn parse_bool(v: &str) -> Option<bool> {
    match v.to_lowercase().as_str() {
        "true" | "on" | "yes" | "1" => Some(true),
        "false" | "off" | "no" | "0" => Some(false),
        _ => None,
    }
}

fn show_value(v: &SValue) -> String {
    match v {
        SValue::Bool(b) => b.to_string(),
        SValue::Num(n) => n.to_string(),
        SValue::Text(t) => format!("{t:?}"),
    }
}

/// The setting NAME is: the whole key, or the end of one if only one has it.
fn resolve_setting(name: &str) -> Result<&'static SettingDef, String> {
    if let Some(d) = SETTINGS.iter().find(|d| d.key == name) {
        return Ok(d);
    }
    let ends: Vec<&SettingDef> = SETTINGS.iter().filter(|d| d.key.rsplit('.').next() == Some(name)).collect();
    match ends.as_slice() {
        [d] => Ok(d),
        [] => Err(format!("no setting called {name}")),
        many => Err(format!("{name} is {}: say which", many.iter().map(|d| d.key).collect::<Vec<_>>().join(" or "))),
    }
}

impl App {
    pub(super) fn vim_on(&self) -> bool {
        self.cfg.bool("editor.vim_mode")
    }

    /// The keys are vim's: nothing is being typed into.
    fn vim_gate(&self, ctx: &egui::Context) -> bool {
        self.search.is_none()
            && self.prompt.is_none()
            && self.confirm.is_none()
            && self.tabs[self.active].2.edit.is_none()
            && ctx.memory(|m| m.focused().is_none())
    }

    /// Before egui reads the input: egui moves its focus with Tab and the
    /// arrows, which would take the keyboard from vim mode. They're held
    /// back while vim mode has it, and `vim_keys` takes them from there.
    pub(super) fn vim_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        if !self.vim_on() || self.tabs.is_empty() || !(self.help || self.vim_gate(ctx)) {
            return;
        }
        let ours = |e: &Event| {
            matches!(e, Event::Key { key: Key::Tab | Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight, modifiers, .. }
                if !modifiers.ctrl && !modifiers.alt && !modifiers.command && !modifiers.mac_cmd)
        };
        let (held, rest): (Vec<Event>, Vec<Event>) = raw.events.drain(..).partition(ours);
        raw.events = rest;
        self.vim_held.extend(held);
    }

    /// Every frame, before the shortcuts: the keys, and what the screen
    /// shows of them.
    pub(super) fn vim_keys(&mut self, ctx: &egui::Context) {
        let on = self.vim_on();
        for (_, _, st) in &mut self.tabs {
            st.vim.on = on;
        }
        if !on {
            self.vim_held.clear();
            if self.vim.mode != Mode::Normal || self.help {
                self.help_offset = 0.0;
                self.vim.reset();
                self.help = false;
            }
            let st = &mut self.tabs[self.active].2;
            st.vim.pin = None;
            st.vim.hints.clear();
            return;
        }
        let id = self.tabs[self.active].0;
        if id != self.vim_tab {
            self.vim_tab = id;
            self.vim.reset();
            for (_, _, st) in &mut self.tabs {
                st.vim.pin = None;
                st.vim.hints.clear();
            }
        }
        let settings = self.tab().settings;
        if !settings {
            self.vim_adopt();
        }
        let mut events = std::mem::take(&mut self.vim_held);
        // what was held back goes back in if it isn't vim's
        let mut taken = !events.is_empty();
        events.extend(ctx.input(|i| i.events.clone()));
        let mut keep = Vec::with_capacity(events.len());
        for e in events {
            if !self.help && !self.vim_gate(ctx) {
                keep.push(e);
                continue;
            }
            match classify(&e, self.vim.mode) {
                Class::Pass => keep.push(e),
                Class::Drop => taken = true,
                Class::Key(k) => {
                    taken = true;
                    self.vim_input(k, ctx);
                }
            }
        }
        if taken {
            ctx.input_mut(|i| i.events = keep);
        }
        if !self.tab().settings {
            self.vim_settle();
            self.vim_reveal(ctx);
            self.vim_hints();
        }
    }

    fn vim_input(&mut self, k: Input, ctx: &egui::Context) {
        if self.help {
            // a line is about 18 points; a page, most of the window
            let (line, half) = (18.0, 260.0);
            let g = std::mem::take(&mut self.help_g);
            match k {
                Input::Esc | Input::Enter | Input::Char('q') => self.help = false,
                Input::Char('j') | Input::Down | Input::Ctrl('e') | Input::Ctrl('n') => self.help_offset += line,
                Input::Char('k') | Input::Up | Input::Ctrl('y') | Input::Ctrl('p') => self.help_offset -= line,
                Input::Char('d') | Input::Ctrl('d') | Input::Char(' ') => self.help_offset += half,
                Input::Char('u') | Input::Ctrl('u') => self.help_offset -= half,
                Input::Char('g') if g => self.help_offset = 0.0,
                Input::Char('g') => self.help_g = true,
                // the window clamps it
                Input::Char('G') => self.help_offset = 1.0e6,
                _ => {}
            }
            self.help_offset = self.help_offset.max(0.0);
            return;
        }
        if self.tab().settings {
            if k == Input::Esc && self.vim.mode == Mode::Normal {
                self.settings_query.clear();
            }
            for a in self.vim.settings_key(k) {
                match a {
                    SAct::Ex(line) => self.vim_ex(&line, ctx),
                    SAct::Complete => self.vim_complete(),
                    a => self.settings_ops.push(a),
                }
            }
            return;
        }
        for a in self.vim.key(k) {
            self.vim_act(a, ctx);
        }
    }

    // ------------------------------------------------------------ focus

    /// The focus is a square that's there, and follows what the mouse
    /// selected.
    fn vim_adopt(&mut self) {
        let (_, tab, st) = &mut self.tabs[self.active];
        if tab.selected != st.vim.sel {
            if !tab.selected.is_empty() {
                let keep = st.vim.focus.filter(|f| tab.selected.contains(f));
                st.vim.focus = keep.or_else(|| tab.selected.iter().next().copied());
            }
            st.vim.sel = tab.selected.clone();
        }
        if st.vim.focus.is_none_or(|f| tab.canvas.square(f).is_none()) {
            st.vim.focus = tab.canvas.squares.first().map(|s| s.id);
            st.vim.reveal = st.vim.focus;
        }
    }

    fn rects(&self) -> nav::Rects {
        let (_, tab, st) = &self.tabs[self.active];
        st.rects.iter().filter(|(id, _)| tab.canvas.square(**id).is_some()).map(|(i, r)| (*i, *r)).collect()
    }

    fn vim_focused(&self) -> Option<SquareId> {
        self.tabs[self.active].2.vim.focus
    }

    /// Focus goes to ID: selected, or added to the selection in Visual.
    fn vim_focus(&mut self, id: SquareId) {
        let mode = self.vim.mode;
        let (_, tab, st) = &mut self.tabs[self.active];
        st.vim.focus = Some(id);
        match mode {
            Mode::Visual => {
                let t = &mut st.vim.trail;
                if t.len() >= 2 && t[t.len() - 2] == id {
                    let top = t.pop().unwrap_or(id);
                    if !t.contains(&top) {
                        tab.selected.remove(&top);
                    }
                } else if !t.contains(&id) {
                    t.push(id);
                    tab.selected.insert(id);
                }
            }
            // wiring doesn't select
            Mode::Pin => {}
            _ => {
                tab.selected.clear();
                tab.selected.insert(id);
                tab.selected_links.clear();
            }
        }
        st.vim.sel = tab.selected.clone();
        st.vim.reveal = Some(id);
        if let Some(p) = &mut st.vim.pin {
            p.cursor = None;
        }
    }

    /// The selection, or the focused square if nothing is selected.
    fn vim_targets(&mut self) -> BTreeSet<SquareId> {
        let (_, tab, st) = &mut self.tabs[self.active];
        if tab.selected.is_empty()
            && let Some(f) = st.vim.focus
        {
            tab.selected.insert(f);
        }
        st.vim.sel = tab.selected.clone();
        tab.selected.clone()
    }

    /// Only the focused square is selected.
    fn vim_collapse(&mut self) {
        let (_, tab, st) = &mut self.tabs[self.active];
        tab.selected.clear();
        tab.selected_links.clear();
        if let Some(f) = st.vim.focus {
            tab.selected.insert(f);
        }
        st.vim.sel = tab.selected.clone();
        st.vim.trail.clear();
    }

    /// Pans the camera to show a square the keys went to.
    pub(super) fn vim_reveal(&mut self, ctx: &egui::Context) {
        let (_, tab, st) = &mut self.tabs[self.active];
        let Some(id) = st.vim.reveal else { return };
        if tab.canvas.square(id).is_none() {
            st.vim.reveal = None;
            return;
        }
        let (Some(r), Some(view)) = (st.rects.get(&id), st.rect) else {
            ctx.request_repaint();
            return;
        };
        let cam = &mut tab.canvas.camera;
        let c = nav::reveal(pos2(cam.x, cam.y), cam.zoom, view.size(), *r, 60.0);
        cam.x = c.x;
        cam.y = c.y;
        st.vim.reveal = None;
    }

    /// The pins numbered on the focused square, while wiring.
    fn vim_hints(&mut self) {
        let cands = self.pin_cands();
        let (_, tab, st) = &mut self.tabs[self.active];
        st.vim.hints.clear();
        let Some(p) = &st.vim.pin else { return };
        let cursor =
            p.cursor.unwrap_or_else(|| match (&p.from, st.vim.focus.and_then(|f| tab.lowered.squares.get(&f))) {
                (Some(from), Some(info)) => canvas::first_fit(info, &from.ty, !from.output)
                    .and_then(|n| cands.iter().position(|c| c.name == n))
                    .unwrap_or(0),
                _ => 0,
            });
        st.vim.hints = cands
            .into_iter()
            .enumerate()
            .map(|(i, pin)| Hint {
                pin,
                label: if i < 9 { (i + 1).to_string() } else { "·".into() },
                cursor: i == cursor,
            })
            .collect();
    }

    // ------------------------------------------------------------ acts

    fn vim_act(&mut self, act: Act, ctx: &egui::Context) {
        let reg = self.reg.clone();
        let focus = self.vim_focused();
        match act {
            Act::Move(d, n) => {
                let rects = self.rects();
                let mut at = focus;
                for _ in 0..n {
                    match at.and_then(|f| nav::nearest(&rects, f, d)) {
                        Some(t) => at = Some(t),
                        None => break,
                    }
                }
                if at != focus {
                    self.vim_go(at);
                }
            }
            Act::Follow(down, n) => {
                let rects = self.rects();
                let to = focus.and_then(|f| nav::follow(&self.tab().canvas.links, &rects, f, down, n));
                self.vim_go(to);
            }
            Act::Sibling(d) => {
                let rects = self.rects();
                let to = focus.and_then(|f| nav::sibling(&self.tab().canvas.links, &rects, f, d));
                self.vim_go(to);
            }
            Act::Chain(end) => {
                let rects = self.rects();
                let to = focus.map(|f| nav::chain_end(&self.tab().canvas.links, &rects, f, end));
                self.vim_go(to.filter(|t| Some(*t) != focus));
            }
            Act::First => {
                let rects = self.rects();
                self.vim_go(nav::order(&rects).first().copied());
            }
            Act::Last(n) => {
                let rects = self.rects();
                let ord = nav::order(&rects);
                let to = match n {
                    Some(n) => ord.get(n.saturating_sub(1)).copied(),
                    None => ord.last().copied(),
                };
                self.vim_go(to);
            }
            Act::Reading(d) => {
                let rects = self.rects();
                self.vim_go(nav::reading(&rects, focus, d));
            }
            Act::Search(q) => {
                self.last_search = q;
                self.vim_search(1, 1, true);
            }
            Act::SearchStep(dir, n) => self.vim_search(dir, n, false),
            Act::Shove(d, n) => {
                let step = self.cfg.num("canvas.grid_step") as f32 * n as f32;
                let (dx, dy) = match d {
                    Dir::Left => (-step, 0.0),
                    Dir::Right => (step, 0.0),
                    Dir::Up => (0.0, -step),
                    Dir::Down => (0.0, step),
                };
                let sel = self.vim_targets();
                let tab = self.tab();
                for sq in tab.canvas.squares.iter_mut().filter(|s| sel.contains(&s.id)) {
                    sq.x += dx;
                    sq.y += dy;
                }
            }
            Act::Edit(n) => {
                let Some(f) = focus else { return };
                // too far out, a field isn't a text box: come in on the square
                if let Some(r) = self.rects().get(&f).copied() {
                    let cam = &mut self.tab().canvas.camera;
                    if cam.zoom < canvas::EDIT_ZOOM {
                        (cam.zoom, cam.x, cam.y) = (canvas::EDIT_ZOOM, r.center().x, r.center().y);
                    }
                }
                let (_, tab, st) = &mut self.tabs[self.active];
                if !canvas::edit_nth(tab, &reg, st, f, n) {
                    self.status("this square has no fields to type in", false);
                }
            }
            Act::Add { wire, side } => self.vim_add(wire, side),
            Act::Delete => self.vim_delete(ctx),
            Act::Yank => {
                self.vim_targets();
                self.vim_yank(ctx);
            }
            Act::Paste(after) => self.vim_paste(after),
            Act::Undo(n) => {
                self.tab().update(&reg, false, false);
                for _ in 0..n {
                    self.tab().undo();
                }
            }
            Act::Redo(n) => {
                self.tab().update(&reg, false, false);
                for _ in 0..n {
                    self.tab().redo();
                }
            }
            Act::VisualOn => {
                self.vim_collapse();
                let (_, _, st) = &mut self.tabs[self.active];
                st.vim.trail = focus.into_iter().collect();
            }
            Act::VisualChain => {
                if let Some(f) = focus {
                    let (_, tab, st) = &mut self.tabs[self.active];
                    tab.selected = nav::connected(&tab.canvas.links, f);
                    st.vim.sel = tab.selected.clone();
                    st.vim.trail = vec![f];
                }
            }
            Act::VisualOff | Act::Escape => self.vim_collapse(),
            Act::PinStart => {
                let Some(f) = focus else {
                    self.vim.reset();
                    return;
                };
                if canvas::pins_of(self.tab(), f).is_empty() {
                    self.status("this square has no pins", false);
                    self.vim.reset();
                    return;
                }
                self.tabs[self.active].2.vim.pin = Some(PinState { from: None, cursor: None });
            }
            Act::PinPick(i) => {
                if let Some(pin) = self.pin_cands().get(i).cloned() {
                    self.vim_pin_choose(pin);
                }
            }
            Act::PinAccept => {
                let cands = self.pin_cands();
                let marked = self.tabs[self.active].2.vim.hints.iter().position(|h| h.cursor).unwrap_or(0);
                if let Some(pin) = cands.get(marked).cloned() {
                    self.vim_pin_choose(pin);
                }
            }
            Act::PinCycle(d) => {
                let n = self.pin_cands().len() as i32;
                let marked = self.tabs[self.active].2.vim.hints.iter().position(|h| h.cursor).unwrap_or(0) as i32;
                if n > 0
                    && let Some(p) = &mut self.tabs[self.active].2.vim.pin
                {
                    p.cursor = Some((marked + d).rem_euclid(n) as usize);
                }
            }
            Act::PinRemove => self.vim_pin_remove(),
            Act::PinCancel => self.vim_pin_end(true),
            Act::OpenSubgraph => self.vim_open_subgraph(),
            Act::TabNext(n) => self.active = (self.active + n) % self.tabs.len(),
            Act::TabPrev(n) => self.active = (self.active + self.tabs.len() - n % self.tabs.len()) % self.tabs.len(),
            Act::TabGo(n) => {
                if n >= 1 && n <= self.tabs.len() {
                    self.active = n - 1;
                } else {
                    self.status(&format!("no tab {n}: there are {}", self.tabs.len()), true);
                }
            }
            Act::Pan(d, n) => {
                let cam = &mut self.tab().canvas.camera;
                let step = 64.0 * n as f32 / cam.zoom;
                match d {
                    Dir::Left => cam.x -= step,
                    Dir::Right => cam.x += step,
                    Dir::Up => cam.y -= step,
                    Dir::Down => cam.y += step,
                }
            }
            Act::PanHalf(down) => {
                let h = self.tabs[self.active].2.rect.map_or(400.0, |r| r.height()) / 2.0;
                let cam = &mut self.tab().canvas.camera;
                cam.y += if down { h } else { -h } / cam.zoom;
            }
            Act::Center => {
                if let Some(r) = focus.and_then(|f| self.rects().get(&f).copied()) {
                    let cam = &mut self.tab().canvas.camera;
                    cam.x = r.center().x;
                    cam.y = r.center().y;
                }
            }
            Act::ZoomIn => self.vim_zoom(Some(1.25)),
            Act::ZoomOut => self.vim_zoom(Some(0.8)),
            Act::ZoomReset => self.vim_zoom(None),
            Act::FitAll => {
                let rects = self.rects();
                let view = self.tabs[self.active].2.rect.map_or(vec2(1000.0, 600.0), |r| r.size());
                let range = (self.cfg.num("canvas.zoom_min") as f32, self.cfg.num("canvas.zoom_max") as f32);
                if let Some((c, z)) = nav::fit(&rects, view, 40.0, range) {
                    let cam = &mut self.tab().canvas.camera;
                    (cam.x, cam.y, cam.zoom) = (c.x, c.y, z);
                }
            }
            Act::Ex(line) => self.vim_ex(&line, ctx),
            Act::Complete => self.vim_complete(),
        }
    }

    /// Focus to TO, if there is one; said if there isn't.
    fn vim_go(&mut self, to: Option<SquareId>) {
        if let Some(t) = to {
            self.vim_focus(t);
        }
    }

    fn vim_zoom(&mut self, factor: Option<f32>) {
        let (lo, hi) = (self.cfg.num("canvas.zoom_min") as f32, self.cfg.num("canvas.zoom_max") as f32);
        let cam = &mut self.tab().canvas.camera;
        cam.zoom = match factor {
            Some(f) => (cam.zoom * f).clamp(lo, hi),
            None => 1.0f32.clamp(lo, hi),
        };
    }

    /// `/` and `n`: DIR squares on from the focus that match, N times.
    /// FRESH: a new search, which also counts a match under the focus.
    fn vim_search(&mut self, dir: i32, n: usize, fresh: bool) {
        let q = self.last_search.clone();
        if q.is_empty() {
            self.status("nothing searched for yet: /text", true);
            return;
        }
        let rects = self.rects();
        let hits = nav::search(&self.tab().canvas, &rects, &q);
        if hits.is_empty() {
            self.status(&format!("not found: {q}"), true);
            return;
        }
        let ord = nav::order(&rects);
        let start = self.vim_focused().and_then(|f| ord.iter().position(|x| *x == f)).unwrap_or(0) as i32;
        let (mut at, mut left) = (start, n);
        // a fresh search can stay on the focused square if it matches
        if fresh && hits.contains(&ord[start as usize]) {
            left = 0;
        }
        let len = ord.len() as i32;
        let mut guard = 0;
        while left > 0 && guard < len * 2 {
            at = (at + dir).rem_euclid(len);
            if hits.contains(&ord[at as usize]) {
                left -= 1;
            }
            guard += 1;
        }
        let to = ord[at as usize];
        let k = hits.iter().position(|h| *h == to).map_or(0, |i| i + 1);
        self.vim_focus(to);
        self.status(&format!("/{q}  [{k} of {}]", hits.len()), false);
    }

    /// World point beside the focused square (the camera's center if none).
    fn vim_beside(&self, dx: f32, dy: f32, right: bool) -> Pos2 {
        let (_, tab, _) = &self.tabs[self.active];
        match self.vim_focused().and_then(|f| self.rects().get(&f).copied()) {
            Some(r) if right => pos2(r.max.x + dx, r.min.y + dy),
            Some(r) => pos2(r.min.x + dx, r.max.y + dy),
            None => pos2(tab.canvas.camera.x, tab.canvas.camera.y),
        }
    }

    /// `a`, `o` and `O` with a direction: the module search for a square
    /// beside the focus, free (`a`), wired from its output (`o`) or into its
    /// input (`O`). `o` puts the square to the right and `O` to the left, a
    /// gap off; down and up are half the focus's height plus a little off,
    /// so a square on each side don't collide; on the opposite side it
    /// touches. The new square's width isn't known yet: `vim_settle`
    /// finishes the job.
    fn vim_add(&mut self, wire: Wire, side: Option<Dir>) {
        const GAP: f32 = 48.0;
        const LITTLE: f32 = 16.0;
        const TIGHT: f32 = 8.0;
        let before = wire == Wire::Before;
        let focus = self.vim_focused();
        let pins = focus.map(|f| canvas::pins_of(self.tab(), f)).unwrap_or_default();
        // the first tick pin on that side, or else the first
        let pin = |output: bool| -> Option<PinRef> {
            let side: Vec<&PinRef> = pins.iter().filter(|p| p.output == output).collect();
            side.iter().find(|p| p.tick).or(side.first()).map(|p| (*p).clone())
        };
        let rect = focus.and_then(|f| self.rects().get(&f).copied());
        let mut edge = None;
        let at = match (rect, side) {
            (Some(r), Some(d)) => {
                let y = match d {
                    Dir::Down => r.min.y + r.height() / 2.0 + LITTLE,
                    Dir::Up => r.min.y - r.height() / 2.0 - LITTLE,
                    _ => r.min.y,
                };
                let (right, gap) = match d {
                    Dir::Right => (true, if before { TIGHT } else { GAP }),
                    Dir::Left => (false, if before { GAP } else { TIGHT }),
                    _ => (!before, GAP),
                };
                if right {
                    // `O` wires into the focus: add_square would shift it left
                    edge = before.then_some((r.max.x + gap, false));
                    pos2(r.max.x + gap, y)
                } else {
                    edge = Some((r.min.x - gap, true));
                    pos2(r.min.x - gap - if before { 0.0 } else { 200.0 }, y)
                }
            }
            _ => {
                let cam = &self.tab().canvas.camera;
                pos2(cam.x, cam.y)
            }
        };
        let pending = match (wire, rect) {
            (Wire::After, Some(_)) => pin(true).map(|p| Pending::From(p.sq, p.name, if p.tick { None } else { p.ty })),
            (Wire::Before, Some(_)) => pin(false).map(|p| Pending::To(p.sq, p.name, if p.tick { None } else { p.ty })),
            _ => None,
        };
        let st = &self.tabs[self.active].2;
        let screen = match (st.rect, &self.tabs[self.active].1.canvas.camera) {
            (Some(r), cam) => {
                let p = nav::to_screen(r, pos2(cam.x, cam.y), cam.zoom, at);
                pos2(
                    p.x.clamp(r.left() + 10.0, (r.right() - 400.0).max(r.left() + 10.0)),
                    p.y.clamp(r.top() + 10.0, (r.bottom() - 330.0).max(r.top() + 10.0)),
                )
            }
            _ => pos2(200.0, 200.0),
        };
        self.search =
            Some(Search { at, screen, query: String::new(), pending, edge, selected: 0, opened: Instant::now() });
    }

    /// Puts a square added with `oh` against the focus, now it's measured.
    fn vim_settle(&mut self) {
        let (_, tab, st) = &mut self.tabs[self.active];
        let Some((id, x, right)) = st.vim.settle else { return };
        match (st.rects.get(&id), tab.canvas.square_mut(id)) {
            (Some(r), Some(sq)) => sq.x = if right { x - r.width() } else { x },
            (None, Some(_)) => return,
            _ => {}
        }
        st.vim.settle = None;
    }

    fn vim_delete(&mut self, ctx: &egui::Context) {
        let removed = self.vim_targets();
        if removed.is_empty() {
            return;
        }
        let rects = self.rects();
        let next = self.vim_focused().and_then(|f| nav::after_delete(&rects, &removed, f));
        self.vim_yank(ctx);
        self.delete_selection();
        let (_, tab, st) = &mut self.tabs[self.active];
        for id in &removed {
            st.rects.remove(id);
        }
        st.vim.focus = next;
        st.vim.trail.clear();
        tab.selected = next.into_iter().collect();
        st.vim.sel = tab.selected.clone();
        st.vim.reveal = next;
    }

    fn vim_yank(&mut self, ctx: &egui::Context) {
        let n = self.tab().selected.len();
        match self.selection_text() {
            Some(t) => {
                ctx.copy_text(t.clone());
                self.register = Some(t);
                self.status(&format!("{n} square{} yanked", if n == 1 { "" } else { "s" }), false);
            }
            None => self.status("nothing to yank", false),
        }
    }

    fn vim_paste(&mut self, after: bool) {
        let Some(text) = self.register.clone() else {
            self.status("nothing yanked: yy or dd first", false);
            return;
        };
        let at = if after { self.vim_beside(32.0, 0.0, true) } else { self.vim_beside(0.0, 32.0, false) };
        self.paste(&text, Some(at));
        let (_, tab, st) = &mut self.tabs[self.active];
        st.vim.reveal = tab.selected.iter().next().copied();
    }

    fn vim_open_subgraph(&mut self) {
        let Some(f) = self.vim_focused() else { return };
        let Some(module) = self.tab().canvas.square(f).map(|s| s.module.clone()) else { return };
        match self.reg.get(&module).and_then(|m| m.subgraph()).and_then(|s| s.file.clone()) {
            Some(file) => self.open(&file),
            None => self.status(&format!("{module} is not a subgraph"), false),
        }
    }

    // ------------------------------------------------------------ pins

    /// The pins of the focused square that can be picked now.
    fn pin_cands(&self) -> Vec<PinRef> {
        let (_, tab, st) = &self.tabs[self.active];
        let (Some(p), Some(f)) = (&st.vim.pin, st.vim.focus) else { return Vec::new() };
        let all = canvas::pins_of(tab, f);
        match &p.from {
            None => all,
            Some(from) => all
                .into_iter()
                .filter(|c| c.output != from.output && canvas::can_wire(from.tick, c.tick, from.sq, c.sq))
                .collect(),
        }
    }

    /// A pin is picked: the first one starts a wire, the second ends it.
    fn vim_pin_choose(&mut self, pin: PinRef) {
        let from = self.tabs[self.active].2.vim.pin.as_ref().and_then(|p| p.from.clone());
        let Some(from) = from else {
            if let Some(p) = &mut self.tabs[self.active].2.vim.pin {
                p.from = Some(pin);
                p.cursor = None;
            }
            return;
        };
        // the same link the mouse makes: a data input takes only one wire
        let (link, exclusive) = if from.output {
            (Link { from: from.sq, output: from.name.clone(), to: pin.sq, input: pin.name.clone() }, !pin.tick)
        } else {
            (Link { from: pin.sq, output: pin.name.clone(), to: from.sq, input: from.name.clone() }, !from.tick)
        };
        self.tab().canvas.connect(link, exclusive);
        self.vim_pin_end(true);
        let module = |app: &App, id: SquareId| app.tabs[app.active].1.canvas.square(id).map(|s| s.module.clone());
        let (a, b) = (module(self, from.sq).unwrap_or_default(), module(self, pin.sq).unwrap_or_default());
        let (a, b) = if from.output {
            (format!("{a}.{}", from.name), format!("{b}.{}", pin.name))
        } else {
            (format!("{b}.{}", pin.name), format!("{a}.{}", from.name))
        };
        self.status(&format!("wired {a} to {b}"), false);
    }

    fn vim_pin_remove(&mut self) {
        let from = self.tabs[self.active].2.vim.pin.as_ref().and_then(|p| p.from.clone());
        let Some(from) = from else {
            self.status("pick a pin first (1-9), then x removes its wires", false);
            return;
        };
        let links = &mut self.tab().canvas.links;
        let before = links.len();
        links.retain(|l| {
            if from.output {
                !(l.from == from.sq && l.output == from.name)
            } else {
                !(l.to == from.sq && l.input == from.name)
            }
        });
        let n = before - links.len();
        self.vim_pin_end(true);
        self.status(&format!("{n} wire{} removed from {}", if n == 1 { "" } else { "s" }, from.name), false);
    }

    /// Wiring is over: Normal, with the focus back on the square it began
    /// from if BACK.
    fn vim_pin_end(&mut self, back: bool) {
        self.vim.mode = Mode::Normal;
        let from = self.tabs[self.active].2.vim.pin.take().and_then(|p| p.from);
        self.tabs[self.active].2.vim.hints.clear();
        match from {
            Some(f) if back => self.vim_focus(f.sq),
            _ => self.vim_collapse(),
        }
    }

    // ------------------------------------------------------------ Ex

    fn vim_complete(&mut self) {
        let line = self.vim.line.clone();
        let keys: Vec<&str> = SETTINGS.iter().map(|d| d.key).collect();
        let done = match line.trim_start().split_once(' ') {
            Some((head, rest)) if matches!(head, "e" | "edit" | "w" | "write") => {
                let full = if std::path::Path::new(rest).is_absolute() {
                    rest.to_string()
                } else {
                    format!("{}/{}", self.tab().dir().display(), rest)
                };
                let entries = completions(&full);
                complete(&full, &entries).map(|c| format!("{head} {c}"))
            }
            _ => vim::complete_ex(&line, &keys),
        };
        if let Some(l) = done {
            self.vim.line = l;
        }
    }

    fn in_canvas(&mut self) -> bool {
        if self.tab().settings {
            self.status("that works on a canvas: this is the Settings tab", true);
            return false;
        }
        true
    }

    fn resolve_path(&mut self, p: &str) -> PathBuf {
        let p = PathBuf::from(p);
        if p.is_relative() { self.tab().dir().join(p) } else { p }
    }

    fn vim_quit(&mut self, force: bool, ctx: &egui::Context) {
        if !force && self.tab().dirty() {
            self.status("this tab has unsaved changes: :w saves, :q! discards them", true);
            return;
        }
        if self.tabs.len() == 1 {
            self.quit_ok = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            self.close_tab(self.active);
        }
    }

    pub(super) fn vim_ex(&mut self, line: &str, ctx: &egui::Context) {
        let ex = match vim::parse_ex(line) {
            Ok(e) => e,
            Err(e) => {
                self.status(&e, true);
                return;
            }
        };
        match ex {
            Ex::Write(None) => self.save(ctx),
            Ex::Write(Some(p)) => {
                if !self.in_canvas() {
                    return;
                }
                let mut path = self.resolve_path(&p);
                if path.extension().is_none() {
                    path.set_extension("lua");
                }
                match self.tab().save_to(&path) {
                    Ok(()) => self.status(&format!("saved {}", path.display()), false),
                    Err(e) => self.status(&e, true),
                }
            }
            Ex::Quit { force } => self.vim_quit(force, ctx),
            Ex::QuitAll { force } => {
                let dirty = self.tabs.iter().filter(|(_, t, _)| t.dirty()).count();
                if !force && dirty > 0 {
                    self.status(
                        &format!(
                            "{dirty} tab{} with unsaved changes: :qa! discards them",
                            if dirty == 1 { "" } else { "s" }
                        ),
                        true,
                    );
                } else {
                    self.quit_ok = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            Ex::WriteQuit => {
                self.save(ctx);
                let t = self.tab();
                if t.settings || (t.path.is_some() && !t.dirty()) {
                    self.vim_quit(true, ctx);
                }
            }
            Ex::Edit(p) => {
                let path = self.resolve_path(&p);
                self.open(&path);
            }
            Ex::TabNew => {
                let t = Tab::empty(&self.reg);
                self.add_tab(t);
            }
            Ex::TabNext(None) => self.active = (self.active + 1) % self.tabs.len(),
            Ex::TabNext(Some(n)) => self.vim_act(Act::TabGo(n), ctx),
            Ex::TabPrev => self.active = (self.active + self.tabs.len() - 1) % self.tabs.len(),
            Ex::TabClose { force } => {
                if !force && self.tab().dirty() {
                    self.status("this tab has unsaved changes: :w saves, :tabc! discards them", true);
                } else {
                    self.close_tab(self.active);
                }
            }
            Ex::Goto(n) => {
                if self.in_canvas() {
                    self.vim_act(Act::Last(Some(n)), ctx);
                }
            }
            Ex::Run => self.start(RunMode::Run, ctx),
            Ex::Step => self.start(RunMode::Step, ctx),
            Ex::Test => self.run_tests(),
            Ex::Debug(name) => self.debug_named(name.as_deref(), ctx),
            Ex::Stop => self.stop(),
            Ex::Compile => self.start(RunMode::Compile, ctx),
            Ex::Output => self.show_output = !self.show_output,
            Ex::ClearOutput => self.tab().output.clear(),
            Ex::Add(name) => {
                if !self.in_canvas() {
                    return;
                }
                let q = name.to_lowercase().replace(' ', "_");
                let best = if self.reg.get(&name).is_some() {
                    Some(name.clone())
                } else {
                    self.reg
                        .modules
                        .keys()
                        .filter_map(|p| search_score(p, &q).map(|s| (s, p.clone())))
                        .min()
                        .map(|b| b.1)
                };
                match best {
                    Some(path) => {
                        let at = self.vim_beside(48.0, 0.0, true);
                        self.add_square(&path, at, None);
                    }
                    None => self.status(&format!("no module matches {name}"), true),
                }
            }
            Ex::Set(op) => self.vim_set(op, ctx),
            Ex::Settings => self.open_settings(),
            Ex::Subgraph => {
                if self.in_canvas() {
                    self.vim_targets();
                    self.subgraph_of = self.tab().selected.clone();
                    if self.subgraph_of.is_empty() {
                        self.status("nothing to make a subgraph of", true);
                    } else {
                        self.ask(PromptKind::SubgraphName, ctx);
                    }
                }
            }
            Ex::Disconnect => {
                if self.in_canvas() {
                    self.vim_targets();
                    canvas::disconnect(self.tab());
                }
            }
            Ex::Dup => {
                if self.in_canvas() {
                    self.vim_targets();
                    if let Some(t) = self.selection_text() {
                        self.paste(&t, None);
                    }
                }
            }
            Ex::OpenSubgraph => self.vim_open_subgraph(),
            Ex::Undo => self.vim_act(Act::Undo(1), ctx),
            Ex::Redo => self.vim_act(Act::Redo(1), ctx),
            Ex::Help => {
                self.help = true;
                self.help_offset = 0.0;
            }
        }
    }

    /// Saves the settings and applies them, as the Settings tab does.
    pub(super) fn apply_cfg(&mut self, ctx: &egui::Context) {
        if let Err(e) = self.cfg.save() {
            self.status(&e, true);
        }
        self.theme = Theme::from_config(&self.cfg);
        self.apply_style(ctx);
    }

    fn vim_set(&mut self, op: SetOp, ctx: &egui::Context) {
        let result = (|| -> Result<Option<(&'static str, SValue)>, String> {
            match op {
                SetOp::Show(name) => {
                    let d = resolve_setting(&name)?;
                    Err(format!("{}={}", d.key, show_value(&self.cfg.get(d.key))))
                }
                SetOp::On(name) => {
                    // `nogrid`: off, unless there's a setting called that
                    let (d, on) = match resolve_setting(&name) {
                        Ok(d) => (d, true),
                        Err(e) => match name.strip_prefix("no").map(resolve_setting) {
                            Some(Ok(d)) => (d, false),
                            _ => return Err(e),
                        },
                    };
                    match d.kind {
                        SKind::Bool => Ok(Some((d.key, SValue::Bool(on)))),
                        _ => Err(format!("{}={}", d.key, show_value(&self.cfg.get(d.key)))),
                    }
                }
                SetOp::Toggle(name) => {
                    let d = resolve_setting(&name)?;
                    match d.kind {
                        SKind::Bool => Ok(Some((d.key, SValue::Bool(!self.cfg.bool(d.key))))),
                        _ => Err(format!("{} is not on or off", d.key)),
                    }
                }
                SetOp::Assign(name, v) => {
                    let d = resolve_setting(&name)?;
                    let value = match &d.kind {
                        SKind::Bool => SValue::Bool(parse_bool(&v).ok_or_else(|| format!("{v}: on or off"))?),
                        SKind::Num { min, max, .. } => {
                            let n: f64 = v.parse().map_err(|_| format!("{v}: a number"))?;
                            if !n.is_finite() {
                                return Err(format!("{v}: a number"));
                            }
                            SValue::Num(n.clamp(*min, *max))
                        }
                        SKind::Theme => {
                            if !theme::NAMES.contains(&v.as_str()) && !self.cfg.themes.contains_key(&v) {
                                return Err(format!("no theme called {v}"));
                            }
                            SValue::Text(v)
                        }
                        SKind::Text | SKind::Font => SValue::Text(v),
                    };
                    Ok(Some((d.key, value)))
                }
            }
        })();
        match result {
            Ok(Some((key, v))) => {
                let shown = show_value(&v);
                self.cfg.set(key, v);
                self.apply_cfg(ctx);
                self.status(&format!("{key}={shown}"), false);
            }
            Ok(None) => {}
            // a value to show, or a mistake
            Err(msg) => {
                let error = !msg.contains('=');
                self.status(&msg, error);
            }
        }
    }

    // ------------------------------------------------------------ bar

    /// The mode, what's half typed and the command line, at the bottom.
    pub(super) fn vim_status_ui(&mut self, ui: &mut egui::Ui) {
        if !self.vim_on() {
            return;
        }
        let theme = self.theme.clone();
        let st = &self.tabs[self.active].2;
        let pin_from = st.vim.pin.as_ref().map(|p| p.from.is_some());
        let (name, color): (String, Color32) = if self.help {
            ("HELP".into(), theme.dim)
        } else if self.search.is_some() {
            ("ADD".into(), theme.bool)
        } else if self.prompt.is_some() {
            ("PROMPT".into(), theme.bool)
        } else if st.edit.is_some() {
            ("INSERT".into(), theme.bool)
        } else {
            match self.vim.mode {
                Mode::Normal if self.tabs[self.active].1.settings => ("SETTINGS".into(), theme.accent),
                Mode::Normal => ("NORMAL".into(), theme.accent),
                Mode::Visual => ("VISUAL".into(), theme.selection),
                Mode::Pin => ("PIN".into(), theme.step),
                Mode::Line(_) => ("COMMAND".into(), theme.dim),
            }
        };
        ui.label(
            egui::RichText::new(format!(" {name} ")).monospace().strong().color(Color32::BLACK).background_color(color),
        );
        if let Mode::Line(c) = self.vim.mode {
            ui.label(egui::RichText::new(format!("{c}{}▏", self.vim.line)).monospace());
        } else if self.vim.mode == Mode::Pin {
            let hint = match pin_from {
                Some(true) => {
                    "go to the other square (h j k l w b), 1-9 or Tab+Enter picks its pin, x removes, Esc cancels"
                }
                _ => "1-9 or Tab+Enter picks a pin of this square, Esc cancels",
            };
            ui.label(egui::RichText::new(hint).color(theme.dim));
        } else {
            let pending = self.vim.pending_text();
            if !pending.is_empty() {
                ui.label(egui::RichText::new(pending).monospace());
            }
        }
        ui.separator();
    }

    /// `:help`: the keys.
    pub(super) fn vim_help_ui(&mut self, ctx: &egui::Context) {
        if !self.help {
            return;
        }
        let mut open = true;
        egui::Window::new("Vim mode")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([640.0, 560.0])
            .show(ctx, |ui| {
                let out = egui::ScrollArea::vertical()
                    .vertical_scroll_offset(self.help_offset)
                    .max_height(ui.available_height() - 24.0)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(vim::HELP).monospace());
                    });
                // what the window made of it (it clamps), and the mouse's scrolling
                self.help_offset = out.state.clamped_offset().y;
                ui.weak("j k scroll, d u half pages, gg G, Esc or q closes");
            });
        if !open {
            self.help = false;
        }
    }

    /// The Settings tab: what the keys queued since the last frame, done to
    /// the row SEL of SHOWN. Returns the row the cursor is on, and the key
    /// of a setting to change, with what to do to it.
    pub(super) fn settings_cursor(
        &mut self,
        shown: &[&SettingDef],
        sections: &[&'static str],
    ) -> (usize, Option<SAct>, bool) {
        let ops = std::mem::take(&mut self.settings_ops);
        let n = shown.len();
        // a search nothing matches: no row to be on
        if n == 0 {
            return (0, None, false);
        }
        let mut sel = self.settings_sel.min(n.saturating_sub(1));
        let mut act = None;
        let mut moved = false;
        for op in ops {
            match op {
                SAct::Rows(d) => {
                    sel = (sel as i32 + d).clamp(0, n as i32 - 1).max(0) as usize;
                    moved = true;
                }
                SAct::First => {
                    sel = 0;
                    moved = true;
                }
                SAct::Last => {
                    sel = n.saturating_sub(1);
                    moved = true;
                }
                SAct::Section(d) => {
                    let here = shown.get(sel).map(|s| super::section(s.key));
                    let i = sections.iter().position(|s| Some(*s) == here).unwrap_or(0) as i32;
                    let j = (i + d).clamp(0, sections.len() as i32 - 1).max(0) as usize;
                    if let Some(s) = sections.get(j) {
                        sel = shown.iter().position(|x| super::section(x.key) == *s).unwrap_or(sel);
                        moved = true;
                    }
                }
                SAct::Search => self.settings_focus = true,
                other => act = Some(other),
            }
        }
        self.settings_sel = sel;
        (sel, act, moved)
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui::Modifiers;

    use super::*;

    fn key(k: Key, m: Modifiers, pressed: bool) -> Event {
        Event::Key { key: k, physical_key: None, pressed, repeat: false, modifiers: m }
    }

    fn is(c: Class, want: Option<Input>) -> bool {
        match (c, want) {
            (Class::Key(a), Some(b)) => a == b,
            (Class::Drop, None) => true,
            _ => false,
        }
    }

    #[test]
    fn letters_come_as_text_and_their_keys_are_dropped() {
        let n = Mode::Normal;
        assert!(is(classify(&Event::Text("j".into()), n), Some(Input::Char('j'))));
        assert!(is(classify(&Event::Text("J".into()), n), Some(Input::Char('J'))));
        // the key of the same press, and its release, say nothing more
        assert!(is(classify(&key(Key::J, Modifiers::NONE, true), n), None));
        assert!(is(classify(&key(Key::J, Modifiers::SHIFT, true), n), None));
        assert!(is(classify(&key(Key::J, Modifiers::NONE, false), n), None));
        assert!(is(classify(&key(Key::Colon, Modifiers::SHIFT, true), n), None));
        assert!(is(classify(&key(Key::Space, Modifiers::NONE, true), n), None));
    }

    #[test]
    fn named_keys() {
        let n = Mode::Normal;
        assert!(is(classify(&key(Key::Escape, Modifiers::NONE, true), n), Some(Input::Esc)));
        assert!(is(classify(&key(Key::Escape, Modifiers::NONE, false), n), None));
        assert!(is(classify(&key(Key::Enter, Modifiers::NONE, true), n), Some(Input::Enter)));
        assert!(is(classify(&key(Key::Tab, Modifiers::NONE, true), n), Some(Input::Tab)));
        assert!(is(classify(&key(Key::Tab, Modifiers::SHIFT, true), n), Some(Input::BackTab)));
        assert!(is(classify(&key(Key::ArrowUp, Modifiers::NONE, true), n), Some(Input::Up)));
        assert!(is(classify(&key(Key::ArrowLeft, Modifiers::NONE, true), n), Some(Input::Left)));
    }

    #[test]
    fn keys_that_are_not_vims_stay_with_the_editor() {
        let n = Mode::Normal;
        for k in [Key::F5, Key::F10, Key::Delete, Key::Home, Key::PageDown, Key::Copy, Key::Paste] {
            assert!(matches!(classify(&key(k, Modifiers::NONE, true), n), Class::Pass), "{k:?}");
        }
        // Shift+F5 stops the run
        assert!(matches!(classify(&key(Key::F5, Modifiers::SHIFT, true), n), Class::Pass));
        // Backspace deletes squares in Normal, and edits the command line
        assert!(matches!(classify(&key(Key::Backspace, Modifiers::NONE, true), n), Class::Pass));
        assert!(is(classify(&key(Key::Backspace, Modifiers::NONE, true), Mode::Line(':')), Some(Input::Backspace)));
        // text that isn't one character (an IME's), and pointers
        assert!(matches!(classify(&Event::Text("ab".into()), n), Class::Pass));
        assert!(matches!(classify(&Event::Copy, n), Class::Pass));
    }

    #[test]
    fn ctrl_keys_split_between_vim_and_the_editor() {
        let n = Mode::Normal;
        let ctrl = Modifiers::CTRL;
        // vim's: panning and redo
        assert!(is(classify(&key(Key::E, ctrl, true), n), Some(Input::Ctrl('e'))));
        assert!(is(classify(&key(Key::D, ctrl, true), n), Some(Input::Ctrl('d'))));
        assert!(is(classify(&key(Key::R, ctrl, true), n), Some(Input::Ctrl('r'))));
        assert!(is(classify(&key(Key::OpenBracket, ctrl, true), n), Some(Input::Ctrl('['))));
        // the editor's: save, new tab, close tab, undo, select all, open
        for k in [Key::S, Key::T, Key::W, Key::Z, Key::A, Key::O, Key::G, Key::X, Key::V] {
            assert!(matches!(classify(&key(k, ctrl, true), n), Class::Pass), "{k:?}");
        }
        // on the command line Ctrl+W erases a word, not a tab
        assert!(is(classify(&key(Key::W, ctrl, true), Mode::Line(':')), Some(Input::Ctrl('w'))));
        assert!(matches!(classify(&key(Key::E, ctrl, true), Mode::Line(':')), Class::Pass));
    }

    #[test]
    fn setting_names() {
        assert_eq!(resolve_setting("editor.vim_mode").map(|d| d.key), Ok("editor.vim_mode"));
        assert_eq!(resolve_setting("vim_mode").map(|d| d.key), Ok("editor.vim_mode"));
        assert_eq!(resolve_setting("snap").map(|d| d.key), Ok("canvas.snap"));
        assert!(resolve_setting("nothing_like_it").is_err());
        // every setting can be reached by its full key
        for d in SETTINGS {
            assert_eq!(resolve_setting(d.key).map(|x| x.key), Ok(d.key));
        }
        assert_eq!(parse_bool("on"), Some(true));
        assert_eq!(parse_bool("0"), Some(false));
        assert_eq!(parse_bool("maybe"), None);
    }
}

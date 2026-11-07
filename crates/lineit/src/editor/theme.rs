//! Colors. Four built in; `themes` in the config adds more, each color
//! left out being dark's.

use eframe::egui::{self, Color32};
use lineit_core::config::{Config, ThemeColors};
use lineit_rt::Type;

#[derive(Clone)]
pub struct Theme {
    pub dark: bool,
    pub background: Color32,
    pub grid: Color32,
    pub grid_major: Color32,
    pub square: Color32,
    pub header: Color32,
    pub border: Color32,
    pub text: Color32,
    pub dim: Color32,
    pub field: Color32,
    pub accent: Color32,
    pub selection: Color32,
    pub error: Color32,
    pub step: Color32,
    pub panel: Color32,
    pub tick: Color32,
    pub int: Color32,
    pub float: Color32,
    pub bool: Color32,
    pub str: Color32,
    pub list: Color32,
    pub map: Color32,
    pub image: Color32,
    pub record: Color32,
}

const fn c(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

pub const NAMES: &[&str] = &["dark", "light", "nord", "gruvbox"];

pub fn dark() -> Theme {
    Theme {
        dark: true,
        background: c(24, 25, 29),
        grid: c(32, 34, 39),
        grid_major: c(42, 44, 51),
        square: c(44, 47, 55),
        header: c(56, 60, 71),
        border: c(70, 75, 88),
        text: c(222, 224, 230),
        dim: c(140, 145, 158),
        field: c(30, 32, 37),
        accent: c(110, 160, 255),
        selection: c(110, 160, 255),
        error: c(240, 90, 90),
        step: c(250, 210, 70),
        panel: c(32, 34, 39),
        tick: c(230, 230, 235),
        int: c(100, 150, 250),
        float: c(80, 200, 220),
        bool: c(240, 150, 70),
        str: c(120, 200, 110),
        list: c(180, 130, 240),
        map: c(240, 120, 180),
        image: c(240, 210, 90),
        record: c(170, 175, 185),
    }
}

fn light() -> Theme {
    Theme {
        dark: false,
        background: c(244, 244, 240),
        grid: c(232, 232, 226),
        grid_major: c(218, 218, 210),
        square: c(255, 255, 255),
        header: c(228, 230, 236),
        border: c(180, 184, 194),
        text: c(30, 32, 38),
        dim: c(110, 115, 125),
        field: c(242, 243, 246),
        accent: c(40, 100, 220),
        selection: c(40, 100, 220),
        error: c(210, 40, 40),
        step: c(220, 160, 0),
        panel: c(232, 232, 228),
        tick: c(40, 42, 48),
        int: c(40, 90, 210),
        float: c(0, 140, 160),
        bool: c(210, 110, 20),
        str: c(40, 140, 40),
        list: c(130, 70, 200),
        map: c(200, 60, 130),
        image: c(180, 140, 0),
        record: c(100, 105, 115),
    }
}

fn nord() -> Theme {
    Theme {
        background: c(46, 52, 64),
        grid: c(52, 58, 71),
        grid_major: c(59, 66, 82),
        square: c(59, 66, 82),
        header: c(67, 76, 94),
        border: c(76, 86, 106),
        text: c(236, 239, 244),
        dim: c(160, 170, 190),
        field: c(46, 52, 64),
        accent: c(136, 192, 208),
        selection: c(136, 192, 208),
        error: c(191, 97, 106),
        step: c(235, 203, 139),
        panel: c(59, 66, 82),
        tick: c(229, 233, 240),
        int: c(129, 161, 193),
        float: c(143, 188, 187),
        bool: c(208, 135, 112),
        str: c(163, 190, 140),
        list: c(180, 142, 173),
        map: c(191, 97, 106),
        image: c(235, 203, 139),
        record: c(216, 222, 233),
        ..dark()
    }
}

fn gruvbox() -> Theme {
    Theme {
        background: c(40, 40, 40),
        grid: c(50, 48, 47),
        grid_major: c(60, 56, 54),
        square: c(60, 56, 54),
        header: c(80, 73, 69),
        border: c(102, 92, 84),
        text: c(235, 219, 178),
        dim: c(168, 153, 132),
        field: c(40, 40, 40),
        accent: c(250, 189, 47),
        selection: c(250, 189, 47),
        error: c(251, 73, 52),
        step: c(184, 187, 38),
        panel: c(50, 48, 47),
        tick: c(235, 219, 178),
        int: c(131, 165, 152),
        float: c(142, 192, 124),
        bool: c(254, 128, 25),
        str: c(184, 187, 38),
        list: c(211, 134, 155),
        map: c(251, 73, 52),
        image: c(250, 189, 47),
        record: c(189, 174, 147),
        ..dark()
    }
}

impl Theme {
    pub fn from_config(cfg: &Config) -> Theme {
        let name = cfg.text("theme");
        let mut t = match name.as_str() {
            "light" => light(),
            "nord" => nord(),
            "gruvbox" => gruvbox(),
            _ => dark(),
        };
        if let Some(custom) = cfg.themes.get(&name) {
            t.apply(custom);
        }
        t
    }

    fn apply(&mut self, colors: &ThemeColors) {
        for (k, [r, g, b, a]) in colors {
            let v = Color32::from_rgba_unmultiplied(*r, *g, *b, *a);
            let slot = match k.as_str() {
                "background" => &mut self.background,
                "grid" => &mut self.grid,
                "grid_major" => &mut self.grid_major,
                "square" => &mut self.square,
                "header" => &mut self.header,
                "border" => &mut self.border,
                "text" => &mut self.text,
                "dim" => &mut self.dim,
                "field" => &mut self.field,
                "accent" => &mut self.accent,
                "selection" => &mut self.selection,
                "error" => &mut self.error,
                "step" => &mut self.step,
                "panel" => &mut self.panel,
                "tick" => &mut self.tick,
                "int" => &mut self.int,
                "float" => &mut self.float,
                "bool" => &mut self.bool,
                "str" => &mut self.str,
                "list" => &mut self.list,
                "map" => &mut self.map,
                "image" => &mut self.image,
                "record" => &mut self.record,
                _ => continue,
            };
            *slot = v;
        }
        let l = self.background.r() as u32 + self.background.g() as u32 + self.background.b() as u32;
        self.dark = l < 384;
    }

    /// The color of a wire or pin of type T (None: a tick).
    pub fn of(&self, t: Option<&Type>) -> Color32 {
        match t {
            None => self.tick,
            Some(Type::Int) => self.int,
            Some(Type::Float) => self.float,
            Some(Type::Bool) => self.bool,
            Some(Type::Str) => self.str,
            Some(Type::List(_)) => self.list,
            Some(Type::Map(_)) => self.map,
            Some(Type::Image) => self.image,
            Some(Type::Record(_)) => self.record,
        }
    }

    /// egui's own colors, from these.
    pub fn visuals(&self) -> egui::Visuals {
        let mut v = if self.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
        v.panel_fill = self.panel;
        v.window_fill = self.panel;
        v.extreme_bg_color = self.field;
        v.override_text_color = Some(self.text);
        v.selection.bg_fill = self.selection.gamma_multiply(0.45);
        v.hyperlink_color = self.accent;
        v.widgets.noninteractive.bg_stroke.color = self.border;
        v
    }
}

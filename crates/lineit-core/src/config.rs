//! Settings. `config.lua` (in [`crate::paths::config_dir`]) is real Lua that
//! sets them, as globals; `settings.lua`, next to it, is what File >
//! Settings changed, read after it, so what's there wins. Every setting has
//! a default, so neither file has to exist.
//!
//! ```lua
//! theme = "nord"
//! font = { size = 15 }
//! themes = { mine = { background = { 20, 20, 30 }, text = { 220, 220, 220 } } }
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub enum SValue {
    Bool(bool),
    Num(f64),
    Text(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Bool,
    Num {
        min: f64,
        max: f64,
        step: f64,
    },
    Text,
    /// One of the themes.
    Theme,
    /// A font: a name, or a file.
    Font,
}

pub struct SettingDef {
    /// `table.key`, or `key` for a global.
    pub key: &'static str,
    pub doc: &'static str,
    pub kind: Kind,
    pub default: fn() -> SValue,
}

macro_rules! num {
    ($min:expr, $max:expr, $step:expr) => {
        Kind::Num { min: $min, max: $max, step: $step }
    };
}

pub const SETTINGS: &[SettingDef] = &[
    SettingDef {
        key: "theme",
        doc: "dark, light, nord, gruvbox, or one in `themes`",
        kind: Kind::Theme,
        default: || SValue::Text("dark".into()),
    },
    SettingDef {
        key: "font.size",
        doc: "Size of the text, in points",
        kind: num!(8.0, 32.0, 1.0),
        default: || SValue::Num(14.0),
    },
    SettingDef {
        key: "font.name",
        doc: "The font of the text: one installed, by its name (Iosevka NFM), or a .ttf or .otf file. Empty: the built-in one",
        kind: Kind::Font,
        default: || SValue::Text(String::new()),
    },
    SettingDef {
        key: "editor.vim_mode",
        doc: "Vim keys: move over the squares with h j k l, : for commands. :help lists them",
        kind: Kind::Bool,
        default: || SValue::Bool(false),
    },
    SettingDef {
        key: "editor.native_dialogs",
        doc: "Open and Save as use the system's file dialog; off, or where there's none (no portal, a container), a path prompt with Tab completion",
        kind: Kind::Bool,
        default: || SValue::Bool(true),
    },
    SettingDef { key: "canvas.grid", doc: "Draw the grid", kind: Kind::Bool, default: || SValue::Bool(true) },
    SettingDef {
        key: "canvas.grid_step",
        doc: "Canvas units between grid lines; squares snap to it",
        kind: num!(4.0, 128.0, 4.0),
        default: || SValue::Num(16.0),
    },
    SettingDef {
        key: "canvas.snap",
        doc: "Squares snap to the grid when moved",
        kind: Kind::Bool,
        default: || SValue::Bool(true),
    },
    SettingDef {
        key: "canvas.square_width",
        doc: "The widest a square gets: they're as wide as what's in them",
        kind: num!(120.0, 400.0, 10.0),
        default: || SValue::Num(200.0),
    },
    SettingDef {
        key: "canvas.zoom_min",
        doc: "How far you can zoom out",
        kind: num!(0.05, 1.0, 0.05),
        default: || SValue::Num(0.2),
    },
    SettingDef {
        key: "canvas.zoom_max",
        doc: "How far you can zoom in",
        kind: num!(1.0, 10.0, 0.5),
        default: || SValue::Num(3.0),
    },
    SettingDef {
        key: "canvas.wire_width",
        doc: "Width of wires",
        kind: num!(1.0, 8.0, 0.5),
        default: || SValue::Num(3.0),
    },
    SettingDef {
        key: "canvas.previews",
        doc: "Show images on the squares that make them",
        kind: Kind::Bool,
        default: || SValue::Bool(true),
    },
    SettingDef {
        key: "save.autosave",
        doc: "Save tabs with changes on their own",
        kind: Kind::Bool,
        default: || SValue::Bool(false),
    },
    SettingDef {
        key: "save.interval",
        doc: "Seconds between autosaves",
        kind: num!(5.0, 600.0, 5.0),
        default: || SValue::Num(30.0),
    },
    SettingDef {
        key: "output.clear_on_run",
        doc: "Empty the output when a run starts",
        kind: Kind::Bool,
        default: || SValue::Bool(true),
    },
    SettingDef {
        key: "output.max_lines",
        doc: "Lines the output keeps",
        kind: num!(100.0, 1_000_000.0, 100.0),
        default: || SValue::Num(10_000.0),
    },
    SettingDef {
        key: "window.width",
        doc: "Width of the window when it opens",
        kind: num!(400.0, 8000.0, 10.0),
        default: || SValue::Num(1280.0),
    },
    SettingDef {
        key: "window.height",
        doc: "Height of the window when it opens",
        kind: num!(300.0, 8000.0, 10.0),
        default: || SValue::Num(800.0),
    },
];

/// A theme's colors, by name (see the editor's `theme.rs` for the names).
pub type ThemeColors = BTreeMap<String, [u8; 4]>;

#[derive(Clone, Debug, Default)]
pub struct Config {
    pub values: BTreeMap<&'static str, SValue>,
    pub themes: BTreeMap<String, ThemeColors>,
    /// What went wrong reading the files.
    pub errors: Vec<String>,
    /// Where `settings.lua` goes.
    pub dir: PathBuf,
}

fn def(key: &str) -> Option<&'static SettingDef> {
    SETTINGS.iter().find(|d| d.key == key)
}

impl Config {
    pub fn defaults() -> Config {
        Config {
            values: SETTINGS.iter().map(|d| (d.key, (d.default)())).collect(),
            dir: crate::paths::config_dir(),
            ..Config::default()
        }
    }

    /// Reads `config.lua` and `settings.lua` in DIR.
    pub fn load(dir: &Path) -> Config {
        let mut c = Config::defaults();
        c.dir = dir.to_path_buf();
        let lua = mlua::Lua::new();
        for name in ["config.lua", "settings.lua"] {
            let p = dir.join(name);
            let Ok(src) = std::fs::read_to_string(&p) else { continue };
            if let Err(e) = lua.load(&src).set_name(format!("@{}", p.display())).exec() {
                c.errors.push(e.to_string());
            }
        }
        let g = lua.globals();
        for d in SETTINGS {
            let v: Option<mlua::Value> = match d.key.split_once('.') {
                Some((t, k)) => g.get::<Option<mlua::Table>>(t).ok().flatten().and_then(|t| t.get(k).ok()),
                None => g.get(d.key).ok(),
            };
            let v = match (v, &d.kind) {
                (None | Some(mlua::Value::Nil), _) => continue,
                (Some(mlua::Value::Boolean(b)), Kind::Bool) => SValue::Bool(b),
                (Some(mlua::Value::Integer(i)), Kind::Num { min, max, .. }) => {
                    SValue::Num((i as f64).clamp(*min, *max))
                }
                (Some(mlua::Value::Number(n)), Kind::Num { min, max, .. }) => SValue::Num(n.clamp(*min, *max)),
                (Some(mlua::Value::String(s)), Kind::Text | Kind::Theme | Kind::Font) => {
                    SValue::Text(s.to_string_lossy())
                }
                (Some(v), _) => {
                    c.errors.push(format!("{}: a {} isn't what this setting takes", d.key, v.type_name()));
                    continue;
                }
            };
            c.values.insert(d.key, v);
        }
        if let Ok(Some(themes)) = g.get::<Option<mlua::Table>>("themes") {
            for pair in themes.pairs::<String, mlua::Table>() {
                let Ok((name, t)) = pair else {
                    c.errors.push("themes: each is a table of colors".into());
                    continue;
                };
                let mut colors = ThemeColors::new();
                for pair in t.pairs::<String, Vec<u8>>() {
                    match pair {
                        Ok((k, rgb)) if rgb.len() == 3 || rgb.len() == 4 => {
                            colors.insert(k, [rgb[0], rgb[1], rgb[2], *rgb.get(3).unwrap_or(&255)]);
                        }
                        _ => c
                            .errors
                            .push(format!("themes.{name}: colors are {{ r, g, b }} or {{ r, g, b, a }}, 0 to 255")),
                    }
                }
                c.themes.insert(name, colors);
            }
        }
        c
    }

    pub fn get(&self, key: &str) -> SValue {
        self.values.get(key).cloned().or_else(|| def(key).map(|d| (d.default)())).unwrap_or(SValue::Bool(false))
    }
    pub fn bool(&self, key: &str) -> bool {
        matches!(self.get(key), SValue::Bool(true))
    }
    pub fn num(&self, key: &str) -> f64 {
        match self.get(key) {
            SValue::Num(n) => n,
            _ => 0.0,
        }
    }
    pub fn text(&self, key: &str) -> String {
        match self.get(key) {
            SValue::Text(s) => s,
            _ => String::new(),
        }
    }

    pub fn set(&mut self, key: &'static str, v: SValue) {
        self.values.insert(key, v);
    }

    /// Writes the settings that aren't the defaults to `settings.lua`.
    pub fn save(&self) -> Result<(), String> {
        let mut globals = String::new();
        let mut tables: BTreeMap<&str, Vec<String>> = BTreeMap::new();
        for d in SETTINGS {
            let v = self.get(d.key);
            if v == (d.default)() {
                continue;
            }
            let text = match &v {
                SValue::Bool(b) => b.to_string(),
                SValue::Num(n) => format!("{n}"),
                SValue::Text(s) => crate::file::quote(s),
            };
            match d.key.split_once('.') {
                Some((t, k)) => tables.entry(t).or_default().push(format!("{} = {text}", crate::file::key(k))),
                None => globals.push_str(&format!("{} = {text}\n", d.key)),
            }
        }
        let mut s = String::from(
            "-- What File > Settings in lineit changed. lineit writes it, and reads it\n\
             -- after config.lua: what's here wins. Take a line out to use config.lua's.\n\n",
        );
        s.push_str(&globals);
        for (t, items) in tables {
            s.push_str(&format!("{t} = {t} or {{}}\n"));
            for i in items {
                s.push_str(&format!("{t}.{i}\n"));
            }
        }
        std::fs::create_dir_all(&self.dir).map_err(|e| format!("{}: {e}", self.dir.display()))?;
        let p = self.dir.join("settings.lua");
        let tmp = p.with_extension("lua.tmp");
        std::fs::write(&tmp, s).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &p).map_err(|e| format!("{}: {e}", p.display()))
    }
}

/// A `config.lua` with every setting, commented out, at its default.
pub fn default_config_text() -> String {
    let mut s = String::from(
        "-- lineit configuration: put it in the config folder (`lineit config` says\n\
         -- where) and change what you like. It's Lua; every setting has a default, so\n\
         -- any line can go. File > Settings (Ctrl+,) changes them too, in settings.lua.\n\n",
    );
    let mut table = "";
    for d in SETTINGS {
        let v = match (d.default)() {
            SValue::Bool(b) => b.to_string(),
            SValue::Num(n) => format!("{n}"),
            SValue::Text(t) => crate::file::quote(&t),
        };
        match d.key.split_once('.') {
            Some((t, k)) => {
                if t != table {
                    if !table.is_empty() {
                        s.push_str("-- }\n");
                    }
                    s.push_str(&format!("\n-- {t} = {{\n"));
                    table = t;
                }
                s.push_str(&format!("--         {k} = {v}, -- {}\n", d.doc));
            }
            None => s.push_str(&format!("-- {} = {v} -- {}\n", d.key, d.doc)),
        }
    }
    if !table.is_empty() {
        s.push_str("-- }\n");
    }
    s.push_str(
        "\n-- Your own themes: any color left out is the dark theme's.\n\
         -- themes = {\n--         mine = { background = { 24, 24, 32 }, square = { 40, 40, 54 }, accent = { 255, 180, 80 } },\n-- }\n",
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_and_save() {
        let dir = std::env::temp_dir().join(format!("lineit-config-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("config.lua"),
            "theme = 'nord'\nfont = { size = 99 }\nthemes = { x = { text = { 1, 2, 3 } } }",
        )
        .unwrap();
        let mut c = Config::load(&dir);
        assert!(c.errors.is_empty(), "{:?}", c.errors);
        assert_eq!(c.text("theme"), "nord");
        assert_eq!(c.num("font.size"), 32.0);
        assert_eq!(c.themes["x"]["text"], [1, 2, 3, 255]);
        c.set("canvas.grid", SValue::Bool(false));
        c.save().unwrap();
        let c = Config::load(&dir);
        assert!(!c.bool("canvas.grid"));
        assert_eq!(c.text("theme"), "nord");
        std::fs::remove_dir_all(&dir).unwrap();
        // the default text is valid Lua that changes nothing
        mlua::Lua::new().load(default_config_text()).exec().unwrap();
    }
}

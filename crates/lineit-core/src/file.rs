//! Canvas files, and the other data files lineit reads (record types,
//! package manifests): Lua that returns a table.
//!
//! They're read in a sandbox: no libraries at all (no `io`, `os`,
//! `require`), at most [`MEMORY`] bytes and [`INSTRUCTIONS`] instructions,
//! so opening a canvas someone sent can't do anything but describe a
//! canvas. The config is different: it's real Lua (see [`crate::config`]).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use mlua::{HookTriggers, Lua, LuaOptions, StdLib, Table, VmState};

use crate::model::{Camera, Canvas, Link, Square};

/// The canvas format this lineit writes and reads.
pub const FORMAT: i64 = 1;

pub const MEMORY: usize = 256 << 20;
pub const INSTRUCTIONS: u64 = 100_000_000;

/// Runs SOURCE in the sandbox and gives F the table it returns.
pub fn with_data<R>(source: &str, name: &str, f: impl FnOnce(&Table) -> Result<R, String>) -> Result<R, String> {
    let lua = Lua::new_with(StdLib::NONE, LuaOptions::default()).map_err(|e| e.to_string())?;
    lua.set_memory_limit(MEMORY).map_err(|e| e.to_string())?;
    let count = std::sync::atomic::AtomicU64::new(0);
    lua.set_hook(HookTriggers::new().every_nth_instruction(10_000), move |_, _| {
        if count.fetch_add(10_000, std::sync::atomic::Ordering::Relaxed) > INSTRUCTIONS {
            Err(mlua::Error::runtime("it runs for too long"))
        } else {
            Ok(VmState::Continue)
        }
    })
    .map_err(|e| e.to_string())?;
    let env = lua.create_table().map_err(|e| e.to_string())?;
    let t: Table = lua
        .load(source)
        .set_name(format!("@{name}"))
        .set_environment(env)
        .eval()
        .map_err(|e| format!("{name}: {e}"))?;
    f(&t).map_err(|e| format!("{name}: {e}"))
}

fn get<T: mlua::FromLua>(t: &Table, k: &str) -> Result<Option<T>, String> {
    t.get::<Option<T>>(k).map_err(|_| format!("`{k}` has the wrong type"))
}

fn strings(t: &Table, k: &str) -> Result<BTreeMap<String, String>, String> {
    let mut m = BTreeMap::new();
    if let Some(t) = get::<Table>(t, k)? {
        for pair in t.pairs::<String, mlua::Value>() {
            let (k2, v) = pair.map_err(|_| format!("`{k}` has keys that aren't strings"))?;
            let v = match v {
                mlua::Value::String(s) => s.to_string_lossy(),
                mlua::Value::Integer(i) => i.to_string(),
                mlua::Value::Number(n) => n.to_string(),
                mlua::Value::Boolean(b) => b.to_string(),
                _ => return Err(format!("`{k}.{k2}` should be a string")),
            };
            m.insert(k2, v);
        }
    }
    Ok(m)
}

pub fn read_canvas(source: &str, name: &str) -> Result<Canvas, String> {
    with_data(source, name, |t| {
        let format = get::<i64>(t, "lineit")?.ok_or("not a lineit canvas (no `lineit = 1`)")?;
        if format > FORMAT {
            return Err(format!("made by a newer lineit (format {format}, this one reads {FORMAT})"));
        }
        let mut c = Canvas { doc: get::<String>(t, "doc")?.unwrap_or_default(), ..Canvas::default() };
        match get::<Table>(t, "camera")? {
            Some(cam) => {
                c.camera = Camera {
                    x: get(&cam, "x")?.unwrap_or(0.0),
                    y: get(&cam, "y")?.unwrap_or(0.0),
                    zoom: get(&cam, "zoom")?.unwrap_or(1.0),
                }
            }
            None => c.no_camera = true,
        }
        if let Some(squares) = get::<Table>(t, "squares")? {
            for (i, s) in squares.sequence_values::<Table>().enumerate() {
                let s = s.map_err(|_| format!("square {} isn't a table", i + 1))?;
                let id: u64 = get(&s, "id")?.ok_or(format!("square {} has no id", i + 1))?;
                let module: String = get(&s, "module")?.ok_or(format!("square {id} has no module"))?;
                if c.square(id).is_some() {
                    return Err(format!("two squares have id {id}"));
                }
                c.add(Square {
                    id,
                    module,
                    x: get(&s, "x")?.unwrap_or(0.0),
                    y: get(&s, "y")?.unwrap_or(0.0),
                    params: strings(&s, "params")?,
                    fields: strings(&s, "fields")?,
                });
            }
        }
        // ids that were used and are gone (older files have none: the highest is it)
        if let Some(next) = get::<u64>(t, "next_id")? {
            c.next = c.next.max(next);
        }
        if let Some(links) = get::<Table>(t, "links")? {
            for (i, l) in links.sequence_values::<Table>().enumerate() {
                let bad = || format!("link {} should be {{ from, \"output\", to, \"input\" }}", i + 1);
                let l = l.map_err(|_| bad())?;
                let link = Link {
                    from: l.get(1).map_err(|_| bad())?,
                    output: l.get(2).map_err(|_| bad())?,
                    to: l.get(3).map_err(|_| bad())?,
                    input: l.get(4).map_err(|_| bad())?,
                };
                // a wire to a square that isn't there is dropped, not an error
                if c.square(link.from).is_some() && c.square(link.to).is_some() && !c.links.contains(&link) {
                    c.links.push(link);
                }
            }
        }
        Ok(c)
    })
}

/// A Lua string literal for S.
pub fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 32 || c as u32 == 127 => {
                let _ = write!(out, "\\{}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A table key: bare if it can be, `["..."]` if not.
pub fn key(s: &str) -> String {
    const KEYWORDS: &[&str] = &[
        "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if", "in", "local", "nil",
        "not", "or", "repeat", "return", "then", "true", "until", "while",
    ];
    let ident = s.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !KEYWORDS.contains(&s);
    if ident { s.to_string() } else { format!("[{}]", quote(s)) }
}

fn number(f: f32) -> String {
    if f.is_finite() { format!("{f}") } else { "0".into() }
}

fn map(m: &BTreeMap<String, String>) -> String {
    let items: Vec<String> = m.iter().map(|(k, v)| format!("{} = {}", key(k), quote(v))).collect();
    format!("{{ {} }}", items.join(", "))
}

pub fn write_canvas(c: &Canvas) -> String {
    let mut s = String::from("-- lineit canvas\nreturn {\n");
    let _ = writeln!(s, "        lineit = {FORMAT},");
    if !c.doc.is_empty() {
        let _ = writeln!(s, "        doc = {},", quote(&c.doc));
    }
    // only when a square was deleted: files that need nothing more don't change
    let needed = c.squares.iter().map(|sq| sq.id + 1).max().unwrap_or(1);
    if c.next > needed {
        let _ = writeln!(s, "        next_id = {},", c.next);
    }
    let _ = writeln!(
        s,
        "        camera = {{ x = {}, y = {}, zoom = {} }},",
        number(c.camera.x),
        number(c.camera.y),
        number(c.camera.zoom)
    );
    s.push_str("        squares = {\n");
    for sq in &c.squares {
        let _ = write!(
            s,
            "                {{ id = {}, module = {}, x = {}, y = {}",
            sq.id,
            quote(&sq.module),
            number(sq.x),
            number(sq.y)
        );
        if !sq.params.is_empty() {
            let _ = write!(s, ", params = {}", map(&sq.params));
        }
        if !sq.fields.is_empty() {
            let _ = write!(s, ", fields = {}", map(&sq.fields));
        }
        s.push_str(" },\n");
    }
    s.push_str("        },\n        links = {\n");
    for l in &c.links {
        let _ = writeln!(s, "                {{ {}, {}, {}, {} }},", l.from, quote(&l.output), l.to, quote(&l.input));
    }
    s.push_str("        },\n}\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut c = Canvas { doc: "a \"doc\"\nwith lines".into(), ..Default::default() };
        c.camera.zoom = 0.5;
        c.add(Square::new(1, "flow/entry", -10.5, 3.0));
        c.add(Square::new(2, "io/print", 100.0, 0.0).with_field("value", "Hi\t\\ é\u{1}"));
        c.add(Square::new(3, "value/constant", 0.0, 50.0).with_param("type", "int").with_param("end", "x"));
        c.connect(Link::new(1, "then", 2, "tick"), false);
        let text = write_canvas(&c);
        assert!(!text.contains("next_id"), "nothing was deleted: {text}");
        assert_eq!(read_canvas(&text, "t").unwrap(), c);
    }

    #[test]
    fn deleted_ids_are_remembered() {
        let mut c = Canvas::default();
        c.add(Square::new(1, "flow/entry", 0.0, 0.0));
        c.add(Square::new(2, "io/print", 0.0, 0.0));
        c.remove(2);
        assert_eq!(c.next_id(), 3);
        let text = write_canvas(&c);
        assert!(text.contains("next_id = 3"), "{text}");
        let back = read_canvas(&text, "t").unwrap();
        assert_eq!(back, c);
        assert_eq!(back.next_id(), 3);
        // a file without it: the highest id plus one
        let old = read_canvas("return { lineit = 1, squares = { { id = 7, module = 'a' } } }", "t").unwrap();
        assert_eq!(old.next_id(), 8);
        // a next_id lower than the squares' is no use: they win
        let low =
            read_canvas("return { lineit = 1, next_id = 2, squares = { { id = 7, module = 'a' } } }", "t").unwrap();
        assert_eq!(low.next_id(), 8);
    }

    #[test]
    fn sandboxed() {
        assert!(read_canvas("os.exit(1)", "t").is_err());
        assert!(read_canvas("return io.open('x')", "t").is_err());
        assert!(read_canvas("while true do end", "t").unwrap_err().contains("too long"));
        assert!(read_canvas("local t = {} for i = 1, 1e9 do t[i] = i end return t", "t").is_err());
        assert!(read_canvas("return { lineit = 99 }", "t").unwrap_err().contains("newer"));
        // a camera at 0, 0 and zoom 1 is a camera too
        assert!(read_canvas("return { lineit = 1 }", "t").unwrap().no_camera);
        assert!(!read_canvas("return { lineit = 1, camera = { x = 0, y = 0, zoom = 1 } }", "t").unwrap().no_camera);
        let c = read_canvas(
            "return { lineit = 1, squares = { { id = 1, module = 'a' } }, links = { { 1, 'o', 2, 'i' } } }",
            "t",
        )
        .unwrap();
        assert!(c.links.is_empty());
    }
}

//! Shaders: a gdshader (Godot's shading language, `shader_type
//! canvas_item;`) run over an image, on the CPU.
//!
//! The text is read by the tree-sitter grammar for gdshader (after the
//! preprocessor in `shader/prep.rs`), turned into a tree where every name
//! is a slot (`shader/parse.rs`) and run for each pixel (`shader/eval.rs`).
//! What a shader sees is what Godot gives a canvas item's `fragment()`:
//! `UV`, `COLOR`, `TEXTURE`, `TEXTURE_PIXEL_SIZE`, `FRAGCOORD`, `TIME`...
//! its uniforms are inputs of the square (see [`uniforms`]).
//!
//! Not supported: other shader types (`spatial`, `particles`, `sky`), the
//! `vertex` and `light` functions (they are read, and not run), and
//! sampler types other than `sampler2D`.

mod ast;
mod builtins;
mod eval;
mod parse;
mod prep;
mod val;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::image::{self, Image};
use crate::value::Value;
use ast::{Program, Sc, Ty, UniformDecl};
use eval::{Interp, Tex};
use val::{R, V, Vn, convert, zero};

/// What a uniform is, for the pin that takes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Float,
    Int,
    Bool,
    /// `source_color`: `#rrggbb` (or `#rrggbbaa` if true) or a color name.
    Color(bool),
    /// Vectors and matrices: this many numbers, separated by commas.
    Numbers(usize),
    Image,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Uniform {
    pub name: String,
    pub kind: Kind,
    /// What the shader starts it as, as the pin shows it.
    pub default: String,
}

struct Compiled {
    program: Program,
    uniforms: Vec<Uniform>,
}

fn cache() -> &'static Mutex<HashMap<String, R<Arc<Compiled>>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, R<Arc<Compiled>>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Reads SRC and tries it on one pixel, once per text.
fn compile(src: &str) -> R<Arc<Compiled>> {
    let mut c = cache().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(r) = c.get(src) {
        return r.clone();
    }
    let r = build(src).map(Arc::new);
    if c.len() > 64 {
        c.clear();
    }
    c.insert(src.to_string(), r.clone());
    r
}

fn build(src: &str) -> R<Compiled> {
    let program = parse::parse(src)?;
    let mut uniforms = Vec::new();
    for u in &program.uniforms {
        let kind = kind_of(u)?;
        uniforms.push(Uniform { name: u.name.clone(), kind, default: default_of(&program, u, kind) });
    }
    let c = Compiled { program, uniforms };
    // a shader that can't run is told now, not for each pixel
    let zeros = c.program.uniforms.iter().map(|u| zero(&u.ty, &c.program.structs)).collect();
    let probe = Image::new(1, 1, [0, 0, 0, 0]);
    render(&c, &probe, 0.0, zeros, Vec::new())?;
    Ok(c)
}

fn kind_of(u: &UniformDecl) -> R<Kind> {
    Ok(match &u.ty {
        Ty::Vec(Sc::Float, 1) => Kind::Float,
        Ty::Vec(Sc::Int | Sc::UInt, 1) => Kind::Int,
        Ty::Vec(Sc::Bool, 1) => Kind::Bool,
        Ty::Vec(Sc::Float, n @ (3 | 4)) if u.color => Kind::Color(*n == 4),
        Ty::Vec(_, n) => Kind::Numbers(*n as usize),
        Ty::Mat(n) => Kind::Numbers((*n * *n) as usize),
        Ty::Sampler => Kind::Image,
        _ => return Err(format!("the uniform {} has a type that is not supported", u.name)),
    })
}

fn float_text(x: f64) -> String {
    let s = format!("{x}");
    if s.contains(['.', 'e', 'n', 'i']) { s } else { format!("{s}.0") }
}

/// The value a uniform starts as, as the text of its pin.
fn default_of(p: &Program, u: &UniformDecl, kind: Kind) -> String {
    let mut it = Interp::new(p, &[]);
    let zeros = p.uniforms.iter().map(|u| zero(&u.ty, &p.structs)).collect();
    let v = match (&u.init, it.setup(zeros)) {
        (Some(e), Ok(())) => it.eval(e, &mut Vec::new()).ok(),
        _ => None,
    };
    let v = v.unwrap_or_else(|| zero(&u.ty, &p.structs));
    match (kind, &v) {
        (Kind::Float, V::N(x)) => float_text(x.c[0]),
        (Kind::Int, V::N(x)) => format!("{}", x.c[0] as i64),
        (Kind::Bool, V::N(x)) => (x.c[0] != 0.0).to_string(),
        (Kind::Color(alpha), V::N(x)) => {
            let ch = |i: usize| (x.c[i].clamp(0.0, 1.0) * 255.0).round() as u8;
            let a = if alpha { ch(3) } else { 255 };
            let t = image::color_text([ch(0), ch(1), ch(2), a]);
            if alpha { t } else { t[..7].to_string() }
        }
        (Kind::Numbers(_), V::N(x)) => {
            (0..x.n as usize).map(|i| float_text(convert(x.k, Sc::Float, x.c[i]))).collect::<Vec<_>>().join(", ")
        }
        (Kind::Numbers(_), V::M(n, m)) => {
            m[..(*n * *n) as usize].iter().map(|x| float_text(*x as f64)).collect::<Vec<_>>().join(", ")
        }
        _ => String::new(),
    }
}

/// The uniforms of the shader SRC, in order; or what is wrong with it.
pub fn uniforms(src: &str) -> Result<Vec<Uniform>, String> {
    Ok(compile(src)?.uniforms.clone())
}

/// IMG through the shader SRC. TIME is what `TIME` reads; ARGS are the
/// values of the uniforms, in the order [`uniforms`] gives them. An empty
/// image if the shader can't be run.
pub fn apply(img: &Image, src: &str, time: f64, args: &[Value]) -> Image {
    run(img, src, time, args).unwrap_or_default()
}

/// Like [`apply`], with the shader in the file PATH (no path: the image
/// as it is; a file that can't be read: an empty image).
pub fn apply_file(img: &Image, path: &str, time: f64, args: &[Value]) -> Image {
    if path.trim().is_empty() {
        return img.clone();
    }
    match std::fs::read_to_string(path) {
        Ok(src) => apply(img, &src, time, args),
        Err(_) => Image::default(),
    }
}

/// Like [`apply`], with the reason it can't.
pub fn run(img: &Image, src: &str, time: f64, args: &[Value]) -> Result<Image, String> {
    let c = compile(src)?;
    let mut values = Vec::new();
    let mut extra = Vec::new();
    for (i, u) in c.program.uniforms.iter().enumerate() {
        let default = Value::Str(c.uniforms[i].default.as_str().into());
        let v = args.get(i).unwrap_or(&default);
        values.push(uniform_value(u, c.uniforms[i].kind, v, &mut extra));
    }
    render(&c, img, time, values, extra)
}

fn uniform_value(u: &UniformDecl, kind: Kind, v: &Value, textures: &mut Vec<Tex>) -> V {
    match (&u.ty, kind) {
        (Ty::Vec(k, 1), _) => {
            let x = match v {
                Value::Float(f) => *f,
                Value::Int(i) => *i as f64,
                Value::Bool(b) => *b as u8 as f64,
                Value::Str(s) => crate::value::parse_float(s).unwrap_or(0.0),
                _ => 0.0,
            };
            V::N(Vn::scalar(*k, convert(Sc::Float, *k, if *k == Sc::Float { x } else { x.trunc() })))
        }
        (Ty::Vec(k, n), _) => {
            let nums = numbers(v, *n as usize, matches!(kind, Kind::Color(_)) || *n >= 3);
            let mut c = [0.0; 4];
            for i in 0..*n as usize {
                c[i] = convert(Sc::Float, *k, nums[i]);
            }
            V::N(Vn { k: *k, n: *n, c })
        }
        (Ty::Mat(n), _) => {
            let nums = numbers(v, (*n * *n) as usize, false);
            let mut m = [0.0f32; 16];
            for (i, x) in nums.iter().enumerate().take(16) {
                m[i] = *x as f32;
            }
            V::M(*n, m)
        }
        _ => {
            // a sampler2D: the image wired in
            let img = match v {
                Value::Image(i) if i.width > 0 && i.height > 0 => i.clone(),
                _ if u.default_white => Image::new(1, 1, [255, 255, 255, 255]),
                _ => Image::new(1, 1, [0, 0, 0, 0]),
            };
            textures.push(Tex { img, linear: !u.filter_nearest, repeat: u.repeat });
            V::Tex(textures.len())
        }
    }
}

/// N numbers from a pin's text: a color (`#ff8800`, `red`) or numbers
/// (`0.5, 1`). One number alone goes to all of them; a missing alpha is 1.
fn numbers(v: &Value, n: usize, color_ok: bool) -> Vec<f64> {
    let text = match v {
        Value::Str(s) => s.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Int(i) => i.to_string(),
        _ => String::new(),
    };
    let t = text.trim();
    let mut out = vec![0.0; n.max(4)];
    let is_color = color_ok && (t.starts_with('#') || t.chars().next().is_some_and(|c| c.is_ascii_alphabetic()));
    if is_color && !t.contains([',', ' ']) {
        for (i, ch) in image::color(t).iter().enumerate() {
            out[i] = *ch as f64 / 255.0;
        }
    } else {
        let parsed: Vec<f64> = t
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .filter_map(crate::value::parse_float)
            .collect();
        match parsed.len() {
            0 => {}
            1 => out.iter_mut().for_each(|x| *x = parsed[0]),
            k => {
                for (i, x) in parsed.iter().enumerate().take(out.len()) {
                    out[i] = *x;
                }
                if n == 4 && k == 3 {
                    out[3] = 1.0;
                }
            }
        }
    }
    out.truncate(n.max(1));
    out.resize(n.max(4), 0.0);
    out
}

fn render(c: &Compiled, img: &Image, time: f64, values: Vec<V>, extra: Vec<Tex>) -> R<Image> {
    let (w, h) = (img.width, img.height);
    if w as u64 * h as u64 > image::MAX_PIXELS {
        return Ok(Image::default());
    }
    let mut tex = vec![Tex { img: img.clone(), linear: true, repeat: false }];
    tex.extend(extra);
    let mut main = Interp::new(&c.program, &tex);
    main.setup(values)?;
    let base = main.globals.clone();
    let mut px = vec![0u8; w as usize * h as usize * 4];
    if px.is_empty() {
        return Ok(Image { width: w, height: h, pixels: Arc::new(px) });
    }
    let threads = if (w as u64 * h as u64) < 4096 {
        1
    } else {
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).min(h as usize)
    };
    let rows = (h as usize).div_ceil(threads);
    let row_bytes = w as usize * 4;
    let time = time as f32;
    let results: Vec<R<()>> = std::thread::scope(|s| {
        let handles: Vec<_> = px
            .chunks_mut(rows * row_bytes)
            .enumerate()
            .map(|(t, chunk)| {
                let (program, tex, base) = (&c.program, &tex, &base);
                s.spawn(move || -> R<()> {
                    let mut it = Interp::new(program, tex);
                    for (r, row) in chunk.chunks_mut(row_bytes).enumerate() {
                        let y = (t * rows + r) as u32;
                        for (x, p) in row.chunks_exact_mut(4).enumerate() {
                            let col = it.pixel(base, x as u32, y, w, h, time)?;
                            for i in 0..4 {
                                p[i] = (col[i].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                            }
                        }
                    }
                    Ok(())
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap_or_else(|_| Err("the shader crashed".into()))).collect()
    });
    for r in results {
        r?;
    }
    Ok(Image { width: w, height: h, pixels: Arc::new(px) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(img: &Image, x: u32, y: u32) -> String {
        image::pixel(img, x as i64, y as i64)
    }

    #[test]
    fn passes_through_and_changes() {
        let img = Image::new(3, 2, image::color("#102030"));
        let same = run(&img, "shader_type canvas_item;", 0.0, &[]).unwrap();
        assert_eq!(px(&same, 1, 1), "#102030ff");
        let src = "shader_type canvas_item;\nvoid fragment() { COLOR = vec4(1.0 - COLOR.rgb, COLOR.a); }";
        assert_eq!(px(&run(&img, src, 0.0, &[]).unwrap(), 2, 0), "#efdfcfff");
    }

    #[test]
    fn uniforms_are_inputs() {
        let src = "shader_type canvas_item;\n\
            uniform float amount : hint_range(0.0, 1.0) = 0.5;\n\
            uniform vec4 tint : source_color = vec4(1.0, 0.0, 0.0, 1.0);\n\
            uniform int steps = 3;\n\
            uniform sampler2D other;\n\
            void fragment() { COLOR = mix(COLOR, tint, amount); }";
        let u = uniforms(src).unwrap();
        let shown: Vec<_> = u.iter().map(|u| (u.name.as_str(), u.kind, u.default.as_str())).collect();
        assert_eq!(
            shown,
            [
                ("amount", Kind::Float, "0.5"),
                ("tint", Kind::Color(true), "#ff0000ff"),
                ("steps", Kind::Int, "3"),
                ("other", Kind::Image, ""),
            ]
        );
        let img = Image::new(2, 2, [0, 0, 255, 255]);
        let args = [Value::Float(1.0), Value::Str("#00ff00".into())];
        assert_eq!(px(&run(&img, src, 0.0, &args).unwrap(), 0, 0), "#00ff00ff");
    }

    #[test]
    fn language() {
        // arrays, structs, out parameters, loops, switch, swizzle writes
        let src = "shader_type canvas_item;\n\
            struct P { float a; vec2 b; };\n\
            const int T[4] = int[](1, 2, 3, 4);\n\
            float twice(inout float x) { x *= 2.0; return x; }\n\
            void fragment() {\n\
                P p = P(1.0, vec2(0.5));\n\
                p.b.y = 0.25;\n\
                float s = 0.0;\n\
                for (int i = 0; i < 4; i++) { if (i == 2) continue; s += float(T[i]); }\n\
                switch (int(s)) { case 7: s += 1.0; break; default: s = 0.0; }\n\
                float k = 1.0; twice(k);\n\
                COLOR = vec4(s / 20.0, k / 4.0, p.b.y, 1.0);\n\
                COLOR.rgb += 0.0; COLOR.a = 1e0;\n\
            }";
        let img = Image::new(1, 1, [0, 0, 0, 255]);
        assert_eq!(px(&run(&img, src, 0.0, &[]).unwrap(), 0, 0), "#668040ff");
    }

    #[test]
    fn errors() {
        assert!(uniforms("shader_type spatial;").unwrap_err().contains("canvas_item"));
        assert!(uniforms("void fragment() { COLOR = nope; }").unwrap_err().contains("nope"));
        assert!(uniforms("void fragment() { COLOR = ; }").unwrap_err().contains("line 1"));
        let looping = "void fragment() { while (true) { } }";
        assert!(uniforms(looping).unwrap_err().contains("too long"));
    }
}

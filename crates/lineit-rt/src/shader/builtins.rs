//! The functions of the shading language (`mix`, `smoothstep`, `texture`...).
//! The maths goes through `libm`, so a shader gives the same pixels on
//! every machine.

use super::ast::{BFn, Sc};
use super::eval::Interp;
use super::val::*;

fn need(a: &[V], n: usize) -> R<()> {
    if a.len() == n { Ok(()) } else { Err(format!("this takes {n} values, not {}", a.len())) }
}

/// A vector or scalar of any kind, as floats.
fn fl(v: &V) -> R<Vn> {
    match v {
        V::N(x) => {
            let mut c = x.c;
            if x.k != Sc::Float {
                for e in c.iter_mut().take(x.n as usize) {
                    *e = convert(x.k, Sc::Float, *e);
                }
            }
            Ok(Vn { k: Sc::Float, n: x.n, c })
        }
        _ => Err("this needs a number or a vector".into()),
    }
}

fn out(n: u8, c: [f64; 4]) -> V {
    V::N(Vn { k: Sc::Float, n, c })
}

fn map1(a: &V, f: impl Fn(f32) -> f32) -> R<V> {
    let x = fl(a)?;
    let mut c = [0.0; 4];
    for i in 0..x.n as usize {
        c[i] = f(x.c[i] as f32) as f64;
    }
    Ok(out(x.n, c))
}

/// F over the components of A and B (a scalar goes with every component).
fn map2(a: &V, b: &V, f: impl Fn(f32, f32) -> f32) -> R<V> {
    let (x, y) = (fl(a)?, fl(b)?);
    let n = size2(&x, &y)?;
    let mut c = [0.0; 4];
    for i in 0..n as usize {
        c[i] = f(pick(&x, i), pick(&y, i)) as f64;
    }
    Ok(out(n, c))
}

fn map3(a: &V, b: &V, d: &V, f: impl Fn(f32, f32, f32) -> f32) -> R<V> {
    let (x, y, z) = (fl(a)?, fl(b)?, fl(d)?);
    let n = size2(&x, &y)?.max(z.n);
    for v in [&x, &y, &z] {
        if v.n != 1 && v.n != n {
            return Err("vectors of different sizes".into());
        }
    }
    let mut c = [0.0; 4];
    for i in 0..n as usize {
        c[i] = f(pick(&x, i), pick(&y, i), pick(&z, i)) as f64;
    }
    Ok(out(n, c))
}

fn pick(v: &Vn, i: usize) -> f32 {
    v.c[if v.n == 1 { 0 } else { i }] as f32
}

fn size2(x: &Vn, y: &Vn) -> R<u8> {
    if x.n == y.n || y.n == 1 {
        Ok(x.n)
    } else if x.n == 1 {
        Ok(y.n)
    } else {
        Err(format!("a vector of {} and one of {}", x.n, y.n))
    }
}

fn is_int(v: &V) -> bool {
    matches!(v, V::N(x) if matches!(x.k, Sc::Int | Sc::UInt))
}

/// Like [`map2`], but integers stay integers.
fn common2(a: &V, b: &V, ff: impl Fn(f32, f32) -> f32, fi: impl Fn(i64, i64) -> i64) -> R<V> {
    if let (true, true, V::N(x), V::N(y)) = (is_int(a), is_int(b), a, b) {
        let n = if x.n == y.n || y.n == 1 {
            x.n
        } else if x.n == 1 {
            y.n
        } else {
            return Err("vectors of different sizes".into());
        };
        let mut c = [0.0; 4];
        for i in 0..n as usize {
            let (p, q) = (x.c[if x.n == 1 { 0 } else { i }], y.c[if y.n == 1 { 0 } else { i }]);
            c[i] = fi(p as i64, q as i64) as f64;
        }
        return Ok(V::N(Vn { k: x.k, n, c }));
    }
    map2(a, b, ff)
}

fn vec_of(v: &V) -> R<Vn> {
    fl(v)
}

fn dot_of(x: &Vn, y: &Vn) -> R<f32> {
    if x.n != y.n {
        return Err("vectors of different sizes".into());
    }
    let mut s = 0.0f32;
    for i in 0..x.n as usize {
        s += x.c[i] as f32 * y.c[i] as f32;
    }
    Ok(s)
}

fn tex_of(v: &V) -> R<usize> {
    match v {
        V::Tex(t) => Ok(*t),
        _ => Err("this needs a sampler2D".into()),
    }
}

fn uv_of(v: &V) -> R<(f32, f32)> {
    let x = fl(v)?;
    if x.n < 2 {
        return Err("a texture is sampled with a vec2".into());
    }
    Ok((x.c[0] as f32, x.c[1] as f32))
}

macro_rules! math1 {
    ($name:ident, $f:expr) => {
        fn $name(_: &mut Interp, a: &[V]) -> R<V> {
            need(a, 1)?;
            map1(&a[0], $f)
        }
    };
}

macro_rules! math2 {
    ($name:ident, $f:expr) => {
        fn $name(_: &mut Interp, a: &[V]) -> R<V> {
            need(a, 2)?;
            map2(&a[0], &a[1], $f)
        }
    };
}

math1!(b_radians, |x| x * (std::f32::consts::PI / 180.0));
math1!(b_degrees, |x| x * (180.0 / std::f32::consts::PI));
math1!(b_sin, libm::sinf);
math1!(b_cos, libm::cosf);
math1!(b_tan, libm::tanf);
math1!(b_asin, libm::asinf);
math1!(b_acos, libm::acosf);
math1!(b_sinh, libm::sinhf);
math1!(b_cosh, libm::coshf);
math1!(b_tanh, libm::tanhf);
math1!(b_asinh, libm::asinhf);
math1!(b_acosh, libm::acoshf);
math1!(b_atanh, libm::atanhf);
math1!(b_exp, libm::expf);
math1!(b_exp2, libm::exp2f);
math1!(b_log, libm::logf);
math1!(b_log2, libm::log2f);
math1!(b_sqrt, libm::sqrtf);
math1!(b_inversesqrt, |x| 1.0 / libm::sqrtf(x));
math1!(b_floor, f32::floor);
math1!(b_ceil, f32::ceil);
math1!(b_trunc, f32::trunc);
math1!(b_round, f32::round);
math1!(b_round_even, libm::rintf);
math1!(b_fract, |x| x - x.floor());
math2!(b_pow, libm::powf);
math2!(b_step, |e, x| if x < e { 0.0 } else { 1.0 });
math2!(b_mod, |x, y| x - y * (x / y).floor());

fn b_atan(_: &mut Interp, a: &[V]) -> R<V> {
    match a.len() {
        1 => map1(&a[0], libm::atanf),
        2 => map2(&a[0], &a[1], libm::atan2f),
        _ => Err("atan takes one or two values".into()),
    }
}

fn b_abs(_: &mut Interp, a: &[V]) -> R<V> {
    need(a, 1)?;
    if let V::N(x) = &a[0] {
        if x.k == Sc::Int {
            let mut c = x.c;
            c.iter_mut().for_each(|e| *e = (*e as i32).wrapping_abs() as f64);
            return Ok(V::N(Vn { c, ..*x }));
        }
        if x.k == Sc::UInt {
            return Ok(a[0].clone());
        }
    }
    map1(&a[0], f32::abs)
}

fn b_sign(_: &mut Interp, a: &[V]) -> R<V> {
    need(a, 1)?;
    let sign = |x: f64| {
        if x > 0.0 {
            1.0
        } else if x < 0.0 {
            -1.0
        } else {
            0.0
        }
    };
    if let V::N(x) = &a[0] {
        if matches!(x.k, Sc::Int | Sc::UInt) {
            let mut c = x.c;
            c.iter_mut().for_each(|e| *e = sign(*e));
            return Ok(V::N(Vn { c, ..*x }));
        }
    }
    map1(&a[0], |x| sign(x as f64) as f32)
}

fn b_min(_: &mut Interp, a: &[V]) -> R<V> {
    need(a, 2)?;
    common2(&a[0], &a[1], |x, y| if y < x { y } else { x }, |x, y| x.min(y))
}

fn b_max(_: &mut Interp, a: &[V]) -> R<V> {
    need(a, 2)?;
    common2(&a[0], &a[1], |x, y| if y > x { y } else { x }, |x, y| x.max(y))
}

fn b_clamp(it: &mut Interp, a: &[V]) -> R<V> {
    need(a, 3)?;
    let lo = b_max(it, &[a[0].clone(), a[1].clone()])?;
    b_min(it, &[lo, a[2].clone()])
}

fn b_mix(_: &mut Interp, a: &[V]) -> R<V> {
    need(a, 3)?;
    map3(&a[0], &a[1], &a[2], |x, y, t| x * (1.0 - t) + y * t)
}

fn b_smoothstep(_: &mut Interp, a: &[V]) -> R<V> {
    need(a, 3)?;
    map3(&a[0], &a[1], &a[2], |e0, e1, x| {
        let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    })
}

fn b_fma(_: &mut Interp, a: &[V]) -> R<V> {
    need(a, 3)?;
    map3(&a[0], &a[1], &a[2], |x, y, z| x * y + z)
}

fn b_isnan(_: &mut Interp, a: &[V]) -> R<V> {
    need(a, 1)?;
    relational1(&a[0], |x| x.is_nan())
}

fn b_isinf(_: &mut Interp, a: &[V]) -> R<V> {
    need(a, 1)?;
    relational1(&a[0], |x| x.is_infinite())
}

fn relational1(a: &V, f: impl Fn(f32) -> bool) -> R<V> {
    let x = fl(a)?;
    let mut c = [0.0; 4];
    for i in 0..x.n as usize {
        c[i] = f(x.c[i] as f32) as u8 as f64;
    }
    Ok(V::N(Vn { k: Sc::Bool, n: x.n, c }))
}

fn compare(a: &[V], f: impl Fn(f64, f64) -> bool) -> R<V> {
    need(a, 2)?;
    let (x, y) = (fl(&a[0])?, fl(&a[1])?);
    if x.n != y.n {
        return Err("vectors of different sizes".into());
    }
    let mut c = [0.0; 4];
    for i in 0..x.n as usize {
        c[i] = f(x.c[i], y.c[i]) as u8 as f64;
    }
    Ok(V::N(Vn { k: Sc::Bool, n: x.n, c }))
}

fn b_less(_: &mut Interp, a: &[V]) -> R<V> {
    compare(a, |x, y| x < y)
}
fn b_less_eq(_: &mut Interp, a: &[V]) -> R<V> {
    compare(a, |x, y| x <= y)
}
fn b_greater(_: &mut Interp, a: &[V]) -> R<V> {
    compare(a, |x, y| x > y)
}
fn b_greater_eq(_: &mut Interp, a: &[V]) -> R<V> {
    compare(a, |x, y| x >= y)
}
fn b_equal(_: &mut Interp, a: &[V]) -> R<V> {
    compare(a, |x, y| x == y)
}
fn b_not_equal(_: &mut Interp, a: &[V]) -> R<V> {
    compare(a, |x, y| x != y)
}

fn b_any(_: &mut Interp, a: &[V]) -> R<V> {
    need(a, 1)?;
    let x = fl(&a[0])?;
    Ok(V::boolean((0..x.n as usize).any(|i| x.c[i] != 0.0)))
}

fn b_all(_: &mut Interp, a: &[V]) -> R<V> {
    need(a, 1)?;
    let x = fl(&a[0])?;
    Ok(V::boolean((0..x.n as usize).all(|i| x.c[i] != 0.0)))
}

fn b_not(_: &mut Interp, a: &[V]) -> R<V> {
    need(a, 1)?;
    let x = fl(&a[0])?;
    let mut c = [0.0; 4];
    for i in 0..x.n as usize {
        c[i] = (x.c[i] == 0.0) as u8 as f64;
    }
    Ok(V::N(Vn { k: Sc::Bool, n: x.n, c }))
}

fn b_length(_: &mut Interp, a: &[V]) -> R<V> {
    need(a, 1)?;
    let x = vec_of(&a[0])?;
    Ok(V::float(libm::sqrtf(dot_of(&x, &x)?)))
}

fn b_distance(_: &mut Interp, a: &[V]) -> R<V> {
    need(a, 2)?;
    let d = map2(&a[0], &a[1], |x, y| x - y)?;
    let x = vec_of(&d)?;
    Ok(V::float(libm::sqrtf(dot_of(&x, &x)?)))
}

fn b_dot(_: &mut Interp, a: &[V]) -> R<V> {
    need(a, 2)?;
    Ok(V::float(dot_of(&vec_of(&a[0])?, &vec_of(&a[1])?)?))
}

fn b_cross(_: &mut Interp, a: &[V]) -> R<V> {
    need(a, 2)?;
    let (x, y) = (vec_of(&a[0])?, vec_of(&a[1])?);
    if x.n != 3 || y.n != 3 {
        return Err("cross takes two vec3".into());
    }
    let (p, q) = (|i: usize| x.c[i] as f32, |i: usize| y.c[i] as f32);
    Ok(V::floats(&[p(1) * q(2) - p(2) * q(1), p(2) * q(0) - p(0) * q(2), p(0) * q(1) - p(1) * q(0)]))
}

fn b_normalize(_: &mut Interp, a: &[V]) -> R<V> {
    need(a, 1)?;
    let x = vec_of(&a[0])?;
    let len = libm::sqrtf(dot_of(&x, &x)?);
    map1(&a[0], |v| v / len)
}

fn b_reflect(_: &mut Interp, a: &[V]) -> R<V> {
    need(a, 2)?;
    let (i, n) = (vec_of(&a[0])?, vec_of(&a[1])?);
    let d = dot_of(&n, &i)?;
    map2(&a[0], &map1(&a[1], |v| v * 2.0 * d)?, |i, n| i - n)
}

fn b_refract(_: &mut Interp, a: &[V]) -> R<V> {
    need(a, 3)?;
    let (i, n) = (vec_of(&a[0])?, vec_of(&a[1])?);
    let eta = a[2].as_f32("eta")?;
    let d = dot_of(&n, &i)?;
    let k = 1.0 - eta * eta * (1.0 - d * d);
    if k < 0.0 {
        return Ok(out(i.n, [0.0; 4]));
    }
    let scale = eta * d + libm::sqrtf(k);
    map2(&map1(&a[0], |v| v * eta)?, &map1(&a[1], |v| v * scale)?, |p, q| p - q)
}

fn b_faceforward(_: &mut Interp, a: &[V]) -> R<V> {
    need(a, 3)?;
    let d = dot_of(&vec_of(&a[2])?, &vec_of(&a[1])?)?;
    if d < 0.0 { Ok(a[0].clone()) } else { map1(&a[0], |v| -v) }
}

fn b_comp_mult(_: &mut Interp, a: &[V]) -> R<V> {
    need(a, 2)?;
    match (&a[0], &a[1]) {
        (V::M(n, x), V::M(m, y)) if n == m => {
            let mut o = [0.0; 16];
            for i in 0..(*n * *n) as usize {
                o[i] = x[i] * y[i];
            }
            Ok(V::M(*n, o))
        }
        _ => Err("matrixCompMult takes two matrices of a size".into()),
    }
}

fn b_transpose(_: &mut Interp, a: &[V]) -> R<V> {
    need(a, 1)?;
    match &a[0] {
        V::M(n, x) => {
            let n = *n as usize;
            let mut o = [0.0; 16];
            for c in 0..n {
                for r in 0..n {
                    o[r * n + c] = x[c * n + r];
                }
            }
            Ok(V::M(n as u8, o))
        }
        _ => Err("transpose takes a matrix".into()),
    }
}

fn b_outer(_: &mut Interp, a: &[V]) -> R<V> {
    need(a, 2)?;
    let (c, r) = (vec_of(&a[0])?, vec_of(&a[1])?);
    if c.n != r.n {
        return Err("outerProduct takes two vectors of a size".into());
    }
    let n = c.n as usize;
    let mut o = [0.0; 16];
    for j in 0..n {
        for i in 0..n {
            o[j * n + i] = c.c[i] as f32 * r.c[j] as f32;
        }
    }
    Ok(V::M(n as u8, o))
}

fn b_texture(it: &mut Interp, a: &[V]) -> R<V> {
    if a.len() < 2 || a.len() > 3 {
        return Err("texture takes a sampler, a position and maybe a bias".into());
    }
    let (u, v) = uv_of(&a[1])?;
    Ok(V::floats(&it.sample(tex_of(&a[0])?, u, v)))
}

fn b_texture_lod(it: &mut Interp, a: &[V]) -> R<V> {
    need(a, 3)?;
    let (u, v) = uv_of(&a[1])?;
    Ok(V::floats(&it.sample(tex_of(&a[0])?, u, v)))
}

fn b_texture_grad(it: &mut Interp, a: &[V]) -> R<V> {
    need(a, 4)?;
    let (u, v) = uv_of(&a[1])?;
    Ok(V::floats(&it.sample(tex_of(&a[0])?, u, v)))
}

fn b_texel_fetch(it: &mut Interp, a: &[V]) -> R<V> {
    need(a, 3)?;
    let p = fl(&a[1])?;
    if p.n < 2 {
        return Err("texelFetch takes an ivec2".into());
    }
    Ok(V::floats(&it.texel(tex_of(&a[0])?, p.c[0] as i64, p.c[1] as i64)))
}

fn b_texture_size(it: &mut Interp, a: &[V]) -> R<V> {
    if a.is_empty() || a.len() > 2 {
        return Err("textureSize takes a sampler and a level".into());
    }
    let (w, h) = it.tex.get(tex_of(&a[0])?).map(|t| (t.img.width, t.img.height)).unwrap_or((0, 0));
    Ok(V::N(Vn { k: Sc::Int, n: 2, c: [w as f64, h as f64, 0.0, 0.0] }))
}

/// Screen derivatives mean nothing for one pixel at a time: no change.
fn b_deriv(_: &mut Interp, a: &[V]) -> R<V> {
    need(a, 1)?;
    map1(&a[0], |_| 0.0)
}

fn b_float_bits_to_int(_: &mut Interp, a: &[V]) -> R<V> {
    need(a, 1)?;
    bits(&a[0], Sc::Int, |x| (x as f32).to_bits() as i32 as f64)
}

fn b_float_bits_to_uint(_: &mut Interp, a: &[V]) -> R<V> {
    need(a, 1)?;
    bits(&a[0], Sc::UInt, |x| (x as f32).to_bits() as f64)
}

fn b_int_bits_to_float(_: &mut Interp, a: &[V]) -> R<V> {
    need(a, 1)?;
    bits(&a[0], Sc::Float, |x| f32::from_bits(x as i32 as u32) as f64)
}

fn b_uint_bits_to_float(_: &mut Interp, a: &[V]) -> R<V> {
    need(a, 1)?;
    bits(&a[0], Sc::Float, |x| f32::from_bits(x as u32) as f64)
}

fn bits(a: &V, k: Sc, f: impl Fn(f64) -> f64) -> R<V> {
    match a {
        V::N(x) => {
            let mut c = [0.0; 4];
            for i in 0..x.n as usize {
                c[i] = f(x.c[i]);
            }
            Ok(V::N(Vn { k, n: x.n, c }))
        }
        _ => Err("this needs a number or a vector".into()),
    }
}

/// `array.length()`.
pub fn array_length(_: &mut Interp, a: &[V]) -> R<V> {
    match a.first() {
        Some(V::Arr(x)) => Ok(V::int(x.len() as i32)),
        _ => Err("length() needs an array".into()),
    }
}

/// The function called NAME, if the language has it.
pub fn lookup(name: &str) -> Option<BFn> {
    let f: BFn = match name {
        "radians" => b_radians,
        "degrees" => b_degrees,
        "sin" => b_sin,
        "cos" => b_cos,
        "tan" => b_tan,
        "asin" => b_asin,
        "acos" => b_acos,
        "atan" => b_atan,
        "sinh" => b_sinh,
        "cosh" => b_cosh,
        "tanh" => b_tanh,
        "asinh" => b_asinh,
        "acosh" => b_acosh,
        "atanh" => b_atanh,
        "pow" => b_pow,
        "exp" => b_exp,
        "exp2" => b_exp2,
        "log" => b_log,
        "log2" => b_log2,
        "sqrt" => b_sqrt,
        "inversesqrt" => b_inversesqrt,
        "abs" => b_abs,
        "sign" => b_sign,
        "floor" => b_floor,
        "ceil" => b_ceil,
        "trunc" => b_trunc,
        "round" => b_round,
        "roundEven" => b_round_even,
        "fract" => b_fract,
        "mod" => b_mod,
        "min" => b_min,
        "max" => b_max,
        "clamp" => b_clamp,
        "mix" => b_mix,
        "step" => b_step,
        "smoothstep" => b_smoothstep,
        "fma" => b_fma,
        "isnan" => b_isnan,
        "isinf" => b_isinf,
        "lessThan" => b_less,
        "lessThanEqual" => b_less_eq,
        "greaterThan" => b_greater,
        "greaterThanEqual" => b_greater_eq,
        "equal" => b_equal,
        "notEqual" => b_not_equal,
        "any" => b_any,
        "all" => b_all,
        "not" => b_not,
        "length" => b_length,
        "distance" => b_distance,
        "dot" => b_dot,
        "cross" => b_cross,
        "normalize" => b_normalize,
        "reflect" => b_reflect,
        "refract" => b_refract,
        "faceforward" => b_faceforward,
        "matrixCompMult" => b_comp_mult,
        "transpose" => b_transpose,
        "outerProduct" => b_outer,
        "texture" => b_texture,
        "textureLod" => b_texture_lod,
        "textureGrad" => b_texture_grad,
        "texelFetch" => b_texel_fetch,
        "textureSize" => b_texture_size,
        "dFdx" | "dFdy" | "fwidth" => b_deriv,
        "floatBitsToInt" => b_float_bits_to_int,
        "floatBitsToUint" => b_float_bits_to_uint,
        "intBitsToFloat" => b_int_bits_to_float,
        "uintBitsToFloat" => b_uint_bits_to_float,
        _ => return None,
    };
    Some(f)
}

//! The values of a shader and the arithmetic on them.
//!
//! A number, a vector or a scalar, keeps its components as `f64`, which
//! holds every `f32`, `i32` and `u32` exactly: floats are rounded to `f32`
//! after each operation and integers wrap at 32 bits, as they do on a GPU.

use std::sync::Arc;

use super::ast::{Bin, Sc, StructDef, Ty, Un};

pub type R<T> = Result<T, String>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vn {
    pub k: Sc,
    /// 1 for a scalar.
    pub n: u8,
    pub c: [f64; 4],
}

#[derive(Clone, Debug)]
pub enum V {
    Void,
    N(Vn),
    /// A square matrix, column by column.
    M(u8, [f32; 16]),
    /// A texture, by its place in the list the run was given.
    Tex(usize),
    Arr(Arc<Vec<V>>),
    Obj(usize, Arc<Vec<V>>),
}

impl Vn {
    pub fn scalar(k: Sc, x: f64) -> Vn {
        Vn { k, n: 1, c: [x, 0.0, 0.0, 0.0] }
    }
}

impl V {
    pub fn float(x: f32) -> V {
        V::N(Vn::scalar(Sc::Float, x as f64))
    }
    pub fn int(x: i32) -> V {
        V::N(Vn::scalar(Sc::Int, x as f64))
    }
    pub fn uint(x: u32) -> V {
        V::N(Vn::scalar(Sc::UInt, x as f64))
    }
    pub fn boolean(x: bool) -> V {
        V::N(Vn::scalar(Sc::Bool, x as u8 as f64))
    }
    /// A float vector (or scalar) of up to 4 components.
    pub fn floats(v: &[f32]) -> V {
        let mut c = [0.0; 4];
        for (i, x) in v.iter().take(4).enumerate() {
            c[i] = *x as f64;
        }
        V::N(Vn { k: Sc::Float, n: v.len().clamp(1, 4) as u8, c })
    }

    pub fn truthy(&self) -> R<bool> {
        match self {
            V::N(x) if x.n == 1 => Ok(x.c[0] != 0.0),
            _ => Err("a condition has to be a single true or false".into()),
        }
    }

    /// The value as a float, or an error saying what WHAT was.
    pub fn as_f32(&self, what: &str) -> R<f32> {
        match self {
            V::N(x) if x.n == 1 => Ok(x.c[0] as f32),
            _ => Err(format!("{what} should be a single number")),
        }
    }

    pub fn as_i32(&self, what: &str) -> R<i32> {
        match self {
            V::N(x) if x.n == 1 => Ok(convert(x.k, Sc::Int, x.c[0]) as i32),
            _ => Err(format!("{what} should be a single number")),
        }
    }
}

/// A number of kind FROM, as one of kind TO.
pub fn convert(from: Sc, to: Sc, x: f64) -> f64 {
    if from == to {
        return x;
    }
    match to {
        Sc::Bool => (x != 0.0) as u8 as f64,
        Sc::Float => x as f32 as f64,
        Sc::Int => match from {
            Sc::Float => (x as f32) as i32 as f64,
            Sc::UInt => (x as u32) as i32 as f64,
            _ => x,
        },
        Sc::UInt => match from {
            Sc::Float => (x as f32) as u32 as f64,
            Sc::Int => (x as i32) as u32 as f64,
            _ => x,
        },
    }
}

/// What a variable of type TY holds before anything is put in it.
pub fn zero(ty: &Ty, structs: &[StructDef]) -> V {
    match ty {
        Ty::Void => V::Void,
        Ty::Vec(k, n) => V::N(Vn { k: *k, n: *n, c: [0.0; 4] }),
        Ty::Mat(n) => V::M(*n, [0.0; 16]),
        Ty::Sampler => V::Tex(usize::MAX),
        Ty::Array(t, n) => V::Arr(Arc::new((0..*n).map(|_| zero(t, structs)).collect())),
        Ty::Struct(i) => V::Obj(*i, Arc::new(structs[*i].fields.iter().map(|(_, t)| zero(t, structs)).collect())),
    }
}

/// V as a value of type TY, when it only differs in the kind of number
/// (`float x = 1;` is an error in Godot; here it works).
pub fn coerce(ty: &Ty, v: V) -> V {
    match (ty, &v) {
        (Ty::Vec(k, n), V::N(x)) if x.k != *k && x.n == *n => {
            let mut c = [0.0; 4];
            for i in 0..x.n as usize {
                c[i] = convert(x.k, *k, x.c[i]);
            }
            V::N(Vn { k: *k, n: *n, c })
        }
        (Ty::Array(t, _), V::Arr(a)) => {
            let off = match (&**t, a.first()) {
                (Ty::Vec(k, _), Some(V::N(x))) => x.k != *k,
                _ => false,
            };
            if off { V::Arr(Arc::new(a.iter().map(|e| coerce(t, e.clone())).collect())) } else { v }
        }
        _ => v,
    }
}

fn arith(op: Bin, k: Sc, a: f64, b: f64) -> R<f64> {
    let bad = || format!("{op:?} doesn't work on these numbers");
    Ok(match k {
        Sc::Float => {
            let (a, b) = (a as f32, b as f32);
            (match op {
                Bin::Add => a + b,
                Bin::Sub => a - b,
                Bin::Mul => a * b,
                Bin::Div => a / b,
                Bin::Rem => libm::fmodf(a, b),
                _ => return Err(bad()),
            }) as f64
        }
        Sc::Int => {
            let (a, b) = (a as i32, b as i32);
            (match op {
                Bin::Add => a.wrapping_add(b),
                Bin::Sub => a.wrapping_sub(b),
                Bin::Mul => a.wrapping_mul(b),
                Bin::Div => {
                    if b == 0 {
                        0
                    } else {
                        a.wrapping_div(b)
                    }
                }
                Bin::Rem => {
                    if b == 0 {
                        0
                    } else {
                        a.wrapping_rem(b)
                    }
                }
                Bin::Shl => a.wrapping_shl(b as u32),
                Bin::Shr => a.wrapping_shr(b as u32),
                Bin::BitAnd => a & b,
                Bin::BitOr => a | b,
                Bin::BitXor => a ^ b,
                _ => return Err(bad()),
            }) as f64
        }
        Sc::UInt => {
            let (a, b) = (a as u32, b as u32);
            (match op {
                Bin::Add => a.wrapping_add(b),
                Bin::Sub => a.wrapping_sub(b),
                Bin::Mul => a.wrapping_mul(b),
                Bin::Div => a.checked_div(b).unwrap_or(0),
                Bin::Rem => a.checked_rem(b).unwrap_or(0),
                Bin::Shl => a.wrapping_shl(b),
                Bin::Shr => a.wrapping_shr(b),
                Bin::BitAnd => a & b,
                Bin::BitOr => a | b,
                Bin::BitXor => a ^ b,
                _ => return Err(bad()),
            }) as f64
        }
        Sc::Bool => match op {
            Bin::Xor | Bin::BitXor | Bin::Ne => ((a != 0.0) != (b != 0.0)) as u8 as f64,
            Bin::And | Bin::BitAnd => ((a != 0.0) && (b != 0.0)) as u8 as f64,
            Bin::Or | Bin::BitOr => ((a != 0.0) || (b != 0.0)) as u8 as f64,
            _ => return Err("can't do arithmetic on true or false".into()),
        },
    })
}

pub fn binary(op: Bin, a: &V, b: &V) -> R<V> {
    match (a, b) {
        (V::N(x), V::N(y)) => num_binary(op, x, y),
        (V::M(n, x), V::M(m, y)) if n == m => mat_mat(op, *n as usize, x, y),
        (V::M(n, x), V::N(y)) => mat_vec(op, *n as usize, x, y),
        (V::N(x), V::M(n, y)) => vec_mat(op, x, *n as usize, y),
        _ => Err(format!("{op:?} doesn't work on these")),
    }
}

fn num_binary(op: Bin, x: &Vn, y: &Vn) -> R<V> {
    let k = if x.k == y.k {
        x.k
    } else if x.k == Sc::Float || y.k == Sc::Float {
        Sc::Float
    } else if x.k == Sc::Bool || y.k == Sc::Bool {
        return Err("true or false mixed with numbers".into());
    } else {
        x.k
    };
    let get = |v: &Vn, i: usize| convert(v.k, k, v.c[if v.n == 1 { 0 } else { i }]);
    if matches!(op, Bin::Lt | Bin::Le | Bin::Gt | Bin::Ge) {
        if x.n != 1 || y.n != 1 {
            return Err("only single numbers can be compared with < and >".into());
        }
        let (a, b) = (get(x, 0), get(y, 0));
        let r = match op {
            Bin::Lt => a < b,
            Bin::Le => a <= b,
            Bin::Gt => a > b,
            _ => a >= b,
        };
        return Ok(V::boolean(r));
    }
    let n = if x.n == y.n {
        x.n
    } else if x.n == 1 {
        y.n
    } else if y.n == 1 {
        x.n
    } else {
        return Err(format!("a vector of {} and one of {}", x.n, y.n));
    };
    let mut c = [0.0; 4];
    for i in 0..n as usize {
        c[i] = arith(op, k, get(x, i), get(y, i))?;
    }
    Ok(V::N(Vn { k, n, c }))
}

fn mat_mat(op: Bin, n: usize, x: &[f32; 16], y: &[f32; 16]) -> R<V> {
    let mut m = [0.0f32; 16];
    match op {
        Bin::Mul => {
            for c in 0..n {
                for r in 0..n {
                    let mut s = 0.0f32;
                    for k in 0..n {
                        s += x[k * n + r] * y[c * n + k];
                    }
                    m[c * n + r] = s;
                }
            }
        }
        Bin::Add | Bin::Sub | Bin::Div => {
            for i in 0..n * n {
                m[i] = match op {
                    Bin::Add => x[i] + y[i],
                    Bin::Sub => x[i] - y[i],
                    _ => x[i] / y[i],
                };
            }
        }
        _ => return Err(format!("{op:?} doesn't work on matrices")),
    }
    Ok(V::M(n as u8, m))
}

fn mat_vec(op: Bin, n: usize, x: &[f32; 16], y: &Vn) -> R<V> {
    if y.n == 1 {
        let s = y.c[0] as f32;
        let mut m = [0.0f32; 16];
        for i in 0..n * n {
            m[i] = match op {
                Bin::Add => x[i] + s,
                Bin::Sub => x[i] - s,
                Bin::Mul => x[i] * s,
                Bin::Div => x[i] / s,
                _ => return Err(format!("{op:?} doesn't work on matrices")),
            };
        }
        return Ok(V::M(n as u8, m));
    }
    if op != Bin::Mul || y.n as usize != n {
        return Err("a matrix and a vector of another size".into());
    }
    let mut c = [0.0; 4];
    for r in 0..n {
        let mut s = 0.0f32;
        for k in 0..n {
            s += x[k * n + r] * y.c[k] as f32;
        }
        c[r] = s as f64;
    }
    Ok(V::N(Vn { k: Sc::Float, n: n as u8, c }))
}

fn vec_mat(op: Bin, x: &Vn, n: usize, y: &[f32; 16]) -> R<V> {
    if x.n == 1 {
        let s = x.c[0] as f32;
        let mut m = [0.0f32; 16];
        for i in 0..n * n {
            m[i] = match op {
                Bin::Add => s + y[i],
                Bin::Sub => s - y[i],
                Bin::Mul => s * y[i],
                Bin::Div => s / y[i],
                _ => return Err(format!("{op:?} doesn't work on matrices")),
            };
        }
        return Ok(V::M(n as u8, m));
    }
    if op != Bin::Mul || x.n as usize != n {
        return Err("a vector and a matrix of another size".into());
    }
    let mut c = [0.0; 4];
    for col in 0..n {
        let mut s = 0.0f32;
        for k in 0..n {
            s += x.c[k] as f32 * y[col * n + k];
        }
        c[col] = s as f64;
    }
    Ok(V::N(Vn { k: Sc::Float, n: n as u8, c }))
}

pub fn unary(op: Un, v: &V) -> R<V> {
    match (op, v) {
        (Un::Plus, _) => Ok(v.clone()),
        (Un::Neg, V::N(x)) => {
            let mut c = [0.0; 4];
            for i in 0..x.n as usize {
                c[i] = match x.k {
                    Sc::Float => -(x.c[i] as f32) as f64,
                    Sc::Int => (x.c[i] as i32).wrapping_neg() as f64,
                    Sc::UInt => (x.c[i] as u32).wrapping_neg() as f64,
                    Sc::Bool => return Err("can't negate true or false".into()),
                };
            }
            Ok(V::N(Vn { c, ..*x }))
        }
        (Un::Neg, V::M(n, m)) => {
            let mut o = *m;
            o.iter_mut().for_each(|x| *x = -*x);
            Ok(V::M(*n, o))
        }
        (Un::Not, V::N(x)) if x.k == Sc::Bool && x.n == 1 => Ok(V::boolean(x.c[0] == 0.0)),
        (Un::BitNot, V::N(x)) if matches!(x.k, Sc::Int | Sc::UInt) => {
            let mut c = [0.0; 4];
            for i in 0..x.n as usize {
                c[i] = if x.k == Sc::Int { !(x.c[i] as i32) as f64 } else { !(x.c[i] as u32) as f64 };
            }
            Ok(V::N(Vn { c, ..*x }))
        }
        _ => Err(format!("{op:?} doesn't work on this")),
    }
}

pub fn equal(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::N(x), V::N(y)) => {
            x.n == y.n && (0..x.n as usize).all(|i| convert(x.k, Sc::Float, x.c[i]) == convert(y.k, Sc::Float, y.c[i]))
        }
        (V::M(n, x), V::M(m, y)) => n == m && x[..(*n * *n) as usize] == y[..(*n * *n) as usize],
        (V::Arr(x), V::Arr(y)) => x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| equal(p, q)),
        (V::Obj(i, x), V::Obj(j, y)) => i == j && x.iter().zip(y.iter()).all(|(p, q)| equal(p, q)),
        _ => false,
    }
}

/// `xyzw`, `rgba` or `stpq` letters as the components they name.
pub fn swizzle(name: &str) -> Option<([u8; 4], u8)> {
    if name.is_empty() || name.len() > 4 {
        return None;
    }
    let set = ["xyzw", "rgba", "stpq"].into_iter().find(|s| name.chars().all(|c| s.contains(c)))?;
    let mut out = [0u8; 4];
    for (i, c) in name.chars().enumerate() {
        out[i] = set.find(c)? as u8;
    }
    Some((out, name.len() as u8))
}

/// `vec3(...)`, `int(...)`: the first N values of ARGS, as numbers of kind K
/// (one value alone fills a vector).
pub fn ctor_vec(k: Sc, n: u8, args: &[V]) -> R<V> {
    let mut buf = [0.0f64; 16];
    let mut len = 0;
    let mut single_scalar = false;
    for a in args {
        match a {
            V::N(x) => {
                single_scalar = args.len() == 1 && x.n == 1;
                for i in 0..x.n as usize {
                    if len < 16 {
                        buf[len] = convert(x.k, k, x.c[i]);
                        len += 1;
                    }
                }
            }
            V::M(m, d) => {
                for x in d.iter().take((*m * *m) as usize) {
                    if len < 16 {
                        buf[len] = convert(Sc::Float, k, *x as f64);
                        len += 1;
                    }
                }
            }
            _ => return Err("can't make a number from this".into()),
        }
    }
    let n = n as usize;
    let mut c = [0.0; 4];
    if single_scalar && n > 1 {
        c[..n].iter_mut().for_each(|x| *x = buf[0]);
    } else if len < n {
        return Err(format!("{} values are not enough for {n}", len));
    } else {
        c[..n].copy_from_slice(&buf[..n]);
    }
    Ok(V::N(Vn { k, n: n as u8, c }))
}

pub fn ctor_mat(n: u8, args: &[V]) -> R<V> {
    let n = n as usize;
    let mut m = [0.0f32; 16];
    if let [V::N(x)] = args {
        if x.n == 1 {
            for i in 0..n {
                m[i * n + i] = convert(x.k, Sc::Float, x.c[0]) as f32;
            }
            return Ok(V::M(n as u8, m));
        }
    }
    if let [V::M(from, d)] = args {
        let from = *from as usize;
        for c in 0..n {
            for r in 0..n {
                m[c * n + r] = if c < from && r < from {
                    d[c * from + r]
                } else if c == r {
                    1.0
                } else {
                    0.0
                };
            }
        }
        return Ok(V::M(n as u8, m));
    }
    let mut len = 0;
    for a in args {
        if let V::N(x) = a {
            for i in 0..x.n as usize {
                if len < n * n {
                    m[len] = convert(x.k, Sc::Float, x.c[i]) as f32;
                    len += 1;
                }
            }
        } else {
            return Err("can't make a matrix from this".into());
        }
    }
    if len < n * n {
        return Err(format!("{len} values are not enough for a matrix of {}", n * n));
    }
    Ok(V::M(n as u8, m))
}

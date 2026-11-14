//! Runs a shader: for each pixel, the `fragment` function over a copy of
//! the globals, with the built-ins (`UV`, `COLOR`...) set.

use std::sync::Arc;

use super::ast::*;
use super::val::*;
use crate::image::Image;

/// A texture a shader can sample.
pub struct Tex {
    pub img: Image,
    pub linear: bool,
    pub repeat: bool,
}

/// The built-ins of `fragment`, in the first slots of the globals.
pub const BUILTINS: &[&str] = &[
    "UV",
    "COLOR",
    "FRAGCOORD",
    "SCREEN_UV",
    "TEXTURE",
    "TEXTURE_PIXEL_SIZE",
    "SCREEN_PIXEL_SIZE",
    "TIME",
    "POINT_COORD",
    "MODULATE",
    "VERTEX",
    "NORMAL",
    "NORMAL_MAP",
    "NORMAL_MAP_DEPTH",
    "AT_LIGHT_PASS",
    "SPECULAR_SHININESS",
    "REGION_RECT",
];
const UV: usize = 0;
const COLOR: usize = 1;
const FRAGCOORD: usize = 2;
const SCREEN_UV: usize = 3;
const TEXTURE: usize = 4;
const TEXEL: usize = 5;
const SCREEN_PIXEL: usize = 6;
const TIME: usize = 7;
const POINT_COORD: usize = 8;
const MODULATE: usize = 9;
const VERTEX: usize = 10;
const NORMAL: usize = 11;
const NORMAL_MAP: usize = 12;
const NORMAL_MAP_DEPTH: usize = 13;
const AT_LIGHT_PASS: usize = 14;
const SPECULAR_SHININESS: usize = 15;
const REGION_RECT: usize = 16;

/// What one pixel may spend (loop turns and calls) before it's said to
/// run for too long.
const BUDGET: u32 = 1 << 22;
const MAX_DEPTH: u32 = 64;

enum Flow {
    Next,
    Break,
    Continue,
    Return(V),
    Discard,
}

/// A place to write to, inside a variable.
enum Step<'a> {
    Idx(usize),
    Fld(&'a Field),
}

enum Root {
    Local(usize),
    Global(usize),
}

pub struct Interp<'a> {
    pub p: &'a Program,
    pub globals: Vec<V>,
    pub tex: &'a [Tex],
    budget: u32,
    depth: u32,
    discarded: bool,
    started: bool,
}

impl<'a> Interp<'a> {
    pub fn new(p: &'a Program, tex: &'a [Tex]) -> Interp<'a> {
        let mut globals = vec![V::Void; p.globals];
        for (i, v) in globals.iter_mut().enumerate().take(BUILTINS.len()) {
            *v = match i {
                COLOR | FRAGCOORD | MODULATE | SPECULAR_SHININESS | REGION_RECT => V::floats(&[0.0; 4]),
                NORMAL | NORMAL_MAP => V::floats(&[0.0; 3]),
                TEXTURE => V::Tex(0),
                NORMAL_MAP_DEPTH | TIME => V::float(0.0),
                AT_LIGHT_PASS => V::boolean(false),
                _ => V::floats(&[0.0; 2]),
            };
        }
        Interp { p, globals, tex, budget: BUDGET, depth: 0, discarded: false, started: false }
    }

    /// Sets the consts, the globals that start with a value, and the
    /// uniforms (VALUES, one for each, in order).
    pub fn setup(&mut self, values: Vec<V>) -> R<()> {
        let mut fr = Vec::new();
        for (u, v) in self.p.uniforms.iter().zip(values) {
            self.globals[u.slot] = v;
        }
        for (slot, ty, init) in &self.p.init {
            let v = match init {
                Some(e) => coerce(ty, self.eval(e, &mut fr)?),
                None => zero(ty, &self.p.structs),
            };
            self.globals[*slot] = v;
        }
        Ok(())
    }

    /// The color of the pixel X, Y of a W x H image (as 0 to 1 floats).
    pub fn pixel(&mut self, base: &[V], x: u32, y: u32, w: u32, h: u32, time: f32) -> R<[f32; 4]> {
        // the first pixel copies it all; after, only what a shader can
        // change (copying a shared array again is slow across threads)
        if self.started {
            for &s in &self.p.mutable {
                self.globals[s] = base[s].clone();
            }
        } else {
            self.globals.clone_from_slice(base);
            self.started = true;
        }
        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
        let uv = V::floats(&[px / w as f32, py / h as f32]);
        let texel = V::floats(&[1.0 / w as f32, 1.0 / h as f32]);
        let g = &mut self.globals;
        g[SCREEN_UV] = uv.clone();
        g[UV] = uv;
        g[FRAGCOORD] = V::floats(&[px, py, 0.0, 1.0]);
        g[TEXEL] = texel.clone();
        g[SCREEN_PIXEL] = texel;
        g[TIME] = V::float(time);
        g[POINT_COORD] = V::floats(&[0.0, 0.0]);
        g[MODULATE] = V::floats(&[1.0; 4]);
        g[VERTEX] = V::floats(&[px, py]);
        g[NORMAL] = V::floats(&[0.0, 0.0, 1.0]);
        g[NORMAL_MAP] = V::floats(&[0.5, 0.5, 1.0]);
        g[NORMAL_MAP_DEPTH] = V::float(1.0);
        g[SPECULAR_SHININESS] = V::floats(&[1.0; 4]);
        g[REGION_RECT] = V::floats(&[0.0, 0.0, w as f32, h as f32]);
        let uvs = match &self.globals[UV] {
            V::N(u) => (u.c[0] as f32, u.c[1] as f32),
            _ => (0.0, 0.0),
        };
        self.globals[COLOR] = V::floats(&self.sample(0, uvs.0, uvs.1));
        self.budget = BUDGET;
        self.depth = 0;
        self.discarded = false;
        if let Some(fi) = self.p.fragment {
            let f = &self.p.funcs[fi];
            let mut fr = vec![V::Void; f.slots];
            if let Flow::Discard = self.block(&f.body, &mut fr)? {
                return Ok([0.0; 4]);
            }
        }
        match &self.globals[COLOR] {
            V::N(c) if c.n == 4 => Ok([c.c[0] as f32, c.c[1] as f32, c.c[2] as f32, c.c[3] as f32]),
            _ => Err("COLOR isn't a vec4".into()),
        }
    }

    /// The color of texture T at U, V.
    pub fn sample(&self, t: usize, u: f32, v: f32) -> [f32; 4] {
        let Some(tx) = self.tex.get(t) else { return [0.0; 4] };
        let (w, h) = (tx.img.width as i64, tx.img.height as i64);
        if w == 0 || h == 0 {
            return [0.0; 4];
        }
        let fetch = |x: i64, y: i64| -> [f32; 4] {
            let (x, y) =
                if tx.repeat { (x.rem_euclid(w), y.rem_euclid(h)) } else { (x.clamp(0, w - 1), y.clamp(0, h - 1)) };
            let p = tx.img.at(x as u32, y as u32);
            [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0, p[3] as f32 / 255.0]
        };
        if !tx.linear {
            return fetch((u * w as f32).floor() as i64, (v * h as f32).floor() as i64);
        }
        let (fx, fy) = (u * w as f32 - 0.5, v * h as f32 - 0.5);
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx_, ty) = (fx - x0, fy - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let (a, b, c, d) = (fetch(x0, y0), fetch(x0 + 1, y0), fetch(x0, y0 + 1), fetch(x0 + 1, y0 + 1));
        let mut o = [0.0; 4];
        for i in 0..4 {
            let top = a[i] * (1.0 - tx_) + b[i] * tx_;
            let bottom = c[i] * (1.0 - tx_) + d[i] * tx_;
            o[i] = top * (1.0 - ty) + bottom * ty;
        }
        o
    }

    pub fn texel(&self, t: usize, x: i64, y: i64) -> [f32; 4] {
        let Some(tx) = self.tex.get(t) else { return [0.0; 4] };
        let (w, h) = (tx.img.width as i64, tx.img.height as i64);
        if w == 0 || h == 0 {
            return [0.0; 4];
        }
        let p = tx.img.at(x.clamp(0, w - 1) as u32, y.clamp(0, h - 1) as u32);
        [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0, p[3] as f32 / 255.0]
    }

    fn tick(&mut self) -> R<()> {
        if self.budget == 0 {
            return Err("the shader runs for too long on a pixel (a loop that doesn't end?)".into());
        }
        self.budget -= 1;
        Ok(())
    }

    fn block(&mut self, b: &'a [S], fr: &mut Vec<V>) -> R<Flow> {
        for s in b {
            match self.stmt(s, fr)? {
                Flow::Next => {}
                f => return Ok(f),
            }
            if self.discarded {
                return Ok(Flow::Discard);
            }
        }
        Ok(Flow::Next)
    }

    /// One turn of a loop's body: Some(flow) if the loop is over.
    fn turn(&mut self, body: &'a [S], fr: &mut Vec<V>) -> R<Option<Flow>> {
        self.tick()?;
        Ok(match self.block(body, fr)? {
            Flow::Break => Some(Flow::Next),
            Flow::Return(v) => Some(Flow::Return(v)),
            Flow::Discard => Some(Flow::Discard),
            _ => None,
        })
    }

    fn stmt(&mut self, s: &'a S, fr: &mut Vec<V>) -> R<Flow> {
        match s {
            S::Expr(e) => {
                self.eval(e, fr)?;
            }
            S::Decl(slot, ty, init) => {
                let v = match init {
                    Some(e) => coerce(ty, self.eval(e, fr)?),
                    None => zero(ty, &self.p.structs),
                };
                fr[*slot] = v;
            }
            S::Block(b) => return self.block(b, fr),
            S::If(c, a, b) => {
                let branch = if self.eval(c, fr)?.truthy()? { a } else { b };
                return self.block(branch, fr);
            }
            S::For(init, cond, update, body) => {
                match self.block(init, fr)? {
                    Flow::Next => {}
                    f => return Ok(f),
                }
                loop {
                    if let Some(c) = cond {
                        if !self.eval(c, fr)?.truthy()? {
                            break;
                        }
                    }
                    if let Some(f) = self.turn(body, fr)? {
                        return Ok(f);
                    }
                    if let Some(u) = update {
                        self.eval(u, fr)?;
                    }
                }
            }
            S::While(c, body) => {
                while self.eval(c, fr)?.truthy()? {
                    if let Some(f) = self.turn(body, fr)? {
                        return Ok(f);
                    }
                }
            }
            S::Do(body, c) => loop {
                if let Some(f) = self.turn(body, fr)? {
                    return Ok(f);
                }
                if !self.eval(c, fr)?.truthy()? {
                    break;
                }
            },
            S::Switch(e, cases) => {
                let v = self.eval(e, fr)?;
                let mut start = None;
                for (i, c) in cases.iter().enumerate() {
                    if let Some(ce) = &c.value {
                        if equal(&v, &self.eval(ce, fr)?) {
                            start = Some(i);
                            break;
                        }
                    }
                }
                let start = start.or_else(|| cases.iter().position(|c| c.value.is_none()));
                if let Some(start) = start {
                    for c in &cases[start..] {
                        match self.block(&c.body, fr)? {
                            Flow::Next => {}
                            Flow::Break => break,
                            f => return Ok(f),
                        }
                    }
                }
            }
            S::Break => return Ok(Flow::Break),
            S::Continue => return Ok(Flow::Continue),
            S::Return(e) => {
                let v = match e {
                    Some(e) => self.eval(e, fr)?,
                    None => V::Void,
                };
                return Ok(Flow::Return(v));
            }
            S::Discard => {
                self.discarded = true;
                return Ok(Flow::Discard);
            }
        }
        Ok(Flow::Next)
    }

    /// The variable E is, if it's one: no copy.
    fn peek<'b>(&'b self, e: &E, fr: &'b [V]) -> Option<&'b V> {
        match e {
            E::Local(i) => Some(&fr[*i]),
            E::Global(i) => Some(&self.globals[*i]),
            _ => None,
        }
    }

    pub fn eval(&mut self, e: &'a E, fr: &mut Vec<V>) -> R<V> {
        match e {
            E::Lit(v) => Ok(v.clone()),
            E::Local(i) => Ok(fr[*i].clone()),
            E::Global(i) => Ok(self.globals[*i].clone()),
            E::Un(op, a) => unary(*op, &self.eval(a, fr)?),
            E::Bin(op, a, b) => match op {
                Bin::And => Ok(V::boolean(self.eval(a, fr)?.truthy()? && self.eval(b, fr)?.truthy()?)),
                Bin::Or => Ok(V::boolean(self.eval(a, fr)?.truthy()? || self.eval(b, fr)?.truthy()?)),
                Bin::Eq => {
                    let (x, y) = (self.eval(a, fr)?, self.eval(b, fr)?);
                    Ok(V::boolean(equal(&x, &y)))
                }
                Bin::Ne => {
                    let (x, y) = (self.eval(a, fr)?, self.eval(b, fr)?);
                    Ok(V::boolean(!equal(&x, &y)))
                }
                _ => {
                    let (x, y) = (self.eval(a, fr)?, self.eval(b, fr)?);
                    binary(*op, &x, &y)
                }
            },
            E::Cond(c, a, b) => {
                if self.eval(c, fr)?.truthy()? {
                    self.eval(a, fr)
                } else {
                    self.eval(b, fr)
                }
            }
            E::Call(callee, args) => self.call(callee, args, fr),
            E::Field(b, f) => {
                if let Some(v) = self.peek(b, fr) {
                    return field(v, f, &self.p.structs);
                }
                let v = self.eval(b, fr)?;
                field(&v, f, &self.p.structs)
            }
            E::Index(b, i) => {
                let idx = self.eval(i, fr)?.as_i32("an index")?;
                if let Some(v) = self.peek(b, fr) {
                    return index(v, idx);
                }
                let v = self.eval(b, fr)?;
                index(&v, idx)
            }
            E::Assign(lhs, op, rhs) => {
                let r = self.eval(rhs, fr)?;
                let v = match op {
                    None => r,
                    Some(op) => {
                        let cur = self.eval(lhs, fr)?;
                        binary(*op, &cur, &r)?
                    }
                };
                self.store(lhs, v, fr)
            }
            E::IncDec(target, delta, post) => {
                let cur = self.eval(target, fr)?;
                let one = match &cur {
                    V::N(x) => V::N(Vn::scalar(x.k, *delta)),
                    _ => return Err("++ and -- need a number".into()),
                };
                let new = binary(Bin::Add, &cur, &one)?;
                self.store(target, new.clone(), fr)?;
                Ok(if *post { cur } else { new })
            }
            E::Comma(list) => {
                let mut last = V::Void;
                for e in list {
                    last = self.eval(e, fr)?;
                }
                Ok(last)
            }
        }
    }

    fn call(&mut self, callee: &'a Callee, args: &'a [E], fr: &mut Vec<V>) -> R<V> {
        if let Callee::User(fi) = callee {
            return self.call_user(*fi, args, fr);
        }
        // most calls take a few values: no allocation
        let mut small = [V::Void, V::Void, V::Void, V::Void];
        let mut big = Vec::new();
        let vals: &[V] = if args.len() <= 4 {
            for (i, a) in args.iter().enumerate() {
                small[i] = self.eval(a, fr)?;
            }
            &small[..args.len()]
        } else {
            for a in args {
                big.push(self.eval(a, fr)?);
            }
            &big
        };
        match callee {
            Callee::Builtin(f) => f(self, vals),
            Callee::Ctor(ty) => self.construct(ty, vals),
            Callee::User(_) => unreachable!(),
        }
    }

    fn construct(&self, ty: &Ty, args: &[V]) -> R<V> {
        match ty {
            Ty::Vec(k, n) => ctor_vec(*k, *n, args),
            Ty::Mat(n) => ctor_mat(*n, args),
            Ty::Array(t, n) => {
                let n = if *n == 0 { args.len() } else { *n };
                if args.len() != n {
                    return Err(format!("an array of {n} was given {} values", args.len()));
                }
                Ok(V::Arr(Arc::new(args.iter().map(|a| coerce(t, a.clone())).collect())))
            }
            Ty::Struct(i) => {
                let def = &self.p.structs[*i];
                if args.len() != def.fields.len() {
                    return Err(format!("{} has {} fields, not {}", def.name, def.fields.len(), args.len()));
                }
                let fields = def.fields.iter().zip(args).map(|((_, t), a)| coerce(t, a.clone())).collect();
                Ok(V::Obj(*i, Arc::new(fields)))
            }
            _ => Err("can't make this".into()),
        }
    }

    fn call_user(&mut self, fi: usize, args: &'a [E], fr: &mut Vec<V>) -> R<V> {
        let p: &'a Program = self.p;
        let f = &p.funcs[fi];
        self.tick()?;
        if self.depth >= MAX_DEPTH {
            return Err(format!("{} calls itself too deep", f.name));
        }
        let mut nf = vec![V::Void; f.slots];
        for (param, a) in f.params.iter().zip(args) {
            nf[param.slot] = match param.dir {
                Dir::Out => zero(&param.ty, &p.structs),
                _ => coerce(&param.ty, self.eval(a, fr)?),
            };
        }
        self.depth += 1;
        let flow = self.block(&f.body, &mut nf);
        self.depth -= 1;
        let ret = match flow? {
            Flow::Return(v) => coerce(&f.ret, v),
            _ => V::Void,
        };
        for (param, a) in f.params.iter().zip(args) {
            if param.dir != Dir::In {
                let v = nf[param.slot].clone();
                self.store(a, v, fr)?;
            }
        }
        Ok(ret)
    }

    /// Puts V in the place E is (a variable, an element, a field, some
    /// components), and gives back what is there now.
    fn store(&mut self, e: &'a E, v: V, fr: &mut Vec<V>) -> R<V> {
        // a variable of one kind of number stays so
        let like = |old: &V, v: V| match (old, &v) {
            (V::N(o), V::N(n)) if o.k != n.k && o.n == n.n => coerce(&Ty::Vec(o.k, o.n), v),
            _ => v,
        };
        match e {
            E::Local(i) => {
                let v = like(&fr[*i], v);
                fr[*i] = v.clone();
                return Ok(v);
            }
            E::Global(i) => {
                let v = like(&self.globals[*i], v);
                self.globals[*i] = v.clone();
                return Ok(v);
            }
            _ => {}
        }
        let mut steps = Vec::new();
        let root = self.path(e, fr, &mut steps)?;
        steps.reverse();
        let target = match root {
            Root::Local(i) => &mut fr[i],
            Root::Global(i) => &mut self.globals[i],
        };
        let mut done = V::Void;
        let mut v = Some(v);
        modify(target, &steps, &self.p.structs, &mut |slot| {
            let new = like(slot, v.take().unwrap_or(V::Void));
            *slot = new.clone();
            done = new;
            Ok(())
        })?;
        Ok(done)
    }

    /// The variable E is inside of, and the steps in, last first.
    fn path(&mut self, e: &'a E, fr: &mut Vec<V>, steps: &mut Vec<Step<'a>>) -> R<Root> {
        match e {
            E::Local(i) => Ok(Root::Local(*i)),
            E::Global(i) => Ok(Root::Global(*i)),
            E::Field(b, f) => {
                steps.push(Step::Fld(f));
                self.path(b, fr, steps)
            }
            E::Index(b, i) => {
                let idx = self.eval(i, fr)?.as_i32("an index")?;
                steps.push(Step::Idx(idx.max(0) as usize));
                self.path(b, fr, steps)
            }
            _ => Err("this can't be assigned to".into()),
        }
    }
}

fn index(v: &V, i: i32) -> R<V> {
    let i = i.max(0) as usize;
    match v {
        V::Arr(a) => a.get(i.min(a.len().saturating_sub(1))).cloned().ok_or_else(|| "an empty array".to_string()),
        V::N(x) if x.n > 1 => Ok(V::N(Vn::scalar(x.k, x.c[i.min(x.n as usize - 1)]))),
        V::M(n, m) => {
            let n = *n as usize;
            let c = i.min(n - 1);
            let mut col = [0.0; 4];
            for r in 0..n {
                col[r] = m[c * n + r] as f64;
            }
            Ok(V::N(Vn { k: Sc::Float, n: n as u8, c: col }))
        }
        _ => Err("this can't be indexed".into()),
    }
}

fn field(v: &V, f: &Field, structs: &[StructDef]) -> R<V> {
    match v {
        V::N(x) => {
            let Some((idx, len)) = f.swz else { return Err(format!(".{} is not a field of a vector", f.name)) };
            let mut c = [0.0; 4];
            for j in 0..len as usize {
                let from = idx[j] as usize;
                if from >= x.n as usize {
                    return Err(format!(".{} reaches past the end of the vector", f.name));
                }
                c[j] = x.c[from];
            }
            Ok(V::N(Vn { k: x.k, n: len, c }))
        }
        V::Obj(i, fields) => {
            let pos = structs[*i].fields.iter().position(|(n, _)| *n == f.name);
            match pos {
                Some(p) => Ok(fields[p].clone()),
                None => Err(format!("{} has no field {}", structs[*i].name, f.name)),
            }
        }
        _ => Err(format!(".{} doesn't work here", f.name)),
    }
}

/// Runs F on the place STEPS lead to inside V.
fn modify(v: &mut V, steps: &[Step], structs: &[StructDef], f: &mut dyn FnMut(&mut V) -> R<()>) -> R<()> {
    let Some((step, rest)) = steps.split_first() else { return f(v) };
    match (v, step) {
        (V::Arr(a), Step::Idx(i)) => {
            let a = Arc::make_mut(a);
            let last = a.len().saturating_sub(1);
            match a.get_mut((*i).min(last)) {
                Some(e) => modify(e, rest, structs, f),
                None => Err("an empty array".into()),
            }
        }
        (V::Obj(si, fields), Step::Fld(fl)) => {
            let Some(pos) = structs[*si].fields.iter().position(|(n, _)| *n == fl.name) else {
                return Err(format!("{} has no field {}", structs[*si].name, fl.name));
            };
            modify(&mut Arc::make_mut(fields)[pos], rest, structs, f)
        }
        (V::N(x), Step::Idx(i)) => {
            let i = (*i).min(x.n as usize - 1);
            let mut t = V::N(Vn::scalar(x.k, x.c[i]));
            modify(&mut t, rest, structs, f)?;
            if let V::N(s) = t {
                x.c[i] = convert(s.k, x.k, s.c[0]);
            }
            Ok(())
        }
        (V::N(x), Step::Fld(fl)) => {
            let Some((idx, len)) = fl.swz else { return Err(format!(".{} is not a field of a vector", fl.name)) };
            let mut c = [0.0; 4];
            for j in 0..len as usize {
                if idx[j] >= x.n {
                    return Err(format!(".{} reaches past the end of the vector", fl.name));
                }
                c[j] = x.c[idx[j] as usize];
            }
            let mut t = V::N(Vn { k: x.k, n: len, c });
            modify(&mut t, rest, structs, f)?;
            if let V::N(s) = t {
                for j in 0..(len as usize).min(s.n as usize) {
                    x.c[idx[j] as usize] = convert(s.k, x.k, s.c[j]);
                }
            }
            Ok(())
        }
        (V::M(n, m), Step::Idx(i)) => {
            let n = *n as usize;
            let col = (*i).min(n - 1);
            let mut c = [0.0; 4];
            for r in 0..n {
                c[r] = m[col * n + r] as f64;
            }
            let mut t = V::N(Vn { k: Sc::Float, n: n as u8, c });
            modify(&mut t, rest, structs, f)?;
            if let V::N(s) = t {
                for r in 0..n.min(s.n as usize) {
                    m[col * n + r] = s.c[r] as f32;
                }
            }
            Ok(())
        }
        _ => Err("this can't be assigned to".into()),
    }
}

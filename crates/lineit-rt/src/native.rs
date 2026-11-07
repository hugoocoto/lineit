//! Rust modules, from packages.
//!
//! A package's `native/` folder is a cargo library crate that depends on
//! `lineit-rt`, implements [`Native`] for each of its modules and lists
//! them with [`export_modules!`]. Their pins are said in the package's
//! `modules/NAME.native.lua` (like a Lua module's, with `native = "Type"`),
//! so lineit knows them without building anything:
//!
//! ```ignore
//! use lineit_rt::native::{Native, Values};
//! use lineit_rt::Value;
//!
//! #[derive(Default)]
//! pub struct Reverse;
//!
//! impl Native for Reverse {
//!     fn new(_params: &Values) -> Self { Reverse }
//!     fn eval(&mut self, i: &Values) -> Result<Values, String> {
//!         let text: String = i["text"].str().chars().rev().collect();
//!         Ok(Values::from([("result".into(), Value::Str(text.into()))]))
//!     }
//! }
//!
//! lineit_rt::export_modules!(Reverse);
//! ```
//!
//! A compiled program depends on the crate and calls it directly
//! ([`Host`]). The editor builds it as a `cdylib` and calls it through the C
//! ABI the macro makes, with values as bytes ([`encode`], [`decode`]): so
//! the editor and the crate don't need the same compiler, only the same
//! `lineit-rt`.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::ext::{Ext, Ticked};
use crate::image::Image;
use crate::types::{RecordType, Type};
use crate::value::{Record, Value};

/// Values by pin (or param) name.
pub type Values = BTreeMap<String, Value>;

/// The C ABI's version: the editor refuses a library with another.
pub const ABI: u32 = 1;

pub trait Native: Send {
    /// A square of the module, with its params.
    fn new(params: &Values) -> Self
    where
        Self: Sized;

    /// A pure module's outputs, from its inputs. Outputs left out get their
    /// type's default.
    fn eval(&mut self, inputs: &Values) -> Result<Values, String> {
        let _ = inputs;
        Err("it has no eval".into())
    }

    /// Tick input INPUT fired (None: the program started). Returns the
    /// outputs to set and the tick outputs to fire, in order.
    fn tick(&mut self, input: Option<&str>, inputs: &Values) -> Result<(Values, Vec<String>), String> {
        let _ = (input, inputs);
        Ok((Values::new(), Vec::new()))
    }
}

/// A module of a crate: its type's name, and how to make one.
pub struct Entry {
    pub name: &'static str,
    pub new: fn(&Values) -> Box<dyn Native>,
}

fn named(v: &[(&str, Value)]) -> Values {
    v.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

/// V as a value of type T: numbers convert, records take the type's fields
/// by name.
pub fn coerce(v: Value, t: &Type) -> Result<Value, String> {
    Ok(match (v, t) {
        (Value::Int(i), Type::Float) => Value::Float(i as f64),
        (Value::Float(f), Type::Int) => Value::Int(f as i64),
        (v @ Value::Int(_), Type::Int)
        | (v @ Value::Float(_), Type::Float)
        | (v @ Value::Bool(_), Type::Bool)
        | (v @ Value::Str(_), Type::Str)
        | (v @ Value::Image(_), Type::Image) => v,
        (Value::List(l), Type::List(e)) => {
            Value::List(Arc::new(l.iter().cloned().map(|x| coerce(x, e)).collect::<Result<_, _>>()?))
        }
        (Value::Map(m), Type::Map(e)) => Value::Map(Arc::new(
            m.iter().map(|(k, x)| Ok((k.clone(), coerce(x.clone(), e)?))).collect::<Result<_, String>>()?,
        )),
        (Value::Record(r), Type::Record(rt)) => {
            let mut fields = Vec::new();
            for (name, ft) in &rt.fields {
                let i = r.ty.fields.iter().position(|(n, _)| n == name);
                fields.push(match i {
                    Some(i) => coerce(r.fields[i].clone(), ft)?,
                    None => Value::default_of(ft),
                });
            }
            Value::Record(Arc::new(Record { ty: rt.clone(), fields }))
        }
        (v, t) => return Err(format!("expected {t}, got {}", v.ty())),
    })
}

fn outputs(mut got: Values, outputs: &[(&str, &Type)], defaults: bool) -> Result<Vec<Option<Value>>, String> {
    outputs
        .iter()
        .map(|(n, t)| match got.remove(*n) {
            Some(v) => coerce(v, t).map(Some).map_err(|e| format!("output `{n}`: {e}")),
            None => Ok(defaults.then(|| Value::default_of(t))),
        })
        .collect()
}

/// Runs a crate's module in a compiled program.
pub struct Host {
    new: fn(&Values) -> Box<dyn Native>,
    squares: Vec<Box<dyn Native>>,
}

impl Host {
    pub fn new(entries: Vec<Entry>, name: &str) -> Result<Host, String> {
        let e =
            entries.into_iter().find(|e| e.name == name).ok_or_else(|| format!("the crate has no module `{name}`"))?;
        Ok(Host { new: e.new, squares: Vec::new() })
    }
}

impl Ext for Host {
    fn instance(&mut self, params: &[(&str, Value)]) -> Result<usize, String> {
        self.squares.push((self.new)(&named(params)));
        Ok(self.squares.len() - 1)
    }
    fn eval(&mut self, inst: usize, inputs: &[(&str, Value)], outs: &[(&str, &Type)]) -> Result<Vec<Value>, String> {
        let got = self.squares[inst].eval(&named(inputs))?;
        Ok(outputs(got, outs, true)?.into_iter().map(|v| v.unwrap_or(Value::Int(0))).collect())
    }
    fn tick(
        &mut self,
        inst: usize,
        input: Option<&str>,
        inputs: &[(&str, Value)],
        outs: &[(&str, &Type)],
    ) -> Result<Ticked, String> {
        let (got, fired) = self.squares[inst].tick(input, &named(inputs))?;
        Ok((outputs(got, outs, false)?, fired))
    }
}

// ------------------------------------------------------------ bytes

fn put_len(out: &mut Vec<u8>, n: usize) {
    out.extend_from_slice(&(n as u64).to_le_bytes());
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    put_len(out, s.len());
    out.extend_from_slice(s.as_bytes());
}

/// V as bytes, for the C ABI.
pub fn encode(v: &Value, out: &mut Vec<u8>) {
    match v {
        Value::Int(i) => {
            out.push(0);
            out.extend_from_slice(&i.to_le_bytes());
        }
        Value::Float(f) => {
            out.push(1);
            out.extend_from_slice(&f.to_le_bytes());
        }
        Value::Bool(b) => {
            out.push(2);
            out.push(*b as u8);
        }
        Value::Str(s) => {
            out.push(3);
            put_str(out, s);
        }
        Value::List(l) => {
            out.push(4);
            put_len(out, l.len());
            for x in l.iter() {
                encode(x, out);
            }
        }
        Value::Map(m) => {
            out.push(5);
            put_len(out, m.len());
            for (k, x) in m.iter() {
                put_str(out, k);
                encode(x, out);
            }
        }
        Value::Image(i) => {
            out.push(6);
            out.extend_from_slice(&i.width.to_le_bytes());
            out.extend_from_slice(&i.height.to_le_bytes());
            out.extend_from_slice(&i.pixels);
        }
        Value::Record(r) => {
            out.push(7);
            put_str(out, &r.ty.name);
            put_len(out, r.fields.len());
            for ((n, _), x) in r.ty.fields.iter().zip(&r.fields) {
                put_str(out, n);
                encode(x, out);
            }
        }
    }
}

fn take<'a>(b: &mut &'a [u8], n: usize) -> Result<&'a [u8], String> {
    if b.len() < n {
        return Err("bytes end too soon".into());
    }
    let (a, rest) = b.split_at(n);
    *b = rest;
    Ok(a)
}

fn take_len(b: &mut &[u8]) -> Result<usize, String> {
    let n = u64::from_le_bytes(take(b, 8)?.try_into().unwrap()) as usize;
    if n > b.len() * 8 + 64 {
        return Err("a length that's too long".into());
    }
    Ok(n)
}

fn take_str(b: &mut &[u8]) -> Result<String, String> {
    let n = take_len(b)?;
    String::from_utf8(take(b, n)?.to_vec()).map_err(|_| "a string that isn't UTF-8".into())
}

/// Reads a value [`encode`] wrote. A record comes with a type of its own
/// (its fields' types are its values'): [`coerce`] gives it the real one.
pub fn decode(b: &mut &[u8]) -> Result<Value, String> {
    let tag = take(b, 1)?[0];
    Ok(match tag {
        0 => Value::Int(i64::from_le_bytes(take(b, 8)?.try_into().unwrap())),
        1 => Value::Float(f64::from_le_bytes(take(b, 8)?.try_into().unwrap())),
        2 => Value::Bool(take(b, 1)?[0] != 0),
        3 => Value::Str(take_str(b)?.into()),
        4 => {
            let n = take_len(b)?;
            let mut l = Vec::with_capacity(n.min(1 << 16));
            for _ in 0..n {
                l.push(decode(b)?);
            }
            Value::List(Arc::new(l))
        }
        5 => {
            let n = take_len(b)?;
            let mut m = BTreeMap::new();
            for _ in 0..n {
                let k = take_str(b)?;
                m.insert(k, decode(b)?);
            }
            Value::Map(Arc::new(m))
        }
        6 => {
            let w = u32::from_le_bytes(take(b, 4)?.try_into().unwrap());
            let h = u32::from_le_bytes(take(b, 4)?.try_into().unwrap());
            let n = w as usize * h as usize * 4;
            Value::Image(Image { width: w, height: h, pixels: Arc::new(take(b, n)?.to_vec()) })
        }
        7 => {
            let name = take_str(b)?;
            let n = take_len(b)?;
            let (mut fields, mut values) = (Vec::new(), Vec::new());
            for _ in 0..n {
                let f = take_str(b)?;
                let v = decode(b)?;
                fields.push((f, v.ty()));
                values.push(v);
            }
            Value::Record(Arc::new(Record { ty: Arc::new(RecordType { name, fields }), fields: values }))
        }
        t => return Err(format!("no value has tag {t}")),
    })
}

// ------------------------------------------------------------ the C ABI

/// Bytes the library made, for the editor to read and give back to
/// `lineit_native_free_buf`.
#[repr(C)]
pub struct Buf {
    pub ptr: *mut u8,
    pub len: usize,
    pub cap: usize,
}

impl Buf {
    pub fn from_vec(v: Vec<u8>) -> Buf {
        let mut v = std::mem::ManuallyDrop::new(v);
        Buf { ptr: v.as_mut_ptr(), len: v.len(), cap: v.capacity() }
    }

    /// # Safety
    /// B came from [`Buf::from_vec`], in this same library.
    pub unsafe fn into_vec(b: Buf) -> Vec<u8> {
        unsafe { Vec::from_raw_parts(b.ptr, b.len, b.cap) }
    }
}

/// What `lineit_native_call` does.
pub const CALL_EVAL: i32 = 0;
pub const CALL_TICK: i32 = 1;
pub const CALL_START: i32 = 2;

fn panic_text(p: Box<dyn std::any::Any + Send>) -> String {
    p.downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| p.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "it panicked".into())
}

/// For [`export_modules!`]: makes a square of module NAME.
///
/// # Safety
/// The pointers and lengths are a valid name and the [`encode`]d params.
pub unsafe fn export_new(
    entries: &[Entry],
    name: *const u8,
    name_len: usize,
    params: *const u8,
    params_len: usize,
) -> *mut std::ffi::c_void {
    let name = unsafe { std::slice::from_raw_parts(name, name_len) };
    let mut p = unsafe { std::slice::from_raw_parts(params, params_len) };
    let Some(e) = entries.iter().find(|e| e.name.as_bytes() == name) else { return std::ptr::null_mut() };
    let params = match decode(&mut p) {
        Ok(Value::Map(m)) => (*m).clone(),
        _ => Values::new(),
    };
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (e.new)(&params))) {
        Ok(b) => Box::into_raw(Box::new(b)) as *mut std::ffi::c_void,
        Err(_) => std::ptr::null_mut(),
    }
}

/// For [`export_modules!`]: an eval, tick or start of a square.
///
/// # Safety
/// STATE came from [`export_new`]; the rest are valid bytes and OUT a valid
/// place for a [`Buf`].
pub unsafe fn export_call(
    state: *mut std::ffi::c_void,
    what: i32,
    input: *const u8,
    input_len: usize,
    args: *const u8,
    args_len: usize,
    out: *mut Buf,
) -> i32 {
    let m = unsafe { &mut *(state as *mut Box<dyn Native>) };
    let input = unsafe { std::slice::from_raw_parts(input, input_len) };
    let input = std::str::from_utf8(input).unwrap_or("");
    let mut a = unsafe { std::slice::from_raw_parts(args, args_len) };
    let args = match decode(&mut a) {
        Ok(Value::Map(m)) => (*m).clone(),
        _ => Values::new(),
    };
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<Value, String> {
        match what {
            CALL_EVAL => Ok(Value::Map(Arc::new(m.eval(&args)?))),
            _ => {
                let (got, fired) = m.tick(if what == CALL_TICK { Some(input) } else { None }, &args)?;
                let fired = fired.into_iter().map(|s| Value::Str(s.into())).collect();
                Ok(Value::List(Arc::new(vec![Value::Map(Arc::new(got)), Value::List(Arc::new(fired))])))
            }
        }
    }));
    let (code, bytes) = match r {
        Ok(Ok(v)) => {
            let mut b = Vec::new();
            encode(&v, &mut b);
            (0, b)
        }
        Ok(Err(e)) => (1, e.into_bytes()),
        Err(p) => (1, panic_text(p).into_bytes()),
    };
    unsafe { out.write(Buf::from_vec(bytes)) };
    code
}

/// Lists a crate's modules, and makes it a library the editor can load.
/// Put it once in the crate's `lib.rs`, with every type that implements
/// [`Native`]; the crate's `Cargo.toml` needs
/// `crate-type = ["cdylib", "rlib"]`.
#[macro_export]
macro_rules! export_modules {
    ($($ty:ident),* $(,)?) => {
        /// The modules of this crate, for a compiled program.
        pub fn lineit_natives() -> Vec<$crate::native::Entry> {
            vec![$($crate::native::Entry {
                name: stringify!($ty),
                new: |p| Box::new(<$ty as $crate::native::Native>::new(p)),
            }),*]
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn lineit_native_abi() -> u32 {
            $crate::native::ABI
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn lineit_native_new(
            name: *const u8,
            name_len: usize,
            params: *const u8,
            params_len: usize,
        ) -> *mut ::std::ffi::c_void {
            unsafe { $crate::native::export_new(&lineit_natives(), name, name_len, params, params_len) }
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn lineit_native_call(
            state: *mut ::std::ffi::c_void,
            what: i32,
            input: *const u8,
            input_len: usize,
            args: *const u8,
            args_len: usize,
            out: *mut $crate::native::Buf,
        ) -> i32 {
            unsafe { $crate::native::export_call(state, what, input, input_len, args, args_len, out) }
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn lineit_native_free(state: *mut ::std::ffi::c_void) {
            if !state.is_null() {
                drop(unsafe { Box::from_raw(state as *mut Box<dyn $crate::native::Native>) });
            }
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn lineit_native_free_buf(b: $crate::native::Buf) {
            drop(unsafe { $crate::native::Buf::into_vec(b) });
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Twice;
    impl Native for Twice {
        fn new(_: &Values) -> Self {
            Twice
        }
        fn eval(&mut self, i: &Values) -> Result<Values, String> {
            Ok(Values::from([("y".into(), Value::Int(i["x"].int() * 2))]))
        }
    }

    #[test]
    fn host_and_bytes() {
        let mut h = Host::new(vec![Entry { name: "Twice", new: |p| Box::new(Twice::new(p)) }], "Twice").unwrap();
        let i = h.instance(&[]).unwrap();
        let out = h.eval(i, &[("x", Value::Int(21))], &[("y", &Type::Float)]).unwrap();
        assert_eq!(out, [Value::Float(42.0)]);
        let rt = Arc::new(RecordType { name: "p".into(), fields: vec![("x".into(), Type::Float)] });
        let v = Value::List(Arc::new(vec![
            Value::Str("é".into()),
            Value::Record(Arc::new(Record { ty: rt.clone(), fields: vec![Value::Float(1.5)] })),
            Value::Image(Image::new(2, 1, [1, 2, 3, 4])),
        ]));
        let mut b = Vec::new();
        encode(&v, &mut b);
        let back = decode(&mut b.as_slice()).unwrap();
        assert_eq!(back.text(), v.text());
        let typed = coerce(back.list()[1].clone(), &Type::Record(rt)).unwrap();
        assert_eq!(typed, v.list()[1]);
        assert!(decode(&mut &b[..b.len() - 1]).is_err());
    }
}

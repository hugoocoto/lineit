//! Code squares in C: the values lineit gives and takes, as one C struct
//! ([`Lv`], `lineit_value` in C), and a square of one ([`CSquare`]).
//!
//! lineit writes C glue for each square (in lineit-core) that turns these
//! into the function's own types (`int64_t`, `list_int`, records...) and
//! back, so the C compiler, not lineit, makes the call. What the C code
//! takes from `lineit_alloc` ([`alloc`]) is freed after each call.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::{Arc, OnceLock};

use crate::native::Values;
use crate::types::{RecordType, Type};
use crate::value::{Record, Value};

/// A value, as C sees it. `typedef struct lineit_value {...}` in the
/// header lineit writes is the same.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Lv {
    pub tag: u32,
    /// An int; a bool (0 or 1).
    pub i: i64,
    pub f: f64,
    pub s: *const c_char,
    /// A list's items, a map's values, a record's fields.
    pub items: *const Lv,
    /// A map's keys, a record's field names.
    pub keys: *const *const c_char,
    pub len: usize,
}

pub const TAG_INT: u32 = 0;
pub const TAG_FLOAT: u32 = 1;
pub const TAG_BOOL: u32 = 2;
pub const TAG_STR: u32 = 3;
pub const TAG_LIST: u32 = 4;
pub const TAG_MAP: u32 = 5;
pub const TAG_RECORD: u32 = 6;

impl Default for Lv {
    fn default() -> Self {
        Lv { tag: TAG_INT, i: 0, f: 0.0, s: std::ptr::null(), items: std::ptr::null(), keys: std::ptr::null(), len: 0 }
    }
}

/// What the [`Lv`]s made of values point to, kept until the call is done.
#[derive(Default)]
pub struct Keep {
    strings: Vec<CString>,
    lists: Vec<Vec<Lv>>,
    keys: Vec<Vec<*const c_char>>,
}

impl Keep {
    fn str(&mut self, s: &str) -> *const c_char {
        // a NUL in it ends it, as C reads it
        let c = CString::new(s.split('\0').next().unwrap_or("")).unwrap_or_default();
        let p = c.as_ptr();
        self.strings.push(c);
        p
    }

    fn list(&mut self, items: Vec<Lv>) -> *const Lv {
        let p = items.as_ptr();
        self.lists.push(items);
        p
    }

    fn keys(&mut self, keys: &[&str]) -> *const *const c_char {
        let k: Vec<*const c_char> = keys.iter().map(|k| self.str(k)).collect();
        let p = k.as_ptr();
        self.keys.push(k);
        p
    }

    /// V for C.
    pub fn lv(&mut self, v: &Value) -> Result<Lv, String> {
        let mut x = Lv::default();
        match v {
            Value::Int(i) => x.i = *i,
            Value::Float(f) => {
                x.tag = TAG_FLOAT;
                x.f = *f;
            }
            Value::Bool(b) => {
                x.tag = TAG_BOOL;
                x.i = *b as i64;
            }
            Value::Str(s) => {
                x.tag = TAG_STR;
                x.s = self.str(s);
            }
            Value::List(l) => {
                x.tag = TAG_LIST;
                let items = l.iter().map(|v| self.lv(v)).collect::<Result<_, _>>()?;
                x.items = self.list(items);
                x.len = l.len();
            }
            Value::Map(m) => {
                x.tag = TAG_MAP;
                let items = m.values().map(|v| self.lv(v)).collect::<Result<_, _>>()?;
                let keys: Vec<&str> = m.keys().map(String::as_str).collect();
                x.items = self.list(items);
                x.keys = self.keys(&keys);
                x.len = m.len();
            }
            Value::Record(r) => {
                x.tag = TAG_RECORD;
                let items = r.fields.iter().map(|v| self.lv(v)).collect::<Result<_, _>>()?;
                let keys: Vec<&str> = r.ty.fields.iter().map(|(n, _)| n.as_str()).collect();
                x.items = self.list(items);
                x.keys = self.keys(&keys);
                x.len = r.fields.len();
            }
            Value::Image(_) => return Err("images can't go to C".into()),
        }
        Ok(x)
    }
}

unsafe fn c_str(p: *const c_char) -> String {
    if p.is_null() { String::new() } else { unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned() }
}

/// What C gave back, as a value of the type it looks like: a record comes
/// with a type of its own, which [`crate::native::coerce`] makes the real one.
///
/// # Safety
/// V's pointers are valid, as the C glue makes them.
pub unsafe fn value(v: &Lv) -> Result<Value, String> {
    let items = |v: &Lv| -> &[Lv] {
        if v.items.is_null() || v.len == 0 { &[] } else { unsafe { std::slice::from_raw_parts(v.items, v.len) } }
    };
    let keys = |v: &Lv| -> Vec<String> {
        if v.keys.is_null() || v.len == 0 {
            return Vec::new();
        }
        unsafe { std::slice::from_raw_parts(v.keys, v.len) }.iter().map(|k| unsafe { c_str(*k) }).collect()
    };
    Ok(match v.tag {
        TAG_INT => Value::Int(v.i),
        TAG_FLOAT => Value::Float(v.f),
        TAG_BOOL => Value::Bool(v.i != 0),
        TAG_STR => Value::Str(unsafe { c_str(v.s) }.into()),
        TAG_LIST => Value::List(Arc::new(items(v).iter().map(|x| unsafe { value(x) }).collect::<Result<_, _>>()?)),
        TAG_MAP => {
            let mut m = BTreeMap::new();
            for (k, x) in keys(v).into_iter().zip(items(v)) {
                m.insert(k, unsafe { value(x) }?);
            }
            Value::Map(Arc::new(m))
        }
        TAG_RECORD => {
            let values: Vec<Value> = items(v).iter().map(|x| unsafe { value(x) }).collect::<Result<_, _>>()?;
            let fields = keys(v).into_iter().zip(&values).map(|(k, x)| (k, x.ty())).collect();
            Value::Record(Arc::new(Record { ty: Arc::new(RecordType { name: String::new(), fields }), fields: values }))
        }
        t => return Err(format!("C gave back a value with tag {t}")),
    })
}

thread_local! {
    /// What C took with `lineit_alloc` during this call.
    static ARENA: RefCell<Vec<Vec<u64>>> = const { RefCell::new(Vec::new()) };
    /// The exits fired during this call: which, and its values.
    static FIRED: RefCell<Vec<(usize, Vec<Value>)>> = const { RefCell::new(Vec::new()) };
}

/// `lineit_alloc`: N bytes for the C code, zeroed, aligned for anything,
/// until the call ends.
pub fn alloc(n: usize) -> *mut c_void {
    let mut b = vec![0u64; n.div_ceil(8).max(1)];
    let p = b.as_mut_ptr() as *mut c_void;
    ARENA.with(|a| a.borrow_mut().push(b));
    p
}

/// An exit of the C code fired, with its N VALUES.
///
/// # Safety
/// VALUES has N valid values.
pub unsafe fn fire(exit: usize, values: *const Lv, n: usize) {
    let vs = if values.is_null() || n == 0 { &[][..] } else { unsafe { std::slice::from_raw_parts(values, n) } };
    let got: Vec<Value> = vs.iter().map(|v| unsafe { value(v) }.unwrap_or(Value::Int(0))).collect();
    FIRED.with(|f| f.borrow_mut().push((exit, got)));
}

/// How a C code square takes and gives its values, by pin name (what
/// lineit-core read of its function).
pub struct Spec {
    pub inputs: &'static [&'static str],
    /// The variables `::NAME` it uses, and whether it sets them.
    pub shared: &'static [(&'static str, bool)],
    pub returns: &'static [&'static str],
    /// Its exits, with the pins of their values.
    pub exits: &'static [(&'static str, &'static [&'static str])],
    pub state: &'static [&'static str],
    /// Whether `then` fires when it returns.
    pub then: bool,
}

pub type CallFn = unsafe extern "C" fn(*const Lv, *mut Lv) -> i32;
pub type GetFn = unsafe extern "C" fn(*mut Lv);
pub type SetFn = unsafe extern "C" fn(*const Lv);

/// A square of a C function: its state is its own, put in the code's
/// variables before each call and taken back after it.
pub struct CSquare {
    spec: &'static Spec,
    call: CallFn,
    get: GetFn,
    set: SetFn,
    state: Vec<Value>,
}

impl CSquare {
    /// A square, whose state starts as START: what the code's variables
    /// had before anything ran (the first square of a library reads them;
    /// a library stays loaded from one run to the next, with what the last
    /// one left in them).
    pub fn new(
        spec: &'static Spec,
        call: CallFn,
        get: GetFn,
        set: SetFn,
        start: &'static OnceLock<Vec<Value>>,
    ) -> CSquare {
        let mut s = CSquare { spec, call, get, set, state: Vec::new() };
        s.state = start.get_or_init(|| s.read_state()).clone();
        s
    }

    fn read_state(&mut self) -> Vec<Value> {
        let mut st = vec![Lv::default(); self.spec.state.len()];
        unsafe { (self.get)(st.as_mut_ptr()) };
        let v = st.iter().map(|x| unsafe { value(x) }.unwrap_or(Value::Int(0))).collect();
        ARENA.with(|a| a.borrow_mut().clear());
        v
    }

    pub fn run(&mut self, inputs: &Values) -> Result<(Values, Vec<(String, Values)>), String> {
        let spec = self.spec;
        let mut keep = Keep::default();
        let get = |k: &str| inputs.get(k).ok_or_else(|| format!("no value for `{k}`"));
        let mut args = Vec::new();
        for i in spec.inputs {
            args.push(keep.lv(get(i)?)?);
        }
        for (g, _) in spec.shared {
            args.push(keep.lv(get(&format!("::{g}"))?)?);
        }
        let state: Vec<Lv> = self.state.iter().map(|v| keep.lv(v)).collect::<Result<_, _>>()?;
        let mut ret = vec![Lv::default(); spec.returns.len() + spec.shared.len()];
        FIRED.with(|f| f.borrow_mut().clear());
        unsafe {
            (self.set)(state.as_ptr());
            (self.call)(args.as_ptr(), ret.as_mut_ptr());
        }
        let mut set = Values::new();
        for (pin, v) in spec.returns.iter().zip(&ret) {
            set.insert(pin.to_string(), unsafe { value(v) }?);
        }
        for ((g, sets), v) in spec.shared.iter().zip(&ret[spec.returns.len()..]) {
            if *sets {
                set.insert(format!("::{g}"), unsafe { value(v) }?);
            }
        }
        let mut fired = Vec::new();
        for (exit, values) in FIRED.with(|f| std::mem::take(&mut *f.borrow_mut())) {
            let (name, pins) = spec.exits.get(exit).ok_or("C fired an exit it doesn't have")?;
            fired.push((name.to_string(), pins.iter().map(|p| p.to_string()).zip(values).collect()));
        }
        if spec.then {
            fired.push(("then".to_string(), Values::new()));
        }
        ARENA.with(|a| a.borrow_mut().clear());
        self.state = self.read_state();
        Ok((set, fired))
    }

    pub fn state(&self) -> Values {
        self.spec.state.iter().map(|s| s.to_string()).zip(self.state.iter().cloned()).collect()
    }

    pub fn set_state(&mut self, s: &Values) -> Result<(), String> {
        for (k, name) in self.spec.state.iter().enumerate() {
            if let Some(v) = s.get(*name) {
                self.state[k] = crate::native::coerce(v.clone(), &self.state[k].ty())?;
            }
        }
        Ok(())
    }
}

/// The type a C type name says: `int64_t`, `double`, `bool`, `str` (a
/// `const char *`), `list_T`, `map_T`, or a record's name.
pub fn c_name(t: &Type) -> String {
    match t {
        Type::Int => "int".into(),
        Type::Float => "float".into(),
        Type::Bool => "bool".into(),
        Type::Str => "str".into(),
        Type::List(t) => format!("list_{}", c_name(t)),
        Type::Map(t) => format!("map_{}", c_name(t)),
        Type::Record(r) => r.name.clone(),
        Type::Image => "image".into(),
    }
}

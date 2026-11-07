//! Rust modules in the interpreter: a package's `native/` crate, built as a
//! dynamic library the first time a canvas runs one of its modules (and
//! again when it changes: cargo knows), and called through the C ABI of
//! [`lineit_rt::native`].
//!
//! The crate depends on `lineit-rt` like any crate would (`lineit-rt =
//! "0.1"`); lineit builds it with the runtime it carries in its place, so
//! the library and the editor read each other's bytes the same way.

use std::collections::HashMap;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};

use lineit_rt::ext::{Ext, Ticked};
use lineit_rt::native::{self, Buf, CALL_EVAL, CALL_START, CALL_TICK, Values};
use lineit_rt::{Type, Value};

/// The `--config` that makes a crate use the runtime lineit carries.
pub fn patch_runtime(cmd: &mut Command, rt: &Path) {
    let rt = rt.to_string_lossy().replace('\\', "/");
    cmd.arg("--config").arg(format!("patch.crates-io.lineit-rt.path=\"{rt}\""));
}

/// The file of a library called NAME on this OS.
fn dylib_name(name: &str) -> String {
    let name = name.replace('-', "_");
    if cfg!(windows) {
        format!("{name}.dll")
    } else if cfg!(target_os = "macos") {
        format!("lib{name}.dylib")
    } else {
        format!("lib{name}.so")
    }
}

/// Builds the crate in CRATE_DIR, and says where its library is.
pub fn build(crate_dir: &Path, crate_name: &str, log: &mut dyn FnMut(&str)) -> Result<PathBuf, String> {
    let rt = crate::codegen::write_runtime()?;
    let target = crate::paths::cache_dir().join("native");
    let cargo = std::env::var_os("CARGO").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("cargo"));
    let mut cmd = Command::new(cargo);
    cmd.arg("build").arg("--release").arg("--lib").arg("--color=never");
    cmd.arg("--manifest-path").arg(crate_dir.join("Cargo.toml"));
    patch_runtime(&mut cmd, &rt);
    cmd.env("CARGO_TARGET_DIR", &target).env_remove("RUSTFLAGS").env_remove("CARGO_ENCODED_RUSTFLAGS");
    let out = cmd
        .output()
        .map_err(|e| format!("couldn't run cargo ({e}): Rust modules need Rust, from https://rustup.rs"))?;
    for line in String::from_utf8_lossy(&out.stderr).lines() {
        log(line);
    }
    if !out.status.success() {
        let tail: Vec<&str> = std::str::from_utf8(&out.stderr).unwrap_or("").lines().rev().take(20).collect();
        let tail: Vec<&str> = tail.into_iter().rev().collect();
        return Err(format!("cargo couldn't build {}:\n{}", crate_dir.display(), tail.join("\n")));
    }
    let lib = target.join("release").join(dylib_name(crate_name));
    if !lib.exists() {
        return Err(format!("{} isn't there: is the crate's crate-type [\"cdylib\", \"rlib\"]?", lib.display()));
    }
    Ok(lib)
}

type NewFn = unsafe extern "C" fn(*const u8, usize, *const u8, usize) -> *mut c_void;
type CallFn = unsafe extern "C" fn(*mut c_void, i32, *const u8, usize, *const u8, usize, *mut Buf) -> i32;
type FreeFn = unsafe extern "C" fn(*mut c_void);
type FreeBufFn = unsafe extern "C" fn(Buf);

struct Lib {
    // kept loaded while its functions are used
    _lib: libloading::Library,
    new: NewFn,
    call: CallFn,
    free: FreeFn,
    free_buf: FreeBufFn,
}

type LibKey = (PathBuf, Option<std::time::SystemTime>);

/// The libraries loaded, by file and when it was built: a rebuilt one is
/// loaded again, so a change to a Rust module shows on the next run. The
/// old ones stay loaded: unloading Rust code that may have left threads or
/// statics behind isn't safe.
fn libs() -> &'static Mutex<HashMap<LibKey, Arc<Lib>>> {
    static LIBS: OnceLock<Mutex<HashMap<LibKey, Arc<Lib>>>> = OnceLock::new();
    LIBS.get_or_init(Default::default)
}

fn load(path: &Path) -> Result<Arc<Lib>, String> {
    let mut libs = libs().lock().unwrap();
    let key = (path.to_path_buf(), std::fs::metadata(path).and_then(|m| m.modified()).ok());
    if let Some(l) = libs.get(&key) {
        return Ok(l.clone());
    }
    // a copy, so building it again while it's loaded works (Windows locks
    // a loaded file), named for this process: another editor may have its own
    let copy = path.with_extension(format!("{}-{}.loaded", std::process::id(), libs.len()));
    std::fs::copy(path, &copy).map_err(|e| format!("{}: {e}", copy.display()))?;
    let err = |e: libloading::Error| format!("{}: {e}", path.display());
    unsafe {
        let lib = libloading::Library::new(&copy).map_err(err)?;
        let abi: libloading::Symbol<unsafe extern "C" fn() -> u32> = lib
            .get(b"lineit_native_abi")
            .map_err(|_| format!("{}: no lineit_rt::export_modules! in it", path.display()))?;
        if abi() != native::ABI {
            return Err(format!(
                "{}: made for another lineit (ABI {}, this one is {})",
                path.display(),
                abi(),
                native::ABI
            ));
        }
        let new = *lib.get::<NewFn>(b"lineit_native_new").map_err(err)?;
        let call = *lib.get::<CallFn>(b"lineit_native_call").map_err(err)?;
        let free = *lib.get::<FreeFn>(b"lineit_native_free").map_err(err)?;
        let free_buf = *lib.get::<FreeBufFn>(b"lineit_native_free_buf").map_err(err)?;
        let l = Arc::new(Lib { _lib: lib, new, call, free, free_buf });
        libs.insert(key, l.clone());
        Ok(l)
    }
}

/// A module of a loaded library.
pub struct Dylib {
    lib: Arc<Lib>,
    ty: String,
    squares: Vec<*mut c_void>,
}

// the squares' states are only used by the thread that has the Dylib
unsafe impl Send for Dylib {}

impl Dylib {
    pub fn open(crate_dir: &Path, crate_name: &str, ty: &str) -> Result<Dylib, String> {
        let path = build(crate_dir, crate_name, &mut |_| {})?;
        Ok(Dylib { lib: load(&path)?, ty: ty.to_string(), squares: Vec::new() })
    }

    fn call(&mut self, inst: usize, what: i32, input: &str, inputs: &[(&str, Value)]) -> Result<Value, String> {
        let args: Values = inputs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
        let mut bytes = Vec::new();
        native::encode(&Value::Map(Arc::new(args)), &mut bytes);
        let mut out = Buf { ptr: std::ptr::null_mut(), len: 0, cap: 0 };
        let code = unsafe {
            (self.lib.call)(
                self.squares[inst],
                what,
                input.as_ptr(),
                input.len(),
                bytes.as_ptr(),
                bytes.len(),
                &mut out,
            )
        };
        let got = unsafe { std::slice::from_raw_parts(out.ptr, out.len) }.to_vec();
        unsafe { (self.lib.free_buf)(out) };
        if code != 0 {
            return Err(format!("{}: {}", self.ty, String::from_utf8_lossy(&got)));
        }
        native::decode(&mut got.as_slice())
    }
}

impl Drop for Dylib {
    fn drop(&mut self) {
        for s in self.squares.drain(..) {
            unsafe { (self.lib.free)(s) };
        }
    }
}

fn outputs(got: &Value, outs: &[(&str, &Type)], defaults: bool) -> Result<Vec<Option<Value>>, String> {
    let Value::Map(m) = got else { return Err("it gave back something that isn't its outputs".into()) };
    outs.iter()
        .map(|(n, t)| match m.get(*n) {
            Some(v) => native::coerce(v.clone(), t).map(Some).map_err(|e| format!("output `{n}`: {e}")),
            None => Ok(defaults.then(|| Value::default_of(t))),
        })
        .collect()
}

impl Ext for Dylib {
    fn instance(&mut self, params: &[(&str, Value)]) -> Result<usize, String> {
        let p: Values = params.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
        let mut bytes = Vec::new();
        native::encode(&Value::Map(Arc::new(p)), &mut bytes);
        let s = unsafe { (self.lib.new)(self.ty.as_ptr(), self.ty.len(), bytes.as_ptr(), bytes.len()) };
        if s.is_null() {
            return Err(format!("the library has no module `{}` (or it panicked making one)", self.ty));
        }
        self.squares.push(s);
        Ok(self.squares.len() - 1)
    }

    fn eval(&mut self, inst: usize, inputs: &[(&str, Value)], outs: &[(&str, &Type)]) -> Result<Vec<Value>, String> {
        let got = self.call(inst, CALL_EVAL, "", inputs)?;
        Ok(outputs(&got, outs, true)?.into_iter().map(|v| v.unwrap_or(Value::Int(0))).collect())
    }

    fn tick(
        &mut self,
        inst: usize,
        input: Option<&str>,
        inputs: &[(&str, Value)],
        outs: &[(&str, &Type)],
    ) -> Result<Ticked, String> {
        let got = match input {
            Some(i) => self.call(inst, CALL_TICK, i, inputs)?,
            None => self.call(inst, CALL_START, "", inputs)?,
        };
        let l = got.list();
        let (Some(set), Some(fired)) = (l.first(), l.get(1)) else {
            return Err("it gave back something that isn't a tick".into());
        };
        Ok((outputs(set, outs, false)?, fired.list().iter().map(|v| v.str().to_string()).collect()))
    }
}

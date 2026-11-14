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

use lineit_rt::ext::{Ext, Run, Ticked};
use lineit_rt::native::{self, Buf, CALL_EVAL, CALL_RUN, CALL_SET_STATE, CALL_START, CALL_STATE, CALL_TICK, Values};
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
    let run = |offline: bool| {
        let mut cmd = Command::new(&cargo);
        cmd.arg("build").arg("--release").arg("--lib").arg("--color=never");
        cmd.arg("--manifest-path").arg(crate_dir.join("Cargo.toml"));
        if offline {
            cmd.arg("--offline");
        }
        patch_runtime(&mut cmd, &rt);
        cmd.env("CARGO_TARGET_DIR", &target).env_remove("RUSTFLAGS").env_remove("CARGO_ENCODED_RUSTFLAGS");
        cmd.output().map_err(|e| format!("couldn't run cargo ({e}): Rust modules need Rust, from https://rustup.rs"))
    };
    let mut out = run(false)?;
    if !out.status.success() {
        // no network: what cargo has already may be enough
        let offline = run(true)?;
        if offline.status.success() {
            out = offline;
        }
    }
    for line in String::from_utf8_lossy(&out.stderr).lines() {
        log(line);
    }
    if !out.status.success() {
        let said = compiler_said(std::str::from_utf8(&out.stderr).unwrap_or(""));
        if crate_name.starts_with(crate::registry::code::rust::CRATE_PREFIX) {
            return Err(format!("the code doesn't compile:\n{said}"));
        }
        return Err(format!("cargo couldn't build {}:\n{said}", crate_dir.display()));
    }
    let lib = target.join("release").join(dylib_name(crate_name));
    if !lib.exists() {
        return Err(format!("{} isn't there: is the crate's crate-type [\"cdylib\", \"rlib\"]?", lib.display()));
    }
    Ok(lib)
}

/// What the compilers said, from cargo's STDERR, from the first error
/// on (at most 40 lines): without cargo's own lines, nor what it repeats
/// of the build scripts (a C compiler's errors come as `cargo:warning=`).
pub fn compiler_said(stderr: &str) -> String {
    let mut kept = Vec::new();
    for line in stderr.lines() {
        let t = line.trim_start();
        if let Some(c) = t.strip_prefix("cargo:warning=") {
            kept.push(c);
            continue;
        }
        let words: Vec<&str> = t.split_whitespace().collect();
        let noise = t.starts_with("cargo:")
            || t.starts_with("--- std")
            || t.starts_with("Caused by:")
            || t.starts_with("process didn't exit successfully")
            || t.starts_with("error: failed to run custom build command")
            || t.starts_with("error occurred in cc-rs")
            || t.starts_with("warning: spurious network error")
            // cargo repeating a build script's warnings: `warning: NAME@VERSION: ...`
            || (t.starts_with("warning: ") && words.get(1).is_some_and(|w| w.contains('@') && w.ends_with(':')))
            || matches!(
                words.first(),
                Some(&("Compiling" | "Updating" | "Running" | "Blocking" | "Locking" | "Adding" | "Downloading" | "Downloaded" | "Finished"))
            )
            // a build script's environment: `CFLAGS = None`
            || (words.len() == 3 && words[1] == "=" && (words[2] == "None" || words[2].starts_with("Some(")));
        if !noise {
            kept.push(line);
        }
    }
    let from = kept.iter().position(|l| l.contains("error")).unwrap_or(0);
    let mut kept: Vec<&str> = kept[from..].iter().copied().take(40).collect();
    while kept.last().is_some_and(|l| l.trim().is_empty()) {
        kept.pop();
    }
    kept.join("\n")
}

#[cfg(test)]
#[test]
fn compiler_lines() {
    let c = "   Compiling lineit-code-1 v0.1.0 (/x)\nwarning: lineit-code-1@0.1.0: src/bad.c:2:16: error: 'y' undeclared\nerror: failed to run custom build command for `x`\n\nCaused by:\n  process didn't exit successfully: `x` (exit status: 1)\n  CC_x86_64-unknown-linux-gnu = None\n  cargo:warning=src/bad.c:2:16: error: 'y' undeclared\n  cargo:warning=    2 |     return x * y;\n  --- stderr\n";
    assert_eq!(compiler_said(c), "src/bad.c:2:16: error: 'y' undeclared\n    2 |     return x * y;");
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

/// The library at PATH, of crate CRATE_NAME (its functions' names end in it).
/// Removes the copies of libraries (`*.loaded`) next to PATH that other
/// processes made: those of editors that ended, and those another one has
/// loaded where that's allowed (Linux, macOS: it keeps its own; Windows
/// refuses, and the copy stays).
fn remove_old_copies(path: &Path) {
    let Some(dir) = path.parent() else { return };
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mine = format!(".{}-", std::process::id());
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.ends_with(".loaded") && !name.contains(&mine) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

fn load(path: &Path, crate_name: &str) -> Result<Arc<Lib>, String> {
    let mut libs = libs().lock().unwrap();
    let key = (path.to_path_buf(), std::fs::metadata(path).and_then(|m| m.modified()).ok());
    if let Some(l) = libs.get(&key) {
        return Ok(l.clone());
    }
    // a copy, so building it again while it's loaded works (Windows locks
    // a loaded file), named for this process: another editor may have its own
    if libs.is_empty() {
        remove_old_copies(path);
    }
    let copy = path.with_extension(format!("{}-{}.loaded", std::process::id(), libs.len()));
    std::fs::copy(path, &copy).map_err(|e| format!("{}: {e}", copy.display()))?;
    let err = |e: libloading::Error| format!("{}: {e}", path.display());
    let sym = |f: &str| format!("lineit_native_{f}_{}", crate_name.replace('-', "_"));
    unsafe {
        let lib = libloading::Library::new(&copy).map_err(err)?;
        let abi: libloading::Symbol<unsafe extern "C" fn() -> u32> = lib
            .get(sym("abi").as_bytes())
            .map_err(|_| format!("{}: no lineit_rt::export_modules! in it", path.display()))?;
        if abi() != native::ABI {
            return Err(format!(
                "{}: made for another lineit (ABI {}, this one is {})",
                path.display(),
                abi(),
                native::ABI
            ));
        }
        let new = *lib.get::<NewFn>(sym("new").as_bytes()).map_err(err)?;
        let call = *lib.get::<CallFn>(sym("call").as_bytes()).map_err(err)?;
        let free = *lib.get::<FreeFn>(sym("free").as_bytes()).map_err(err)?;
        let free_buf = *lib.get::<FreeBufFn>(sym("free_buf").as_bytes()).map_err(err)?;
        let l = Arc::new(Lib { _lib: lib, new, call, free, free_buf });
        libs.insert(key, l.clone());
        Ok(l)
    }
}

/// The program that runs libraries apart (the `lineit` binary, which has
/// `native-host`), if it's set: then a library's crash ends its run, not the
/// editor.
static HOST: OnceLock<PathBuf> = OnceLock::new();

/// Libraries run in processes of their own, by EXE (`lineit native-host`).
pub fn run_apart(exe: PathBuf) {
    let _ = HOST.set(exe);
}

/// Where a module's library runs: here, or in a process of its own.
enum Backend {
    Here { lib: Arc<Lib>, squares: Vec<*mut c_void> },
    Apart(Helper),
}

// the squares' states are only used by the thread that has the Dylib
unsafe impl Send for Backend {}

impl Backend {
    /// A square of module TY, with its PARAMS as bytes.
    fn new_square(&mut self, ty: &str, params: &[u8]) -> Result<usize, String> {
        match self {
            Backend::Here { lib, squares } => {
                let s = unsafe { (lib.new)(ty.as_ptr(), ty.len(), params.as_ptr(), params.len()) };
                if s.is_null() {
                    return Err(format!("the library has no module `{ty}` (or it panicked making one)"));
                }
                squares.push(s);
                Ok(squares.len() - 1)
            }
            Backend::Apart(h) => h.new_square(ty, params),
        }
    }

    /// Calls square SQ (WHAT: eval, run...): the library's code, and its bytes.
    fn call(&mut self, sq: usize, what: i32, input: &str, args: &[u8]) -> Result<(i32, Vec<u8>), String> {
        match self {
            Backend::Here { lib, squares } => Ok(unsafe { raw_call(lib, squares[sq], what, input.as_bytes(), args) }),
            Backend::Apart(h) => h.call(sq, what, input, args),
        }
    }
}

unsafe fn raw_call(lib: &Lib, sq: *mut c_void, what: i32, input: &[u8], args: &[u8]) -> (i32, Vec<u8>) {
    let mut out = Buf { ptr: std::ptr::null_mut(), len: 0, cap: 0 };
    let code = unsafe { (lib.call)(sq, what, input.as_ptr(), input.len(), args.as_ptr(), args.len(), &mut out) };
    let got = unsafe { std::slice::from_raw_parts(out.ptr, out.len) }.to_vec();
    unsafe { (lib.free_buf)(out) };
    (code, got)
}

/// A library in a process of its own (`lineit native-host LIB CRATE ADDRESS
/// TOKEN`), which connects to ADDRESS (see [`Stream`]) and sends TOKEN (so
/// it's this one), then is asked in frames of bytes: `0, ty, params` makes
/// a square (the answer: 1 and its number, or 0 and why not); `1, square,
/// what, input, args` calls one (the answer: its code and bytes). Its
/// stdout and stderr are the library's to write to.
struct Helper {
    /// Shared with what stops it, which can kill it while a call waits.
    child: Arc<Mutex<std::process::Child>>,
    stderr: Option<Arc<Mutex<Vec<String>>>>,
    to: Stream,
    from: std::io::BufReader<Stream>,
}

fn put(out: &mut Vec<u8>, b: &[u8]) {
    out.extend_from_slice(&(b.len() as u64).to_le_bytes());
    out.extend_from_slice(b);
}

fn take_u64(r: &mut impl std::io::Read) -> std::io::Result<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

fn take_bytes(r: &mut impl std::io::Read) -> std::io::Result<Vec<u8>> {
    let n = take_u64(r)? as usize;
    let mut b = vec![0u8; n];
    r.read_exact(&mut b)?;
    Ok(b)
}

/// How lineit and a helper talk: a Unix socket where there are (the
/// fastest), TCP on this machine where not (Windows).
enum Stream {
    Tcp(std::net::TcpStream),
    #[cfg(unix)]
    Unix(std::os::unix::net::UnixStream),
}

impl Stream {
    fn try_clone(&self) -> std::io::Result<Stream> {
        Ok(match self {
            Stream::Tcp(s) => Stream::Tcp(s.try_clone()?),
            #[cfg(unix)]
            Stream::Unix(s) => Stream::Unix(s.try_clone()?),
        })
    }

    fn set_read_timeout(&self, t: Option<std::time::Duration>) -> std::io::Result<()> {
        match self {
            Stream::Tcp(s) => s.set_read_timeout(t),
            #[cfg(unix)]
            Stream::Unix(s) => s.set_read_timeout(t),
        }
    }

    /// Connects to ADDRESS, as [`Listener::listen`] gives it.
    fn connect(address: &str) -> std::io::Result<Stream> {
        #[cfg(unix)]
        if let Some(path) = address.strip_prefix("unix:") {
            return Ok(Stream::Unix(std::os::unix::net::UnixStream::connect(path)?));
        }
        let port: u16 =
            address.strip_prefix("tcp:").and_then(|p| p.parse().ok()).ok_or(std::io::ErrorKind::InvalidInput)?;
        let s = std::net::TcpStream::connect(("127.0.0.1", port))?;
        let _ = s.set_nodelay(true);
        Ok(Stream::Tcp(s))
    }
}

impl std::io::Read for Stream {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Stream::Tcp(s) => s.read(b),
            #[cfg(unix)]
            Stream::Unix(s) => s.read(b),
        }
    }
}

impl std::io::Write for Stream {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        match self {
            Stream::Tcp(s) => s.write(b),
            #[cfg(unix)]
            Stream::Unix(s) => s.write(b),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Stream::Tcp(s) => s.flush(),
            #[cfg(unix)]
            Stream::Unix(s) => s.flush(),
        }
    }
}

enum Listener {
    Tcp(std::net::TcpListener),
    /// And its file, removed when it's done.
    #[cfg(unix)]
    Unix(std::os::unix::net::UnixListener, PathBuf),
}

impl Listener {
    /// A listener, and its address for the helper (`unix:PATH`, `tcp:PORT`).
    fn listen() -> std::io::Result<(Listener, String)> {
        #[cfg(unix)]
        {
            static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("lineit-{}-{n}.sock", std::process::id()));
            let _ = std::fs::remove_file(&path);
            if let Ok(l) = std::os::unix::net::UnixListener::bind(&path) {
                let address = format!("unix:{}", path.display());
                return Ok((Listener::Unix(l, path), address));
            }
        }
        let l = std::net::TcpListener::bind("127.0.0.1:0")?;
        let port = l.local_addr()?.port();
        Ok((Listener::Tcp(l), format!("tcp:{port}")))
    }

    fn set_nonblocking(&self) -> std::io::Result<()> {
        match self {
            Listener::Tcp(l) => l.set_nonblocking(true),
            #[cfg(unix)]
            Listener::Unix(l, _) => l.set_nonblocking(true),
        }
    }

    fn accept(&self) -> std::io::Result<Stream> {
        Ok(match self {
            Listener::Tcp(l) => {
                let (s, _) = l.accept()?;
                s.set_nonblocking(false)?;
                let _ = s.set_nodelay(true);
                Stream::Tcp(s)
            }
            #[cfg(unix)]
            Listener::Unix(l, _) => {
                let (s, _) = l.accept()?;
                s.set_nonblocking(false)?;
                Stream::Unix(s)
            }
        })
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Listener::Unix(_, path) = self {
            let _ = std::fs::remove_file(path);
        }
    }
}

impl Helper {
    fn start(exe: &Path, lib: &Path, crate_name: &str) -> Result<Helper, String> {
        use std::hash::{BuildHasher, Hasher};
        let (listener, address) =
            Listener::listen().map_err(|e| format!("couldn't listen for the code's process ({e})"))?;
        let token = std::collections::hash_map::RandomState::new().build_hasher().finish();
        let captured = lineit_rt::ext::captures_stderr();
        let out = || if captured { std::process::Stdio::piped() } else { std::process::Stdio::inherit() };
        let mut child = Command::new(exe)
            .arg("native-host")
            .arg(lib)
            .arg(crate_name)
            .arg(&address)
            .arg(token.to_string())
            .stdin(std::process::Stdio::null())
            .stdout(out())
            .stderr(out())
            .spawn()
            .map_err(|e| format!("couldn't start {} ({e})", exe.display()))?;
        // what it writes, both ways, is the library's: kept for the output
        let stderr = lineit_rt::ext::stderr_of(&mut child);
        if let (Some(lines), Some(o)) = (&stderr, child.stdout.take()) {
            lineit_rt::ext::keep_lines(o, lines.clone());
        }
        // it, and only it (it says the token), connects
        listener.set_nonblocking().map_err(|e| e.to_string())?;
        let started = std::time::Instant::now();
        let stream = loop {
            match listener.accept() {
                Ok(mut s) => {
                    let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(5)));
                    if take_u64(&mut s).ok() == Some(token) {
                        let _ = s.set_read_timeout(None);
                        break s;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if let Ok(Some(status)) = child.try_wait() {
                        return Err(format!("the code's process ended before it started ({status})"));
                    }
                    if started.elapsed() > std::time::Duration::from_secs(30) {
                        let _ = child.kill();
                        return Err("the code's process didn't start".into());
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(e) => return Err(format!("the code's process couldn't connect ({e})")),
            }
        };
        let from = std::io::BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
        Ok(Helper { child: Arc::new(Mutex::new(child)), stderr, to: stream, from })
    }

    /// What went wrong with the process: it crashed, most likely.
    fn gone(&mut self, e: std::io::Error) -> String {
        let mut child = self.child.lock().unwrap_or_else(|e| e.into_inner());
        let _ = child.kill();
        match child.wait() {
            Ok(s) if !s.success() => format!("the code crashed ({s})"),
            _ => format!("the code's process ended ({e})"),
        }
    }

    fn ask(&mut self, frame: &[u8]) -> Result<(u64, Vec<u8>), String> {
        use std::io::Write;
        let sent = self.to.write_all(frame).and_then(|_| self.to.flush());
        if let Err(e) = sent {
            return Err(self.gone(e));
        }
        let got = take_u64(&mut self.from).and_then(|n| Ok((n, take_bytes(&mut self.from)?)));
        got.map_err(|e| self.gone(e))
    }

    fn new_square(&mut self, ty: &str, params: &[u8]) -> Result<usize, String> {
        let mut f = vec![0u8];
        put(&mut f, ty.as_bytes());
        put(&mut f, params);
        match self.ask(&f)? {
            (1, b) => Ok(u64::from_le_bytes(b.try_into().map_err(|_| "a bad answer")?) as usize),
            (_, why) => Err(String::from_utf8_lossy(&why).into_owned()),
        }
    }

    fn call(&mut self, sq: usize, what: i32, input: &str, args: &[u8]) -> Result<(i32, Vec<u8>), String> {
        let mut f = vec![1u8];
        f.extend_from_slice(&(sq as u64).to_le_bytes());
        f.extend_from_slice(&(what as i64 as u64).to_le_bytes());
        put(&mut f, input.as_bytes());
        put(&mut f, args);
        let (code, bytes) = self.ask(&f)?;
        Ok((code as i64 as i32, bytes))
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        let mut child = self.child.lock().unwrap_or_else(|e| e.into_inner());
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// `lineit native-host LIB CRATE ADDRESS TOKEN`: runs the library at LIB
/// (of crate CRATE) for the process that started this one, which it reaches
/// at ADDRESS (`unix:PATH`, `tcp:PORT`), until it goes (see [`Helper`]).
pub fn serve(lib: &Path, crate_name: &str, address: &str, token: u64) -> Result<(), String> {
    use std::io::{Read, Write};
    let stream = Stream::connect(address).map_err(|e| format!("couldn't connect to lineit at {address} ({e})"))?;
    let mut out = std::io::BufWriter::new(stream.try_clone().map_err(|e| e.to_string())?);
    out.write_all(&token.to_le_bytes()).and_then(|_| out.flush()).map_err(|e| e.to_string())?;
    let lib = load(lib, crate_name)?;
    let mut squares: Vec<*mut c_void> = Vec::new();
    let mut input = std::io::BufReader::new(stream);
    fn answer(out: &mut impl Write, n: u64, b: &[u8]) -> std::io::Result<()> {
        out.write_all(&n.to_le_bytes())?;
        out.write_all(&(b.len() as u64).to_le_bytes())?;
        out.write_all(b)?;
        out.flush()
    }
    loop {
        let mut op = [0u8];
        if input.read_exact(&mut op).is_err() {
            return Ok(());
        }
        let r = match op[0] {
            0 => {
                let ty = take_bytes(&mut input).map_err(|e| e.to_string())?;
                let params = take_bytes(&mut input).map_err(|e| e.to_string())?;
                let s = unsafe { (lib.new)(ty.as_ptr(), ty.len(), params.as_ptr(), params.len()) };
                if s.is_null() {
                    let why = format!(
                        "the library has no module `{}` (or it panicked making one)",
                        String::from_utf8_lossy(&ty)
                    );
                    answer(&mut out, 0, why.as_bytes())
                } else {
                    squares.push(s);
                    answer(&mut out, 1, &((squares.len() - 1) as u64).to_le_bytes())
                }
            }
            1 => {
                let sq = take_u64(&mut input).map_err(|e| e.to_string())? as usize;
                let what = take_u64(&mut input).map_err(|e| e.to_string())? as i64 as i32;
                let inp = take_bytes(&mut input).map_err(|e| e.to_string())?;
                let args = take_bytes(&mut input).map_err(|e| e.to_string())?;
                let Some(s) = squares.get(sq) else { return Err(format!("no square {sq}")) };
                let (code, bytes) = unsafe { raw_call(&lib, *s, what, &inp, &args) };
                answer(&mut out, code as i64 as u64, &bytes)
            }
            o => return Err(format!("no request {o}")),
        };
        r.map_err(|e| e.to_string())?;
    }
}

/// A module of a library.
pub struct Dylib {
    backend: Backend,
    ty: String,
}

impl Dylib {
    pub fn open(crate_dir: &Path, crate_name: &str, ty: &str) -> Result<Dylib, String> {
        let path = build(crate_dir, crate_name, &mut |_| {})?;
        let backend = match HOST.get() {
            Some(exe) => Backend::Apart(Helper::start(exe, &path, crate_name)?),
            None => Backend::Here { lib: load(&path, crate_name)?, squares: Vec::new() },
        };
        Ok(Dylib { backend, ty: ty.to_string() })
    }

    fn call(&mut self, inst: usize, what: i32, input: &str, inputs: &[(&str, Value)]) -> Result<Value, String> {
        let args: Values = inputs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
        let mut bytes = Vec::new();
        native::encode(&Value::Map(Arc::new(args)), &mut bytes);
        let (code, got) = self.backend.call(inst, what, input, &bytes)?;
        if code != 0 {
            return Err(format!("{}: {}", self.ty, String::from_utf8_lossy(&got)));
        }
        // what it gave back, and what it printed
        let both = native::decode(&mut got.as_slice())?;
        let Value::List(l) = both else { return Err(format!("{}: a bad answer", self.ty)) };
        for line in l.get(1).map(|p| p.list().to_vec()).unwrap_or_default() {
            lineit_rt::ext::print(line.str());
        }
        l.first().cloned().ok_or_else(|| format!("{}: a bad answer", self.ty))
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        if let Backend::Here { lib, squares } = self {
            for s in squares.drain(..) {
                unsafe { (lib.free)(s) };
            }
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
    fn handles(&mut self) -> lineit_rt::ext::Handles {
        let Backend::Apart(h) = &self.backend else { return lineit_rt::ext::Handles::default() };
        let child = h.child.clone();
        lineit_rt::ext::Handles {
            kill: Some(Box::new(move || {
                let _ = child.lock().unwrap_or_else(|e| e.into_inner()).kill();
            })),
            stderr: h.stderr.clone(),
        }
    }

    fn instance(&mut self, params: &[(&str, Value)]) -> Result<usize, String> {
        let p: Values = params.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
        let mut bytes = Vec::new();
        native::encode(&Value::Map(Arc::new(p)), &mut bytes);
        self.backend.new_square(&self.ty, &bytes)
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

    fn run(
        &mut self,
        inst: usize,
        input: Option<&str>,
        inputs: &[(&str, Value)],
        outs: &[(&str, &Type)],
    ) -> Result<Run, String> {
        let Some(input) = input else {
            let (set, fire) = self.tick(inst, None, inputs, outs)?;
            return Ok(Run { set, fire: fire.into_iter().map(|f| (f, Vec::new())).collect() });
        };
        let got = self.call(inst, CALL_RUN, input, inputs)?;
        let (set, fire) = native::run_from_value(&got)?;
        native::run_of(set, fire, outs)
    }

    fn state(&mut self, inst: usize) -> Result<Vec<(String, Value)>, String> {
        match self.call(inst, CALL_STATE, "", &[])? {
            Value::Map(m) => Ok(m.iter().map(|(k, v)| (k.clone(), v.clone())).collect()),
            _ => Err("it gave back something that isn't a state".into()),
        }
    }

    fn set_state(&mut self, inst: usize, state: &[(&str, Value)]) -> Result<(), String> {
        self.call(inst, CALL_SET_STATE, "", state).map(|_| ())
    }
}

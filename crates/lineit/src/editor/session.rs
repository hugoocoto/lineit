//! When the lineit binary is rebuilt, the editor hands over to the new one,
//! as the C version did: it writes everything to a session file (every
//! tab's canvas, file, what was saved, selection, undo and redo, and
//! output, which tab is shown, and the window), starts the new binary with
//! `--restore` on it, and waits for it to say it's drawing. Then this one
//! closes and the new one carries on where it left. If the new one fails
//! or doesn't answer, this one stays, and tries again when the binary
//! changes again. Canvases that were running are run again (their
//! variables start over).
//!
//! The new one says it's drawing by making a file (`LINEIT_READY`), which
//! works the same on every OS.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// How long the old one waits for the new one.
const HANDOVER_TIMEOUT: Duration = Duration::from_secs(15);

/// Frames the new one draws before it says it's ready.
const READY_FRAMES: u32 = 3;

/// What a session has besides the tabs.
pub struct Window {
    pub pos: Option<[f32; 2]>,
    pub size: Option<[f32; 2]>,
}

/// The binary this editor runs, and when it last changed.
pub struct Exe {
    path: PathBuf,
    mtime: Option<SystemTime>,
}

fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

impl Exe {
    pub fn current() -> Option<Exe> {
        let path = std::env::current_exe().ok()?;
        Some(Exe { mtime: mtime(&path), path })
    }

    /// Whether it was rebuilt, and is done being written (unchanged for a
    /// second). A rebuild is only reported once.
    pub fn rebuilt(&mut self) -> bool {
        let now = mtime(&self.path);
        if now.is_none() || now == self.mtime {
            return false;
        }
        let settled = now.and_then(|t| t.elapsed().ok()).is_some_and(|age| age > Duration::from_secs(1));
        if settled {
            self.mtime = now;
        }
        settled
    }

    /// Writes SESSION to a file and starts the new binary on it. True once
    /// it's drawing: then this one has to close.
    pub fn hand_over(&self, session: &str) -> Result<(), String> {
        let tmp = std::env::temp_dir();
        let id = format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
        );
        let file = tmp.join(format!("lineit-session-{id}.lua"));
        let ready = tmp.join(format!("lineit-session-{id}.ready"));
        std::fs::write(&file, session).map_err(|e| format!("{}: {e}", file.display()))?;
        let mut child =
            match lineit_rt::ext::command(&self.path).arg("--restore").arg(&file).env("LINEIT_READY", &ready).spawn() {
                Ok(c) => c,
                Err(e) => {
                    let _ = std::fs::remove_file(&file);
                    return Err(format!("couldn't start it: {e}"));
                }
            };
        let started = Instant::now();
        loop {
            if ready.exists() {
                let _ = std::fs::remove_file(&ready);
                return Ok(());
            }
            if let Ok(Some(status)) = child.try_wait() {
                let _ = std::fs::remove_file(&file);
                return Err(format!("it quit ({status})"));
            }
            if started.elapsed() > HANDOVER_TIMEOUT {
                let _ = child.kill();
                let _ = child.wait();
                let _ = std::fs::remove_file(&file);
                return Err("it didn't start drawing in time".into());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// In the new one: says it's drawing, once it has drawn a few frames.
#[derive(Default)]
pub struct Ready {
    file: Option<PathBuf>,
    frames: u32,
}

impl Ready {
    pub fn from_env() -> Ready {
        let file = std::env::var_os("LINEIT_READY").map(PathBuf::from);
        // its own children (a rebuilt one) get their own
        // SAFETY: called at start, before the editor starts any thread
        unsafe { std::env::remove_var("LINEIT_READY") };
        Ready { file, frames: 0 }
    }

    pub fn frame(&mut self) {
        if self.file.is_none() {
            return;
        }
        self.frames += 1;
        if self.frames >= READY_FRAMES
            && let Some(f) = self.file.take()
        {
            let _ = std::fs::write(f, "ready");
        }
    }
}

/// A session: the tabs (each as [`super::tab::Tab::session`] writes it),
/// which one is shown, the output panel, and the window.
pub fn write(tabs: &[String], active: usize, show_output: bool, window: &Window) -> String {
    let mut s = String::from("-- lineit session, see --restore\nreturn {\n");
    s.push_str(&format!("        active = {},\n        show_output = {show_output},\n", active + 1));
    if let Some([x, y]) = window.pos {
        s.push_str(&format!("        x = {x}, y = {y},\n"));
    }
    if let Some([w, h]) = window.size {
        s.push_str(&format!("        width = {w}, height = {h},\n"));
    }
    s.push_str("        tabs = {\n");
    for t in tabs {
        s.push_str(&format!("                {t},\n"));
    }
    s.push_str("        },\n}\n");
    s
}

/// What a session file has, read in the sandbox canvases are read in.
pub struct Session {
    pub source: String,
    pub window: Window,
}

pub fn read(path: &Path) -> Result<Session, String> {
    let source = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    // read once: the new one doesn't leave it behind
    let _ = std::fs::remove_file(path);
    let window = lineit_core::file::with_data(&source, "the session", |t| {
        let num = |k: &str| t.get::<Option<f32>>(k).ok().flatten();
        Ok(Window {
            pos: num("x").zip(num("y")).map(|(x, y)| [x, y]),
            size: num("width").zip(num("height")).map(|(w, h)| [w, h]),
        })
    })?;
    Ok(Session { source, window })
}

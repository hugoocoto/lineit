//! One editor: `lineit FILE` with an editor already running gives it FILE,
//! to open in a tab, and quits (`--new` starts another editor instead).
//!
//! The editor listens on a port of 127.0.0.1, and writes it, with a token,
//! to `editor.addr` in the cache folder, readable by its user only: a
//! request without the token is ignored, so another user of the machine
//! can't make it open files. It works the same on every OS. An editor that
//! takes over from an older one (a rebuild, see session.rs) writes its own,
//! so requests go to the new one.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui;

fn addr_file() -> PathBuf {
    lineit_core::paths::cache_dir().join("editor.addr")
}

/// Gives PATHS (none: a new tab) to the editor running, if there's one.
/// True if it took them.
pub fn send(paths: &[PathBuf]) -> bool {
    let Ok(text) = std::fs::read_to_string(addr_file()) else { return false };
    let Some((port, token)) = text.trim().split_once(' ') else { return false };
    let Ok(addr) = format!("127.0.0.1:{port}").parse() else { return false };
    let Ok(mut s) = TcpStream::connect_timeout(&addr, Duration::from_millis(500)) else { return false };
    let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
    let mut msg = format!("{token}\n");
    for p in paths {
        let abs = std::fs::canonicalize(p).unwrap_or_else(|_| std::env::current_dir().unwrap_or_default().join(p));
        msg.push_str(&format!("{}\n", abs.display()));
    }
    msg.push('\n');
    if s.write_all(msg.as_bytes()).is_err() {
        return false;
    }
    let mut answer = String::new();
    BufReader::new(s).read_line(&mut answer).is_ok() && answer.trim() == "ok"
}

/// What other `lineit` commands asked this editor to open: paths, and None
/// for a new tab.
pub type Inbox = Arc<Mutex<Vec<Option<PathBuf>>>>;

fn token() -> String {
    // the time and the process, mixed: not guessable from outside, and no
    // crate for randomness
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let local = 0u8;
    let mut h: u64 = 0xcbf29ce484222325;
    for b in t
        .to_le_bytes()
        .iter()
        .chain(&std::process::id().to_le_bytes())
        .chain(&(&local as *const u8 as usize).to_le_bytes())
    {
        h = (h ^ *b as u64).wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}{:016x}", h.rotate_left(29) ^ t as u64)
}

/// An editor listening. When it goes, its address goes, unless another
/// editor wrote its own since.
pub struct Single {
    pub inbox: Inbox,
    token: String,
}

impl Drop for Single {
    fn drop(&mut self) {
        if std::fs::read_to_string(addr_file()).is_ok_and(|t| t.contains(&self.token)) {
            let _ = std::fs::remove_file(addr_file());
        }
    }
}

/// Starts listening, if it can. What's sent goes to its inbox, and the
/// editor is woken up to open it.
pub fn listen(ctx: egui::Context) -> Option<Single> {
    let listener = TcpListener::bind("127.0.0.1:0").ok()?;
    let port = listener.local_addr().ok()?.port();
    let token = token();
    let file = addr_file();
    std::fs::create_dir_all(file.parent()?).ok()?;
    let tmp = file.with_extension("tmp");
    std::fs::write(&tmp, format!("{port} {token}\n")).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, &file).ok()?;
    let inbox: Inbox = Arc::default();
    let i = inbox.clone();
    let mine = token.clone();
    std::thread::Builder::new()
        .name("lineit single".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let mut lines = BufReader::new(&stream).lines().map_while(Result::ok);
                if lines.next().as_deref() != Some(token.as_str()) {
                    continue;
                }
                let paths: Vec<String> = lines.take_while(|l| !l.is_empty()).collect();
                let mut inbox = i.lock().unwrap();
                if paths.is_empty() {
                    inbox.push(None);
                }
                inbox.extend(paths.into_iter().map(|p| Some(PathBuf::from(p))));
                drop(inbox);
                let _ = (&stream).write_all(b"ok\n");
                ctx.request_repaint();
            }
        })
        .ok()?;
    Some(Single { inbox, token: mine })
}

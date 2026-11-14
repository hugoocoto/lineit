//! The system's file dialogs (Open, Save as), through `rfd`. They run on a
//! thread of their own: the editor goes on drawing while one is open, and
//! gets the answer on a channel. The dialog is put over the editor's window.
//!
//! rfd says "nothing" both for Cancel and for a dialog it couldn't show, so
//! whether one can be shown is checked before ([`available`]), and where it
//! can't, the editor asks for the path in its own prompt (`App::ask`).

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

/// What to ask for: set by `App::ask`, opened by `ui` (it has the window).
pub struct Request {
    /// The id of the tab it's asked from: a save goes to it, not to the tab shown by then.
    pub tab: u64,
    pub save: bool,
    /// Where the dialog starts, and the name it proposes to save as.
    pub dir: PathBuf,
    pub name: String,
}

/// A dialog open: what it's for, and the answer when it comes.
pub struct Dialog {
    pub tab: u64,
    pub save: bool,
    rx: Receiver<Option<PathBuf>>,
}

pub enum Answer {
    Waiting,
    Chosen(PathBuf),
    Cancelled,
    /// The thread of the dialog ended without an answer (it panicked).
    Failed,
}

impl Dialog {
    /// Opens a dialog over WINDOW.
    pub fn open(ctx: &eframe::egui::Context, window: &eframe::Frame, r: Request) -> Dialog {
        let (tx, rx) = channel();
        let mut d =
            rfd::FileDialog::new().set_directory(&r.dir).set_parent(window).add_filter("lineit canvas", &["lua"]);
        d = if r.save {
            d.set_title("Save as").set_file_name(r.name)
        } else {
            d.set_title("Open").add_filter("All files", &["*"])
        };
        let save = r.save;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let answer = if save { d.save_file() } else { d.pick_file() };
            let _ = tx.send(answer);
            ctx.request_repaint();
        });
        Dialog { tab: r.tab, save, rx }
    }

    pub fn poll(&self) -> Answer {
        use std::sync::mpsc::TryRecvError::{Disconnected, Empty};
        match self.rx.try_recv() {
            Ok(Some(p)) => Answer::Chosen(p),
            Ok(None) => Answer::Cancelled,
            Err(Disconnected) => Answer::Failed,
            Err(Empty) => Answer::Waiting,
        }
    }
}

/// Whether a dialog can be shown. Windows and macOS: always. Elsewhere rfd
/// asks the desktop portal over the session bus, and if that can't, runs
/// `zenity`; so there has to be a session bus and a portal with a file
/// chooser (xdg-desktop-portal, and a backend for the desktop: its `.portal`
/// file says it has `FileChooser`), or the program is in a Flatpak (which
/// always has the portal), or `zenity` is installed. This looks at the
/// installed files and the environment: it doesn't talk to D-Bus (a library
/// for that is some 50 crates more than rfd, which loads libdbus only when it
/// has a dialog to show).
pub fn available() -> bool {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        true
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty());
        let bus = env("DBUS_SESSION_BUS_ADDRESS").is_some()
            || env("XDG_RUNTIME_DIR").is_some_and(|d| std::path::Path::new(&d).join("bus").exists());
        let path: Vec<PathBuf> = env("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
        (bus && (std::path::Path::new("/.flatpak-info").exists()
            || portal_in(&data_dirs(env("XDG_DATA_HOME"), env("XDG_DATA_DIRS"), env("HOME")))))
            || path.iter().any(|d| d.join("zenity").is_file())
    }
}

/// The folders data files are looked for in, as the XDG spec says.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn data_dirs(
    home_data: Option<std::ffi::OsString>,
    dirs: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> Vec<PathBuf> {
    let mut v = Vec::new();
    match (home_data, home) {
        (Some(d), _) => v.push(PathBuf::from(d)),
        (None, Some(h)) => v.push(PathBuf::from(h).join(".local/share")),
        _ => {}
    }
    match dirs {
        Some(d) => v.extend(std::env::split_paths(&d)),
        None => v.extend(["/usr/local/share".into(), "/usr/share".into()]),
    }
    v
}

/// Whether DIRS have the portal's service and a backend with a file chooser.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn portal_in(dirs: &[PathBuf]) -> bool {
    let service = dirs.iter().any(|d| d.join("dbus-1/services/org.freedesktop.portal.Desktop.service").is_file());
    let chooser = dirs.iter().any(|d| {
        std::fs::read_dir(d.join("xdg-desktop-portal/portals")).into_iter().flatten().flatten().any(|e| {
            e.path().extension().is_some_and(|x| x == "portal")
                && std::fs::read_to_string(e.path())
                    .is_ok_and(|t| t.contains("org.freedesktop.impl.portal.FileChooser"))
        })
    });
    service && chooser
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dialog() -> (std::sync::mpsc::Sender<Option<PathBuf>>, Dialog) {
        let (tx, rx) = channel();
        (tx, Dialog { tab: 1, save: false, rx })
    }

    #[test]
    fn answers() {
        let (tx, d) = dialog();
        assert!(matches!(d.poll(), Answer::Waiting));
        tx.send(Some("a.lua".into())).unwrap();
        assert!(matches!(d.poll(), Answer::Chosen(p) if p == std::path::Path::new("a.lua")));
        let (tx, d) = dialog();
        tx.send(None).unwrap();
        assert!(matches!(d.poll(), Answer::Cancelled));
        let (tx, d) = dialog();
        drop(tx);
        assert!(matches!(d.poll(), Answer::Failed));
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    #[test]
    fn the_portal_is_looked_for_in_files() {
        let root = std::env::temp_dir().join(format!("lineit-portal-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dirs = [root.clone()];
        assert!(!portal_in(&dirs));
        std::fs::create_dir_all(root.join("dbus-1/services")).unwrap();
        std::fs::write(root.join("dbus-1/services/org.freedesktop.portal.Desktop.service"), "").unwrap();
        assert!(!portal_in(&dirs), "a service and no file chooser");
        std::fs::create_dir_all(root.join("xdg-desktop-portal/portals")).unwrap();
        std::fs::write(
            root.join("xdg-desktop-portal/portals/wlr.portal"),
            "Interfaces=org.freedesktop.impl.portal.Screenshot;",
        )
        .unwrap();
        assert!(!portal_in(&dirs), "a backend with no file chooser");
        std::fs::write(
            root.join("xdg-desktop-portal/portals/gtk.portal"),
            "Interfaces=org.freedesktop.impl.portal.FileChooser;",
        )
        .unwrap();
        assert!(portal_in(&dirs));
        let _ = std::fs::remove_dir_all(&root);
        // the order of the XDG folders
        let d = data_dirs(None, Some("/a:/b".into()), Some("/home/x".into()));
        assert_eq!(d, [PathBuf::from("/home/x/.local/share"), "/a".into(), "/b".into()]);
        assert_eq!(data_dirs(Some("/h".into()), None, None)[0], PathBuf::from("/h"));
    }
}

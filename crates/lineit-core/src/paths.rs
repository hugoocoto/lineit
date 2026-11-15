//! Where lineit keeps things on each OS, with no crate for it:
//!
//! |         | Linux                              | macOS                             | Windows             |
//! |---------|------------------------------------|-----------------------------------|---------------------|
//! | config  | `$XDG_CONFIG_HOME` or `~/.config`  | `~/Library/Application Support`   | `%APPDATA%`         |
//! | data    | `$XDG_DATA_HOME` or `~/.local/share` | `~/Library/Application Support` | `%APPDATA%`         |
//! | cache   | `$XDG_CACHE_HOME` or `~/.cache`    | `~/Library/Caches`                | `%LOCALAPPDATA%`    |
//!
//! each with a `lineit` folder. `LINEIT_HOME`, if set, replaces all three
//! (tests and the Docker image use it).

use std::path::PathBuf;

fn env(k: &str) -> Option<PathBuf> {
    std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from)
}

fn home() -> PathBuf {
    env("HOME").or_else(|| env("USERPROFILE")).unwrap_or_else(|| PathBuf::from("."))
}

fn base(kind: &str) -> PathBuf {
    if let Some(h) = env("LINEIT_HOME") {
        return h.join(kind);
    }
    let dir = if cfg!(windows) {
        match kind {
            "cache" => env("LOCALAPPDATA"),
            _ => env("APPDATA"),
        }
        .unwrap_or_else(home)
    } else if cfg!(target_os = "macos") {
        match kind {
            "cache" => home().join("Library/Caches"),
            _ => home().join("Library/Application Support"),
        }
    } else {
        match kind {
            "config" => env("XDG_CONFIG_HOME").unwrap_or_else(|| home().join(".config")),
            "data" => env("XDG_DATA_HOME").unwrap_or_else(|| home().join(".local/share")),
            _ => env("XDG_CACHE_HOME").unwrap_or_else(|| home().join(".cache")),
        }
    };
    dir.join("lineit")
}

/// `config.lua` and `settings.lua`.
pub fn config_dir() -> PathBuf {
    base("config")
}

/// `modules/` (yours: subgraphs, Lua modules, types) and `packages/`.
pub fn data_dir() -> PathBuf {
    base("data")
}

/// Compiled programs' builds, packages' Rust modules built for this machine.
pub fn cache_dir() -> PathBuf {
    #[cfg(test)]
    if let Some(p) = CACHE.with(|c| c.borrow().clone()) {
        return p;
    }
    base("cache")
}

#[cfg(test)]
thread_local! {
    static CACHE: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Runs F with the cache in ROOT, for this thread only (the environment is
/// the process's, and tests run side by side).
#[cfg(test)]
pub fn with_cache<T>(root: &std::path::Path, f: impl FnOnce() -> T) -> T {
    let before = CACHE.with(|c| c.replace(Some(root.to_path_buf())));
    let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    CACHE.with(|c| *c.borrow_mut() = before);
    out.unwrap_or_else(|e| std::panic::resume_unwind(e))
}

pub fn modules_dir() -> PathBuf {
    data_dir().join("modules")
}

pub fn packages_dir() -> PathBuf {
    data_dir().join("packages")
}

/// The name of an executable on this OS.
pub fn exe(name: &str) -> String {
    if cfg!(windows) { format!("{name}.exe") } else { name.to_string() }
}

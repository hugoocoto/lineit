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
    base("cache")
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

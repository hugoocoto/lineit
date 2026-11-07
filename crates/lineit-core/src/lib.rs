//! lineit without the GUI: the canvas and its files, the modules, type
//! inference, the IR, and running or compiling it. The editor and the CLI
//! (the `lineit` crate) are on top of this.

pub mod codegen;
pub mod config;
pub mod file;
pub mod fonts;
pub mod interp;
pub mod ir;
pub mod lower;
pub mod model;
pub mod native;
pub mod package;
pub mod paths;
pub mod registry;
pub mod types;

use std::path::Path;

/// Every module a canvas in DIR can use: the built-in ones, the installed
/// packages', the user's (in [`paths::modules_dir`]) and those in a
/// `modules` folder next to the canvas.
pub fn registry(dir: Option<&Path>) -> registry::Registry {
    let mut r = registry::Registry::builtin();
    package::load_installed(&mut r);
    r.load_dir(&paths::modules_dir(), "");
    if let Some(d) = dir {
        let local = d.join("modules");
        if local.is_dir() && local != paths::modules_dir() {
            r.load_dir(&local, "");
        }
    }
    r
}

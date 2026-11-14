//! lineit without the GUI: the canvas and its files, the modules, type
//! inference, the IR, and running or compiling it. The editor and the CLI
//! (the `lineit` crate) are on top of this.

pub mod bisect;
pub mod codegen;
pub mod codetest;
pub mod config;
pub mod diff;
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
pub mod trace;
pub mod types;

use std::path::Path;

/// Every module a canvas in DIR can use: the built-in ones, the installed
/// packages', the user's (in [`paths::modules_dir`]) and those in a
/// `modules` folder next to the canvas.
pub fn registry(dir: Option<&Path>) -> registry::Registry {
    registry_for(&dir.into_iter().collect::<Vec<_>>())
}

/// The modules and languages for canvases in DIRS (the editor's tabs'):
/// lineit's, the packages', and those of a `modules` and a `languages`
/// folder next to each canvas (a later one's first, if two have a module
/// of the same name).
pub fn registry_for(dirs: &[&Path]) -> registry::Registry {
    // the languages from files first: code squares can be in them
    let langs = paths::data_dir().join("languages");
    let local: Vec<std::path::PathBuf> = dirs.iter().map(|d| d.join("languages")).filter(|l| *l != langs).collect();
    let mut all = vec![langs.as_path()];
    all.extend(local.iter().map(|l| l.as_path()));
    let errors = registry::code::file::load_dirs(&all);
    let mut r = registry::Registry::builtin();
    r.errors.extend(errors);
    package::load_installed(&mut r);
    r.load_dir(&paths::modules_dir(), "");
    for d in dirs {
        let local = d.join("modules");
        if local.is_dir() && local != paths::modules_dir() {
            r.load_dir(&local, "");
        }
    }
    r
}

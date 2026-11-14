//! The commands of lineit by name, and which of them open the editor: the one
//! table `run` (lib.rs) and `lineitw` (which only opens the editor) go by.

/// A command of the command line. `run` matches on every one (no `_`), so a
/// new one is handled there and classified in [`Command::opens_editor`] or it
/// doesn't compile.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Command {
    Help,
    Version,
    Run,
    Check,
    /// Not in the help: runs a Rust or C library for the editor, apart.
    NativeHost,
    Test,
    Show,
    Text,
    Compile,
    NewPackage,
    Pack,
    Install,
    /// Not in the help: lays out canvases written by hand.
    Fmt,
    Config,
    Modules,
    /// An older editor handing over to this one (see editor/session.rs).
    Restore,
    Diff,
}

use Command::*;

/// Every command, for the tests.
#[cfg(test)]
pub const ALL: &[Command] = &[
    Help, Version, Run, Check, NativeHost, Test, Show, Text, Compile, NewPackage, Pack, Install, Fmt, Config, Modules,
    Restore, Diff,
];

/// The names each command goes by.
pub const NAMES: &[(&str, Command)] = &[
    ("--help", Help),
    ("-h", Help),
    ("help", Help),
    ("--version", Version),
    ("-V", Version),
    ("run", Run),
    ("check", Check),
    ("native-host", NativeHost),
    ("test", Test),
    ("show", Show),
    ("text", Text),
    ("compile", Compile),
    ("new-package", NewPackage),
    ("pack", Pack),
    ("install", Install),
    ("fmt", Fmt),
    ("config", Config),
    ("modules", Modules),
    ("--restore", Restore),
    ("diff", Diff),
];

impl Command {
    pub fn parse(name: &str) -> Option<Command> {
        NAMES.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
    }

    /// Whether this command, with ARGS (the whole command line), opens the
    /// editor (or helps it) instead of printing something.
    pub fn opens_editor(self, args: &[String]) -> bool {
        match self {
            Show | NativeHost | Restore => true,
            Diff => args.iter().any(|a| a == "--view"),
            Help | Version | Run | Check | Test | Text | Compile | NewPackage | Pack | Install | Fmt | Config
            | Modules => false,
        }
    }
}

/// Whether ARGS (without the program's name) open the editor: no arguments,
/// files, `--new`, or a command that does. An option that isn't known is an
/// error to print.
pub fn opens_editor(args: &[String]) -> bool {
    match args.first().map(String::as_str) {
        None => true,
        Some(first) => match Command::parse(first) {
            Some(c) => c.opens_editor(args),
            None => !first.starts_with('-') || matches!(first, "-" | "--new" | "-n"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opens(args: &[&str]) -> bool {
        opens_editor(&args.iter().map(|a| a.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn what_opens_the_editor() {
        assert!(opens(&[]));
        assert!(opens(&["a.lua"]));
        assert!(opens(&["--new", "a.lua"]));
        assert!(opens(&["show", "a.lua"]));
        assert!(opens(&["diff", "a.lua", "b.lua", "--view"]));
        assert!(!opens(&["diff", "a.lua", "b.lua"]));
        assert!(!opens(&["run", "a.lua"]));
        assert!(!opens(&["--version"]));
        assert!(!opens(&["--bogus"]));
    }

    #[test]
    fn every_command_has_a_name_and_names_are_unique() {
        for c in ALL {
            assert!(NAMES.iter().any(|(_, n)| n == c), "{c:?} has no name");
        }
        for (i, (name, c)) in NAMES.iter().enumerate() {
            assert_eq!(Command::parse(name), Some(*c));
            assert!(NAMES[..i].iter().all(|(n, _)| n != name), "{name} twice");
        }
    }
}

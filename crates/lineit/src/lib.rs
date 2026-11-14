//! lineit: the editor, and the commands that work without it. `lineit` (main.rs)
//! is the program; `lineitw` (lineitw.rs) is the one for Windows that has no
//! console window and only opens the editor.

mod bisectcmd;
mod commands;
mod diffcmd;
mod editor;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use commands::Command;
use lineit_core::{codegen, file, interp, lower, package};

const HELP: &str = "\
lineit: a visual programming language

Usage:
    lineit [FILE...]                open these canvases (none: a new one) in tabs of the
                                    editor running, or in a new editor if none is
    lineit --new [FILE...]          open them in a new editor, even if one is running
    lineit run FILE [--for SECONDS] run a canvas here, without the editor
    lineit check FILE               say what's wrong with a canvas, if anything
    lineit test FILE                run the tests of its code squares
    lineit diff A B [--view]        what changed from canvas A to B, by square id (exit
                                    0 same, 1 different, 2 error); --view: in the editor,
                                    on one canvas. A and B can be REV:path, a file as
                                    git has it
    lineit bisect FILE [--limit N] [--bad SQUARE[:RUN]] [--answers STRING]
                                    run a canvas, recording each square that ran, and
                                    ask about points of the run (good, bad, skip) to
                                    find the square where it went wrong
    lineit show FILE|REV:PATH...    open canvases, or files as git has them, read-only
    lineit text FILE                a canvas as text without positions (a git textconv
                                    driver, see the README)
    lineit compile FILE [-o OUT]    compile a canvas to a program (needs cargo)
    lineit new-package NAME [--native]  make a folder for a new package (--native:
                                    with a Rust module too)
    lineit pack DIR [-o OUT]        make a package (.lipkg) of a folder
    lineit install FILE.lipkg       install a package
    lineit modules                  list the modules lineit finds
    lineit config                   where the config goes, and every setting
    lineit --version | --help

A canvas with a clock runs until it's stopped (Ctrl+C), or for --for SECONDS.
";

/// `lineit`: the editor and the commands.
pub fn cli() -> ExitCode {
    start(false)
}

/// `lineitw`: only the editor. It has no console: what `lineit` would print
/// (help, the version, an error) is said in a message box.
pub fn gui() -> ExitCode {
    start(true)
}

/// Says TEXT: to stderr, or in a message box if there's no console.
fn say(gui: bool, text: &str, error: bool) {
    if gui {
        let level = if error { rfd::MessageLevel::Error } else { rfd::MessageLevel::Info };
        rfd::MessageDialog::new().set_level(level).set_title("lineit").set_description(text).show();
    } else {
        eprintln!("lineit: {text}");
    }
}

/// What `lineitw` does with ARGS that aren't for the editor: say what
/// `lineit` would, or that this is the program for a terminal. None: they open
/// the editor.
fn gui_only(args: &[String]) -> Option<ExitCode> {
    if commands::opens_editor(args) {
        return None;
    }
    match args.first().and_then(|a| Command::parse(a)) {
        Some(Command::Help) => say(true, HELP, false),
        Some(Command::Version) => say(true, &format!("lineit {}", env!("CARGO_PKG_VERSION")), false),
        _ => {
            let line = args.join(" ");
            say(true, &format!("lineitw only opens the editor. For `{line}` use lineit in a terminal."), true);
            return Some(ExitCode::from(2));
        }
    }
    Some(ExitCode::SUCCESS)
}

fn start(gui: bool) -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if gui {
        if let Some(code) = gui_only(&args) {
            return code;
        }
        // the processes it starts (cargo, dotnet, python...) get no console window either
        lineit_rt::ext::no_console_windows();
    }
    // Rust and C code runs in a process of its own: its crash ends its run,
    // not lineit
    if let Ok(exe) = std::env::current_exe() {
        lineit_core::native::run_apart(exe);
    }
    // like diff(1): 0 the same, 1 different, 2 trouble
    if args.first().and_then(|a| Command::parse(a)) == Some(Command::Diff) {
        let view = args.iter().any(|a| a == "--view");
        let r = match (only_flags(&args[1..], &["--view"], "diff A B [--view]"), positional(&args[1..], &[]).as_slice())
        {
            (Err(e), _) => Err(e),
            (_, [a, b]) if view => diffcmd::view(a, b).map(|()| false),
            (_, [a, b]) => diffcmd::run(a, b),
            _ => Err("diff needs two canvases: lineit diff A B".into()),
        };
        return match r {
            Ok(false) => ExitCode::SUCCESS,
            Ok(true) => ExitCode::from(1),
            Err(e) => {
                say(gui, &e, true);
                ExitCode::from(2)
            }
        };
    }
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            say(gui, &e, true);
            ExitCode::FAILURE
        }
    }
}

/// The value after FLAG, if it's there.
fn flag(args: &[String], flag: &str) -> Result<Option<String>, String> {
    match args.iter().position(|a| a == flag) {
        Some(i) => args.get(i + 1).cloned().map(Some).ok_or_else(|| format!("{flag} needs a value")),
        None => Ok(None),
    }
}

/// The arguments that aren't flags or flags' values.
fn positional<'a>(args: &'a [String], with_value: &[&str]) -> Vec<&'a String> {
    let mut v = Vec::new();
    let mut skip = false;
    for a in args {
        if std::mem::take(&mut skip) {
            continue;
        }
        if with_value.contains(&a.as_str()) {
            skip = true;
        } else if !a.starts_with('-') || a == "-" {
            v.push(a);
        }
    }
    v
}

/// An error if ARGS has an option that isn't one of OK, or a `-` (these
/// commands don't read the standard input).
fn only_flags(args: &[String], ok: &[&str], usage: &str) -> Result<(), String> {
    match args.iter().find(|a| a.starts_with('-') && !ok.contains(&a.as_str())) {
        Some(a) => Err(format!("{a}: no such option (usage: lineit {usage})")),
        None => Ok(()),
    }
}

fn one_file(args: &[String], with_value: &[&str]) -> Result<PathBuf, String> {
    match positional(args, with_value).as_slice() {
        [f] => Ok(PathBuf::from(f)),
        [] => Err("which file?".into()),
        _ => Err("only one file".into()),
    }
}

/// Reads the canvas at PATH and lowers it, with the modules it can see.
pub fn load(path: &Path) -> Result<(lineit_core::model::Canvas, lower::Lowered), String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut canvas = file::read_canvas(&src, &path.display().to_string())?;
    canvas.dir = path.parent().map(Path::to_path_buf);
    let reg = lineit_core::registry(path.parent());
    for e in &reg.errors {
        eprintln!("lineit: {e}");
    }
    let lowered = lower::lower(&canvas, &reg);
    Ok((canvas, lowered))
}

fn diags(path: &Path, canvas: &lineit_core::model::Canvas, l: &lower::Lowered) -> String {
    let mut s = String::new();
    for d in &l.diags {
        let at = d
            .square
            .and_then(|id| canvas.square(id))
            .map(|sq| format!(" (square {}, {})", sq.id, sq.module))
            .unwrap_or_default();
        s.push_str(&format!("\n  {}{at}: {}", path.display(), d.message));
    }
    s
}

fn run(args: &[String]) -> Result<(), String> {
    let cmd = args.first().map(String::as_str).unwrap_or("");
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match Command::parse(cmd) {
        Some(Command::Help) => {
            print!("{HELP}");
            Ok(())
        }
        Some(Command::Version) => {
            println!("lineit {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some(Command::Run) => {
            let path = one_file(rest, &["--for"])?;
            let seconds = flag(rest, "--for")?
                .map(|s| s.parse::<f64>().map_err(|_| format!("--for {s}: not a number")))
                .transpose()?;
            let (canvas, l) = load(&path)?;
            let prog = l.program.clone().ok_or_else(|| format!("it can't run:{}", diags(&path, &canvas, &l)))?;
            let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
            use std::io::Write;
            let out = std::io::stdout();
            let mut out = std::io::BufWriter::new(out.lock());
            let r = interp::run_to_end(prog, dir, seconds, |line| {
                let _ = writeln!(out, "{line}");
            });
            let _ = out.flush();
            r
        }
        Some(Command::Bisect) => {
            let with_value = ["--limit", "--bad", "--answers"];
            only_flags(rest, &with_value, "bisect FILE [--limit N] [--bad SQUARE[:RUN]] [--answers STRING]")?;
            let path = one_file(rest, &with_value)?;
            let limit = flag(rest, "--limit")?
                .map(|s| s.parse::<usize>().map_err(|_| format!("--limit {s}: not a number")))
                .transpose()?
                .unwrap_or(bisectcmd::DEFAULT_LIMIT);
            let opts = bisectcmd::Options {
                limit,
                bad: flag(rest, "--bad")?,
                answers: flag(rest, "--answers")?.unwrap_or_default(),
                file: path.display().to_string(),
            };
            let (canvas, l) = load(&path)?;
            let prog = l.program.clone().ok_or_else(|| format!("it can't run:{}", diags(&path, &canvas, &l)))?;
            let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
            bisectcmd::run(prog, dir, &opts, &mut std::io::stdin().lock(), &mut std::io::stdout().lock())
        }
        Some(Command::Check) => {
            let path = one_file(rest, &[])?;
            let (canvas, l) = load(&path)?;
            if l.diags.is_empty() {
                println!("{}: fine", path.display());
                Ok(())
            } else {
                Err(format!("it has problems:{}", diags(&path, &canvas, &l)))
            }
        }
        // not in the help: runs a Rust or C library for the editor, apart
        Some(Command::NativeHost) => match rest {
            [lib, crate_name, address, token] => {
                let token = token.parse().map_err(|_| "native-host: a bad token")?;
                lineit_core::native::serve(Path::new(lib), crate_name, address, token)
            }
            _ => Err("native-host LIB CRATE ADDRESS TOKEN".into()),
        },
        Some(Command::Test) => {
            let path = one_file(rest, &[])?;
            let (canvas, l) = load(&path)?;
            let all = lineit_core::codetest::run_canvas(&canvas, &l);
            let (mut passed, mut failed) = (0, 0);
            // the canvas's tests: a run from each test/case
            let reg = lineit_core::registry(path.parent());
            let cases = lineit_core::codetest::run_cases(&canvas, &reg);
            for c in &cases {
                let what = format!("{} (test `{}`)", path.display(), c.name);
                if c.ok() {
                    passed += 1;
                    println!("{what}: passed, {} check{}", c.checks.len(), if c.checks.len() == 1 { "" } else { "s" });
                    continue;
                }
                failed += 1;
                println!("{what}: failed");
                for k in c.checks.iter().filter(|k| !k.ok) {
                    let label = if k.label.is_empty() { format!("square {}", k.square) } else { k.label.clone() };
                    println!("  {label}: expected {}, got {}", k.expected.text(), k.got.text());
                }
                if let Some(e) = &c.error {
                    println!("  {e}");
                }
            }
            for t in &all {
                let what = format!(
                    "{} (square {}{})",
                    path.display(),
                    t.square,
                    if t.name.is_empty() { String::new() } else { format!(", {}", t.name) }
                );
                match &t.results {
                    Err(e) => {
                        failed += t.rows.max(1);
                        println!("{what}: {e}");
                    }
                    Ok(rows) => {
                        let bad: Vec<(usize, &lineit_core::codetest::RowResult)> =
                            rows.iter().enumerate().filter(|(_, r)| !r.ok()).collect();
                        passed += rows.len() - bad.len();
                        failed += bad.len();
                        println!("{what}: {} of {} passed", rows.len() - bad.len(), rows.len());
                        for (k, r) in bad {
                            for p in &r.problems {
                                println!("  test {}: {p}", k + 1);
                            }
                            for line in &r.printed {
                                println!("  test {} printed: {line}", k + 1);
                            }
                        }
                    }
                }
            }
            if all.is_empty() && cases.is_empty() {
                println!("{}: it has no tests (test/case squares, or tests on code squares)", path.display());
            }
            if failed > 0 { Err(format!("{failed} failed, {passed} passed")) } else { Ok(()) }
        }
        Some(Command::Show) => {
            only_flags(rest, &[], "show FILE|REV:PATH...")?;
            match positional(rest, &[]).as_slice() {
                [] => Err("which file? FILE or REV:path".into()),
                specs => diffcmd::show(specs),
            }
        }
        Some(Command::Text) => {
            only_flags(rest, &[], "text FILE")?;
            let (canvas, _) = diffcmd::read(&one_file(rest, &[])?.to_string_lossy())?;
            print!("{}", lineit_core::diff::outline(&canvas));
            Ok(())
        }
        Some(Command::Compile) => {
            let path = one_file(rest, &["-o"])?;
            let (canvas, l) = load(&path)?;
            let prog = l.program.clone().ok_or_else(|| format!("it can't compile:{}", diags(&path, &canvas, &l)))?;
            let out = match flag(rest, "-o")? {
                Some(o) => PathBuf::from(o),
                None => path.with_file_name(lineit_core::paths::exe(&codegen::program_name(&path))),
            };
            let exe =
                codegen::compile(&prog, &codegen::Options::for_canvas(&path, out), &mut |line| eprintln!("{line}"))?;
            eprintln!("lineit: compiled to {}", exe.display());
            Ok(())
        }
        Some(Command::NewPackage) => {
            let name = one_file(rest, &[])?;
            let dir = package::scaffold(&name, rest.iter().any(|a| a == "--native"))?;
            println!("made {}: fill in its modules/, then `lineit pack {}`", dir.display(), dir.display());
            Ok(())
        }
        Some(Command::Pack) => {
            let dir = one_file(rest, &["-o"])?;
            let out = flag(rest, "-o")?.map(PathBuf::from);
            let file = package::pack(&dir, out.as_deref())?;
            println!("made {}", file.display());
            Ok(())
        }
        Some(Command::Install) => {
            let file = one_file(rest, &[])?;
            let to = package::install(&file)?;
            println!("installed {}", to.display());
            Ok(())
        }
        // not in the help: lays out canvases written by hand, for the examples
        Some(Command::Fmt) => {
            for f in positional(rest, &[]) {
                let path = Path::new(f);
                let src = std::fs::read_to_string(path).map_err(|e| format!("{f}: {e}"))?;
                let mut c = file::read_canvas(&src, f)?;
                if c.needs_arranging() {
                    lower::arrange(&mut c, &lineit_core::registry(path.parent()));
                }
                std::fs::write(path, file::write_canvas(&c)).map_err(|e| format!("{f}: {e}"))?;
            }
            Ok(())
        }
        Some(Command::Config) => {
            let dir = lineit_core::paths::config_dir();
            println!("-- lineit reads {}", dir.join("config.lua").display());
            print!("{}", lineit_core::config::default_config_text());
            Ok(())
        }
        Some(Command::Modules) => {
            let reg = lineit_core::registry(Some(Path::new(".")));
            for e in &reg.errors {
                eprintln!("lineit: {e}");
            }
            for (path, m) in &reg.modules {
                println!("{path:28} {}", m.doc());
            }
            Ok(())
        }
        // an older editor handing over to this one (see editor/session.rs)
        Some(Command::Restore) => editor::restore(&one_file(rest, &[])?),
        Some(Command::Diff) => unreachable!("start handles diff"),
        None if cmd.starts_with('-') && cmd != "-" && cmd != "--new" && cmd != "-n" => {
            Err(format!("{cmd}: no such option (see lineit --help)"))
        }
        None => {
            let new = args.iter().any(|a| a == "--new" || a == "-n");
            let files = args.iter().filter(|a| *a != "--new" && *a != "-n").map(PathBuf::from).collect();
            // what code squares' processes write to stderr shows in the Output
            lineit_rt::ext::capture_stderr();
            editor::run(files, new)
        }
    }
}

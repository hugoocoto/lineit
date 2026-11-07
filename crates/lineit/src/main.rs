//! lineit: the editor, and the commands that work without it.

mod editor;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lineit_core::{codegen, file, interp, lower, package};

const HELP: &str = "\
lineit: a visual programming language

Usage:
    lineit [FILE...]                open these canvases (none: a new one) in tabs of the
                                    editor running, or in a new editor if none is
    lineit --new [FILE...]          open them in a new editor, even if one is running
    lineit run FILE [--for SECONDS] run a canvas here, without the editor
    lineit check FILE               say what's wrong with a canvas, if anything
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

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("lineit: {e}");
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
    let canvas = file::read_canvas(&src, &path.display().to_string())?;
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
    match cmd {
        "--help" | "-h" | "help" => {
            print!("{HELP}");
            Ok(())
        }
        "--version" | "-V" => {
            println!("lineit {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "run" => {
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
        "check" => {
            let path = one_file(rest, &[])?;
            let (canvas, l) = load(&path)?;
            if l.diags.is_empty() {
                println!("{}: fine", path.display());
                Ok(())
            } else {
                Err(format!("it has problems:{}", diags(&path, &canvas, &l)))
            }
        }
        "compile" => {
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
        "new-package" => {
            let name = one_file(rest, &[])?;
            let dir = package::scaffold(&name, rest.iter().any(|a| a == "--native"))?;
            println!("made {}: fill in its modules/, then `lineit pack {}`", dir.display(), dir.display());
            Ok(())
        }
        "pack" => {
            let dir = one_file(rest, &["-o"])?;
            let out = flag(rest, "-o")?.map(PathBuf::from);
            let file = package::pack(&dir, out.as_deref())?;
            println!("made {}", file.display());
            Ok(())
        }
        "install" => {
            let file = one_file(rest, &[])?;
            let to = package::install(&file)?;
            println!("installed {}", to.display());
            Ok(())
        }
        // not in the help: lays out canvases written by hand, for the examples
        "fmt" => {
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
        "config" => {
            let dir = lineit_core::paths::config_dir();
            println!("-- lineit reads {}", dir.join("config.lua").display());
            print!("{}", lineit_core::config::default_config_text());
            Ok(())
        }
        "modules" => {
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
        "--restore" => editor::restore(&one_file(rest, &[])?),
        _ if cmd.starts_with('-') && cmd != "-" && cmd != "--new" && cmd != "-n" => {
            Err(format!("{cmd}: no such option (see lineit --help)"))
        }
        _ => {
            let new = args.iter().any(|a| a == "--new" || a == "-n");
            let files = args.iter().filter(|a| *a != "--new" && *a != "-n").map(PathBuf::from).collect();
            editor::run(files, new)
        }
    }
}

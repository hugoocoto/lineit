//! Every canvas in the repo's `tests/` and `examples/` folders, run by the
//! interpreter, must print what its `.out` file says.
//!
//! With `LINEIT_TEST_COMPILE=1`, each is also compiled (with cargo, so it
//! takes a while) and the program must print the same.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn canvases() -> Vec<PathBuf> {
    let mut v = Vec::new();
    let dotnet = lineit_core::registry::code::sdk_ready();
    for dir in ["tests", "examples"] {
        let Ok(entries) = std::fs::read_dir(root().join(dir)) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().is_none_or(|x| x != "lua") || !p.with_extension("out").exists() {
                continue;
            }
            if !dotnet && p.file_name().is_some_and(|n| n.to_string_lossy().contains("csharp")) {
                eprintln!("skipped {}: no .NET 10 SDK", p.display());
            } else {
                v.push(p);
            }
        }
    }
    v.sort();
    v
}

fn lower(path: &Path) -> lineit_core::ir::Program {
    let src = std::fs::read_to_string(path).unwrap();
    let mut canvas = lineit_core::file::read_canvas(&src, &path.display().to_string()).unwrap();
    canvas.dir = path.parent().map(|p| p.to_path_buf());
    // the languages from files the canvases use (always the same, so tests
    // in parallel load the same)
    let langs = root().join("examples/languages");
    lineit_core::registry::code::file::load_dirs(&[langs.as_path()]);
    let mut reg = lineit_core::registry::Registry::builtin();
    reg.load_dir(&path.parent().unwrap().join("modules"), "");
    assert!(reg.errors.is_empty(), "{:?}", reg.errors);
    let l = lineit_core::lower::lower(&canvas, &reg);
    l.program.unwrap_or_else(|| panic!("{}: {:?}", path.display(), l.diags))
}

/// Clocks run for this long.
const SECONDS: f64 = 0.4;

#[test]
fn interpreted() {
    let mut failed = Vec::new();
    for path in canvases() {
        let want = std::fs::read_to_string(path.with_extension("out")).unwrap();
        let mut got = String::new();
        let r = lineit_core::interp::run_to_end(lower(&path), path.parent().unwrap().into(), Some(SECONDS), |l| {
            got.push_str(l);
            got.push('\n');
        });
        if let Err(e) = r {
            failed.push(format!("{}: {e}", path.display()));
        } else if got != want {
            failed.push(format!("{}:\n--- want\n{want}--- got\n{got}", path.display()));
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}

#[test]
fn compiled() {
    if std::env::var_os("LINEIT_TEST_COMPILE").is_none() {
        eprintln!("skipped: set LINEIT_TEST_COMPILE=1 to compile every canvas");
        return;
    }
    let out = std::env::temp_dir().join(format!("lineit-test-{}", std::process::id()));
    std::fs::create_dir_all(&out).unwrap();
    let mut failed = Vec::new();
    for path in canvases() {
        let want = std::fs::read_to_string(path.with_extension("out")).unwrap();
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let exe = out.join(lineit_core::paths::exe(&name));
        let opts = lineit_core::codegen::Options::for_canvas(&path, exe.clone());
        let mut log = String::new();
        let prog = lower(&path);
        // a canvas with a clock runs until it's killed; the others, to their end
        let limit = if prog.clocks.is_empty() { 60.0 } else { SECONDS + 0.05 };
        if let Err(e) = lineit_core::codegen::compile(&prog, &opts, &mut |l| {
            log.push_str(l);
            log.push('\n');
        }) {
            failed.push(format!("{}: {e}\n{log}", path.display()));
            continue;
        }
        let mut child = std::process::Command::new(&exe)
            .current_dir(path.parent().unwrap())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let started = std::time::Instant::now();
        while child.try_wait().unwrap().is_none() && started.elapsed().as_secs_f64() < limit {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let _ = child.kill();
        let got = child.wait_with_output().unwrap();
        let got = String::from_utf8_lossy(&got.stdout).to_string();
        if got != want {
            failed.push(format!("{} (compiled):\n--- want\n{want}--- got\n{got}", path.display()));
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}

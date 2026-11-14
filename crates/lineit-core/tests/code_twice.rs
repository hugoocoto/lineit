//! A code square's state starts as its code says on every run, also when
//! the editor runs a canvas again (its libraries stay loaded).

use std::path::Path;

#[test]
fn twice() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests");
    for name in ["code", "code_python", "code_rust", "code_c", "code_csharp"] {
        if name == "code_csharp" && !lineit_core::registry::code::sdk_ready() {
            eprintln!("skipped {name}: no .NET 10 SDK");
            continue;
        }
        let path = root.join(format!("{name}.lua"));
        let want = std::fs::read_to_string(path.with_extension("out")).unwrap();
        let mut canvas = lineit_core::file::read_canvas(&std::fs::read_to_string(&path).unwrap(), name).unwrap();
        canvas.dir = Some(root.clone());
        let reg = lineit_core::registry::Registry::builtin();
        for run in 1..=2 {
            let prog = lineit_core::lower::lower(&canvas, &reg).program.unwrap();
            let mut got = String::new();
            lineit_core::interp::run_to_end(prog, root.clone(), None, |l| {
                got.push_str(l);
                got.push('\n');
            })
            .unwrap();
            assert_eq!(got, want, "{name}, run {run}");
        }
    }
}

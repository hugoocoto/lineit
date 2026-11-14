//! The tests on the code squares of `tests/code*.lua` pass, in every
//! language.

use std::path::Path;

#[test]
fn all_pass() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests");
    for name in ["code", "code_python", "code_rust", "code_c"] {
        let path = root.join(format!("{name}.lua"));
        let mut canvas = lineit_core::file::read_canvas(&std::fs::read_to_string(&path).unwrap(), name).unwrap();
        canvas.dir = Some(root.clone());
        let l = lineit_core::lower::lower(&canvas, &lineit_core::registry::Registry::builtin());
        let all = lineit_core::codetest::run_canvas(&canvas, &l);
        assert_eq!(all.len(), 2, "{name}: two squares with tests");
        for t in all {
            let rows = t.results.unwrap_or_else(|e| panic!("{name}: {e}"));
            for (k, r) in rows.iter().enumerate() {
                assert!(r.ok(), "{name}, {} test {}: {:?}", t.name, k + 1, r.problems);
            }
        }
    }
}

//! image/shader reads its shader from a file, and the uniforms of the
//! shader are its inputs: the pins follow the file.

use lineit_core::lower;
use lineit_core::registry::Registry;

fn canvas(dir: &std::path::Path, file: &str) -> lineit_core::model::Canvas {
    let src = format!(
        "return {{ lineit = 1, squares = {{ {{ id = 1, module = \"image/shader\", params = {{ file = \"{file}\" }} }} }}, links = {{}} }}"
    );
    let mut c = lineit_core::file::read_canvas(&src, "test").unwrap();
    c.dir = Some(dir.to_path_buf());
    c
}

fn pins(l: &lower::Lowered) -> Vec<String> {
    l.squares[&1].sig.inputs.iter().map(|p| p.name.clone()).collect()
}

#[test]
fn pins_follow_the_file() {
    let dir = std::env::temp_dir().join(format!("lineit-shader-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let reg = Registry::builtin();
    let file = dir.join("s.gdshader");

    // uniforms are inputs, after TEXTURE and TIME
    std::fs::write(&file, "shader_type canvas_item;\nuniform float amount = 0.5;\nvoid fragment() {}\n").unwrap();
    let l = lower::lower(&canvas(&dir, "s.gdshader"), &reg);
    assert_eq!(pins(&l), ["TEXTURE", "TIME", "amount"]);
    // the file the pins came from is known, to lower again when it changes
    assert!(l.deps.iter().any(|(p, t)| p.ends_with("s.gdshader") && t.is_some()));

    // another uniform: another input
    std::fs::write(&file, "shader_type canvas_item;\nuniform float amount;\nuniform int steps = 3;\n").unwrap();
    let l = lower::lower(&canvas(&dir, "s.gdshader"), &reg);
    assert_eq!(pins(&l), ["TEXTURE", "TIME", "amount", "steps"]);

    // a shader that doesn't work says why, with the line
    std::fs::write(&file, "shader_type canvas_item;\nvoid fragment() { COLOR = nope; }\n").unwrap();
    let l = lower::lower(&canvas(&dir, "s.gdshader"), &reg);
    assert!(l.diags.iter().any(|d| d.message.contains("line 2") && d.message.contains("nope")), "{:?}", l.diags);

    // a file that isn't there is an error, and waiting for it is a dependency
    let l = lower::lower(&canvas(&dir, "missing.gdshader"), &reg);
    assert!(l.diags.iter().any(|d| d.message.contains("missing.gdshader")), "{:?}", l.diags);
    assert!(l.deps.iter().any(|(p, t)| p.ends_with("missing.gdshader") && t.is_none()));

    // no file: the image as it is
    let l = lower::lower(&canvas(&dir, ""), &reg);
    assert!(l.diags.is_empty(), "{:?}", l.diags);
    assert_eq!(pins(&l), ["TEXTURE", "TIME"]);

    std::fs::remove_dir_all(&dir).unwrap();
}

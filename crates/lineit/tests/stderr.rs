//! What the processes of code squares write to stderr is kept, and comes out
//! on lineit's stderr, not its stdout: dotnet's "The build failed" isn't said before
//! lineit's own error, which has the compiler's message already.
// tests start the programs as a user does
#![allow(clippy::disallowed_methods)]

fn run(canvas: &str) -> (Option<i32>, String, String) {
    let canvas = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join(canvas);
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_lineit")).arg("run").arg(&canvas).output().unwrap();
    let text = |b: &[u8]| String::from_utf8_lossy(b).replace("\r\n", "\n");
    (out.status.code(), text(&out.stdout), text(&out.stderr))
}

#[test]
fn a_build_that_fails_is_said_once() {
    let (code, stdout, stderr) = run("stderr_bad_csharp.lua");
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.starts_with("lineit: F: the code doesn't build:"), "{stderr}");
    assert!(stderr.contains("error CS0103"), "{stderr}");
    assert!(!stderr.contains("The build failed"), "{stderr}");
    assert!(!stdout.contains("The build failed"), "{stdout}");
}

#[test]
fn what_a_square_writes_to_stderr_while_it_runs_goes_to_stderr() {
    let (code, stdout, stderr) = run("stderr_python.lua");
    assert_eq!(code, Some(0), "{stderr}");
    // stdout is the program's alone
    assert_eq!(stdout, "before\nafter\n");
    assert_eq!(stderr, "to stderr\n");
}

/// Runs a canvas with one square in LANGUAGE (Lua: "") that writes "mine" itself.
fn says(language: &str, code: &str) -> (Option<i32>, String, String) {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    let file = dir.join(format!("says_{}.lua", if language.is_empty() { "lua" } else { language }));
    let lang = if language.is_empty() { String::new() } else { format!("language = {language:?}, ") };
    let canvas = format!(
        "return {{ lineit = 1, squares = {{ {{ id = 1, module = \"flow/entry\" }}, {{ id = 2, module = \"code/function\", params = {{ {lang}code = {code:?} }} }} }}, links = {{ {{ 1, \"then\", 2, \"tick\" }} }} }}"
    );
    std::fs::write(&file, canvas).unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_lineit")).arg("run").arg(&file).output().unwrap();
    let text = |b: &[u8]| String::from_utf8_lossy(b).replace("\r\n", "\n");
    (out.status.code(), text(&out.stdout), text(&out.stderr))
}

/// What a square writes to stdout is the program's, not stderr's, in each
/// kind of module: a library in a process of its own (C and Rust: raw, to
/// the process's stdout), and Python, C# and Lua (their `print`, which comes
/// in the answers of a process whose stdout is its talk with lineit, or is
/// lineit's own in-process).
fn stays_on_stdout(language: &str, code: &str) {
    let (status, stdout, stderr) = says(language, code);
    assert_eq!((status, stdout.as_str(), stderr.as_str()), (Some(0), "mine\n", ""));
}

#[test]
fn c_stdout_stays_on_stdout() {
    stays_on_stdout("c", "#include <stdio.h>\nvoid say(void) {\n    printf(\"mine\\n\");\n    fflush(stdout);\n}\n");
}

#[test]
fn rust_stdout_stays_on_stdout() {
    stays_on_stdout("rust", "pub fn say() {\n    println!(\"mine\");\n}\n");
}

#[test]
fn python_stdout_stays_on_stdout() {
    stays_on_stdout("python", "def say():\n    print(\"mine\")\n");
}

#[test]
fn csharp_stdout_stays_on_stdout() {
    stays_on_stdout(
        "csharp",
        "static class A\n{\n    public static void Say()\n    {\n        System.Console.WriteLine(\"mine\");\n    }\n}\n",
    );
}

#[test]
fn lua_stdout_stays_on_stdout() {
    stays_on_stdout("", "function say()\n    print(\"mine\")\nend\n");
}

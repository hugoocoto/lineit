//! Rust and C code runs in a process of its own: a crash in it ends the
//! run with an error, not lineit, and what was printed before stays.
// tests start the programs as a user does
#![allow(clippy::disallowed_methods)]

#[test]
fn a_crash_is_an_error() {
    let canvas = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/crash.lua");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_lineit")).arg("run").arg(&canvas).output().unwrap();
    // C writes "\r\n" on Windows
    let stdout = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stdout.contains("still talking\n"), "{stdout}");
    assert!(stderr.contains("the code crashed"), "{stderr}");
    // what the code wrote to stdout itself is there too, and didn't get in
    // the way of lineit's talk with it
    assert!(stdout.contains("raw to stdout\n"), "{stdout}");
}

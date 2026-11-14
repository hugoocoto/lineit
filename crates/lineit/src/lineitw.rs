//! lineitw: lineit's editor with no console window, for a double click, the
//! Start menu and file associations on Windows. `lineit` (main.rs) has the
//! console and the commands.
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    lineit_app::gui()
}

#[cfg(not(windows))]
fn main() -> std::process::ExitCode {
    eprintln!("lineitw is for Windows (the editor without a console window): use lineit");
    std::process::ExitCode::FAILURE
}

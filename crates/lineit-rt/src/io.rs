//! What a compiled program prints. Lines go through one buffer, flushed
//! when the program waits (clocks) or ends, so printing in a loop doesn't
//! make a system call per line.

use std::io::{BufWriter, Stdout, Write};
use std::sync::Mutex;

static OUT: Mutex<Option<BufWriter<Stdout>>> = Mutex::new(None);

pub fn print(line: &str) {
    let mut out = OUT.lock().unwrap_or_else(|e| e.into_inner());
    let w = out.get_or_insert_with(|| BufWriter::with_capacity(1 << 16, std::io::stdout()));
    let _ = w.write_all(line.as_bytes());
    let _ = w.write_all(b"\n");
}

pub fn flush() {
    let mut out = OUT.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(w) = out.as_mut() {
        let _ = w.flush();
    }
}

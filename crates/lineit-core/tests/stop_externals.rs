//! Stop ends a code square stuck in its code, in a process of its own: its
//! process is killed, and the run ends instead of waiting for it.

/// A canvas whose square in LANGUAGE runs CODE, from its start.
fn stuck(language: &str, code: &str) {
    let src = format!(
        r#"return {{ lineit = 1, squares = {{
        {{ id = 1, module = "flow/entry" }},
        {{ id = 2, module = "code/function", params = {{ language = "{language}", code = {} }} }},
    }}, links = {{ {{ 1, "then", 2, "tick" }} }} }}"#,
        lineit_core::file::quote(code)
    );
    let src = src.as_str();
    let canvas = lineit_core::file::read_canvas(src, "stuck").unwrap();
    let prog = lineit_core::lower::lower(&canvas, &lineit_core::registry::Registry::builtin()).program.unwrap();
    let mut it = lineit_core::interp::Interp::new(prog, std::env::temp_dir()).unwrap();
    let w = it.w.clone();
    let killer = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(500));
        w.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        w.kill_externals();
    });
    let started = std::time::Instant::now();
    it.start().unwrap();
    let r = loop {
        match it.run(100_000) {
            Ok(lineit_core::interp::Status::Idle) => break Ok(()),
            Ok(_) => {}
            Err(e) => break Err(e),
        }
    };
    killer.join().unwrap();
    assert!(r.is_err(), "{language}: the stuck square's run ends with what happened to it");
    assert!(started.elapsed().as_secs_f64() < 10.0, "{language}: it took {:?}", started.elapsed());
}

#[test]
fn a_stuck_python_square_stops() {
    stuck("python", "def spin():\n    while True:\n        pass\n");
}

#[test]
fn a_stuck_lua_square_stops() {
    // Lua runs in lineit: it checks Stop every so often
    stuck("lua", "function spin()\n    while true do end\nend\n");
}

#[test]
fn a_stuck_lua_square_stops_inside_pcall() {
    // a loop that catches errors: Stop can't be caught for long
    stuck("lua", "function spin()\n    while true do\n        pcall(function() for i = 1, 100 do end end)\n    end\nend\n");
}

# Contributing

Thanks for wanting to help. Bugs and ideas go in the
[issues](https://github.com/hugoocoto/lineit/issues); changes in pull
requests, against `main`.

## Building and testing

    git config core.hooksPath scripts/hooks   # format, clippy and the tests before every push
    cargo build                          # everything
    cargo run -- examples/hello.lua      # the editor
    cargo test                           # unit tests, and every canvas in tests/ and examples/, run
    LINEIT_TEST_COMPILE=1 cargo test     # and compiled too (slow: cargo builds each one)
    cargo fmt --all
    cargo clippy --all-targets -- -D warnings

CI runs all of it on Linux, macOS and Windows, the tests again with
AddressSanitizer and ThreadSanitizer (Rust nightly, see the workflow), and
builds and checks what's released (see `.github/workflows/ci.yml`). The
hook skips once with `git push --no-verify`.

`LINEIT_HOME=DIR` makes lineit keep its config, modules, packages and
builds in `DIR`, not in yours: useful to try things.

## Where things are

- `crates/lineit-rt`: the runtime. Values, strings, lists, images, branches,
  Lua and Rust modules. The interpreter and compiled programs both use it,
  so it builds on its own, with no dependency unless a feature asks for it:
  lineit writes it out next to every program it compiles.
- `crates/lineit-core`: everything but the GUI. The canvas and its files
  (`model`, `file`), comparing two (`diff`), the modules (`registry/`), type inference (`types`),
  lowering to the IR (`lower`, `ir`), the interpreter (`interp`), the
  codegen (`codegen`), packages (`package`, `native`) and the config.
- `crates/lineit`: the command line (`main.rs`) and the editor (`editor/`,
  egui).
- `tests/`, `examples/`: canvases, each with the `.out` it must print.

[`docs/internals.typ`](docs/internals.typ) explains how they work.

## Changes

- A change in behavior comes with a canvas that shows it, in `tests/` (or
  `examples/` if it's worth showing), with its `.out`. Canvases can be
  written by hand without positions: `lineit fmt FILE` lays them out.
- A new built-in module goes in `registry/builtin.rs`, as IR: it never runs
  code of its own, so the interpreter and the codegen both get it for free.
  If it needs a function, put it in `lineit-rt`, used by both.
- Comments say what and why, in plain words, as the code around them does.

## Documentation

What users see is in the README; how lineit works, in
`docs/internals.typ`; what's planned, in `docs/roadmap.md`. A change that
makes one of them wrong updates it in the same pull request.

## Versions

- The canvas format is `file::FORMAT`: a change that older lineits couldn't
  read raises it.
- The native module ABI is `lineit_rt::native::ABI`: a change to the C
  functions `export_modules!` makes, or to how values are encoded, raises
  it.
- A release is a tag `vX.Y.Z` with the version in `Cargo.toml`
  (`scripts/version.sh` prints it): bump it in a commit on main, then
  `scripts/release.sh` tags it and pushes the tag, and CI builds and
  publishes it.

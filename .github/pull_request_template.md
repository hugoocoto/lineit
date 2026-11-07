<!-- What does this change, and why? Link the issue it fixes: Fixes #123 -->

- [ ] `cargo fmt --all --check` and `cargo clippy --all-targets -- -D warnings` pass
- [ ] `cargo test` passes, and `LINEIT_TEST_COMPILE=1 cargo test` if it touches
      the IR, the codegen or `lineit-rt`
- [ ] New behavior has a canvas in `tests/` (or `examples/`) with its `.out`
- [ ] Docs are updated if it changes what users see or how lineit works, see
      Documentation in CONTRIBUTING.md
- [ ] The canvas format (`file::FORMAT`) or the native ABI (`native::ABI`)
      goes up if it changes them, see Versions in CONTRIBUTING.md

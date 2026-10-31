# Contributing to lineit

Thanks for helping! Bug reports, ideas, modules, docs and code are all
welcome. lineit is in alpha: things change and break between commits, and
that's fine while it is.

## Reporting bugs

[Open an issue](../../issues/new/choose). The more of this it has, the
faster it gets fixed:

- what you did, and what lineit printed (the terminal and the output panel),
- the canvas, or the smallest one that shows it,
- whether it happens with Run, with Compile, or both,
- the output of `lineit --version` (and the commit, if you built it), and
  your distro.

Search the open issues first: someone may have reported it already. A
security problem goes privately instead, not in an issue: see
[SECURITY.md](.github/SECURITY.md).

## Suggesting features

Open an issue describing the problem you want to solve, not just the
solution. For anything that needs more than a few lines of code, agree on it
in the issue before writing it: it saves everyone's time.

## Setting up

See [From source](README.md#from-source) for what to install, then:

```sh
git clone --recursive https://github.com/hugoocoto/lineit
cd lineit
make hooks   # run the tests before every push
make         # or make debug, with the sanitizers, while working
make test
```

Editors that use clangd get the right flags from the `.clangd` files, and
the code style from `.clang-format` and `.editorconfig`.

How lineit works inside, from the module API to Compile, is in
`docs/internals.typ` (`make docs` makes the PDF). Read the parts you'll
touch: it names the functions, so you can read the code next to it.

### Project layout

```
src/main.c               the editor: canvas, modules, Run, Compile, tabs, reloads
src/lineit.h             the module API, documented: all a module includes
src/value.c              values, variables, lists, trees: the editor's and Compile's
src/rt/                  the runtime the programs Compile makes link with
modules/                 the standard modules (.c), subgraphs (.lua), types (.type.lua)
examples/, tests/        canvases, each with the .out make test checks it against
tools/                   the test runners, gen_examples, lineit-pack (packages)
thirdparty/              git submodules: flag.h, conf.h, cum.h, bt.h
docs/                    Typst documents, to PDF with make docs
scripts/                 install.sh, release.sh, git hooks
assets/                  the icon and the desktop entry
```

## Making a change

1. Fork the repo and make a branch from main.
2. Make the change, with a test (see [Tests](#tests)).
3. Update the docs if it changes what users see or how lineit works (see
   [Documentation](#documentation)).
4. Check it: `make WERROR=1`, `make test`, `make test-asan`,
   `make analyze` and `make check-format`.
5. Open a pull request, saying what it changes and why. Link the issue it
   fixes (`Fixes #123`).

CI builds and tests it with gcc and clang, with and without the
sanitizers. A pull request is merged when CI is green and it has been
reviewed. Small pull requests are reviewed much faster than big ones: split
them when you can.

## Code style

- No warnings with `-Wall -Wextra -Wformat=2 -Wshadow`: CI builds with
  `make WERROR=1`, which makes them errors. Warnings in `thirdparty/` and
  raylib don't count, they're included with `-isystem`.
- Format with `make format`. CI checks it with the clang-format version in
  the Makefile (`CLANG_FORMAT_VERSION`): if yours formats differently,
  install that one with `pipx install clang-format==<version>` and run
  `make format CLANG_FORMAT=~/.local/bin/clang-format`.
- Free what you allocate: `make test-asan` fails on any leak.
- Comments say why, not what. Keep them short.
- In modules, everything but the `module_*` callbacks is `static`: Compile
  puts a copy of the module per square in one program (see `src/lineit.h`).

## Tests

`make test` runs every canvas in `examples/` and `tests/` in the editor,
for 10 seconds of fake time, and compares what it prints and the text of
every square with the `.out` next to it. Then it compiles each one and
checks the program prints the same: Run and Compile have to agree. It also
checks Make subgraph on some examples. `make test-asan` does it all again
with AddressSanitizer and UBSan.

To add a test, add a canvas to `tests/`, with a comment at the top saying
what it checks, and run `make test-update` to write its `.out`. Read the
`.out` before keeping it: it's what the test will expect from now on.

## Documentation

A change users can see updates, in the same pull request:

- the README, for what it shows (keys, install, build, config)
- `config.lua`, for a setting: it documents every one
- `src/lineit.h`, for the module API
- `docs/internals.typ`, for how lineit works inside

## Versions

There's one version, `LI_VERSION` in `src/lineit.h`: of lineit, its module
API and its files. Something made with a version works with a lineit of the
same major and a minor at least as new. A change that adds something to the
API or the files bumps the minor; one that changes or removes something, the
major (see Versions in `docs/internals.typ`).

## Releases

Maintainers release from main: bump `LI_VERSION` in a commit, push it, and
run `scripts/release.sh`, which tags it and pushes the tag. CI then builds
and publishes the release, only if every check passes, and refuses a tag
that isn't `LI_VERSION`. Every push to main updates the nightly release.

## Third-party code

`thirdparty/` holds git submodules. Fix bugs in them upstream; to update
one by hand:

```sh
git -C thirdparty/flag.h pull origin main
git add thirdparty/flag.h
```

Dependabot also opens a pull request every week when they have new
commits, and for the GitHub Actions, and CI tests it.

## License

By contributing you agree that your work is licensed under the GPLv3 or
later, like the rest of the project.

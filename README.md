# lineit

**A visual programming language: you wire squares on a canvas, run the
program in the editor, step through it, or compile it to a native program.**

[![CI](https://github.com/hugoocoto/lineit/actions/workflows/ci.yml/badge.svg)](https://github.com/hugoocoto/lineit/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/hugoocoto/lineit)](LICENSE)

![The images example in the lineit editor: a photo opened, resized, made
gray with more contrast, cropped, tinted and mirrored, each square showing
its image, and two saves ticked by an entry](assets/screenshot.png)

# Try it

On Linux or macOS:

    curl -fsSL https://github.com/hugoocoto/lineit/releases/latest/download/install.sh | bash
    lineit ~/.local/share/lineit/examples/hello.lua

On Windows, download `lineit-x86_64-pc-windows-msvc.zip` from the
[latest release](https://github.com/hugoocoto/lineit/releases/latest), unzip
it and run `lineit.exe`. On Linux there's an AppImage there too.

Or, with [Docker](https://docs.docker.com/get-docker/), in the folder to
work in, then open <http://localhost:6080>:

    docker run --rm -p 127.0.0.1:6080:6080 -v "$PWD":/work ghcr.io/hugoocoto/lineit:nightly

Press **Run** (F5). Right click on the canvas to add a square: type to
search and press Enter. Drag from an output to an input to wire them.

lineit is in alpha: expect breaking changes with any commit.

# What it does

- **Run, step or compile the same canvas.** Run it in the editor, go
  through it a square at a time, or compile it to a program that runs
  without the editor.
- **Compiled means compiled.** A canvas becomes a typed IR, and Compile
  turns that into Rust that cargo builds with every optimization: loops are
  loops, numbers are `i64` and `f64`. There's no interpreter in the program.
- **Typed, without writing types.** The types flow over the wires: an int
  and a float added are a float, and a string wired into a number is an
  error on the square, before anything runs.
- **Parallel by default.** A tick wired to several squares runs them as
  branches, on threads once compiled, and they meet again where their wires
  do.
- **Extended in Lua, in Rust, or with squares.** A module is a Lua file, a
  Rust type, or a selection of squares made a module of its own (Ctrl+G).
  Packages (`.lipkg`) share them, on every OS as they are.

![FizzBuzz in the lineit editor: a loop sends each index through three
modulo and equal checks, and three ifelse squares pick what to print; the
output below](assets/fizzbuzz.png)

The canvases in [`examples/`](examples) show the rest, each explained in
its corner.

# Using the editor

| Do this                              | To                                                   |
|--------------------------------------|------------------------------------------------------|
| Right click on the canvas            | Add a square: type to search, Enter adds it          |
| Drag from an output to an input      | Wire them (a data input takes one wire)              |
| Drag from a pin to the canvas        | Add a square there, wired to that pin                |
| Drag a plugged input                 | Take its wire, to put it elsewhere or drop it        |
| Click a wire, then Delete            | Remove it (Shift+click selects more)                 |
| Type in a field next to an input     | The value it has while nothing is wired to it        |
| Right click on a square              | Duplicate, copy, disconnect, make a subgraph, delete |
| Drag on the canvas, or Shift+click   | Select with a box, or add to the selection           |
| Right or middle drag / mouse wheel   | Pan / zoom                                           |
| Drop files on the window             | Canvases open; images become `image/open` squares    |
| Delete, Backspace                    | Delete the selection                                 |
| Ctrl+C, Ctrl+X, Ctrl+V, Ctrl+D       | Copy, cut, paste, duplicate                          |
| Ctrl+Z, Ctrl+Y (or Ctrl+Shift+Z)     | Undo, redo                                           |
| Ctrl+A, Escape                       | Select all, select none                              |
| Ctrl+S, Ctrl+Shift+S, Ctrl+O         | Save, save as, open                                  |
| Ctrl+T, Ctrl+Shift+T, Ctrl+W         | New tab, reopen closed tab, close tab                |
| Ctrl+Tab, Ctrl+PageUp/PageDown       | Next / previous tab                                  |
| Ctrl+G                               | Make a subgraph of the selection                     |
| F5, F10, Shift+F5                    | Run, step, stop                                      |
| Ctrl+,                               | Settings                                             |

(Cmd instead of Ctrl on macOS.) Hovering a pin shows its type, and while a
program runs, its value. Squares that make images show them, without
running anything. While stepping, the squares about to run have a yellow
outline. What's wrong with a square is written under it, in red.

The bar at the bottom runs, steps, compiles (More) and stops, and has
**Output** (what the program prints) and **File**.

# The language

**Squares and wires.** Each square is a module. Wires carry values, or
ticks: the signal that says what runs next. A program starts at its
`flow/entry` squares, and its `flow/clock` squares tick it again and again
until it's stopped (or a `flow/stop` square ends it).

**Ticks push, data is pulled.** A tick runs a square, which may tick the
next. A value is computed when it's read: pure squares (math, comparisons,
strings, images) have no ticks at all.

**Types.** `int`, `float`, `bool`, `str`, `list<T>`, `map<T>` (string keys),
`image`, and record types of your own. Generic squares (`math/add` takes any
number, `io/print` anything) get their types from what's wired to them, or
typed in their fields: `2` is an int, `2.5` a float.

**Branches.** A tick output wired to several inputs runs each as a branch,
in parallel, and goes on once they're all done. A square that two branches
reach is where they meet: it runs once, after all of them. The two sides of
an `ifelse` meeting again don't wait for each other: there's only one
branch. As in any parallel program, two branches that change one variable
at once can lose a change.

**Variables.** A `var/var` square declares one: its name, its type and its
value when the program starts. `var/get`, `var/set` and the `list/` and
`map/` squares use it by name. A name that starts with `::` is global; the
rest belong to their canvas, so each copy of a subgraph has its own.

**Subgraphs.** A canvas with `graph/input` and `graph/output` squares is a
module. Select squares and press Ctrl+G to make one: it goes to your
modules folder, and a square of it takes their place.

# Modules of your own

Modules are found in your modules folder (`~/.local/share/lineit/modules`
on Linux, `~/Library/Application Support/lineit/modules` on macOS,
`%APPDATA%\lineit\modules` on Windows), in a `modules` folder next to the
canvas, and in the packages you install. The editor reloads them when they
change.

**Lua**, in a `.lua` file that returns a table (this is
[`examples/modules/shout.lua`](examples/modules/shout.lua)):

```lua
return {
        doc = "The text in capitals, with as many ! as you say",
        inputs = { { "text", "str", default = "hello" }, { "times", "int", default = 3 } },
        outputs = { { "result", "str" } },
        eval = function(i)
                return { result = i.text:upper() .. string.rep("!", i.times) }
        end,
}
```

A module with tick pins has `tick(self, input, i)` instead, which returns
the outputs it sets and the tick outputs to fire; `self` is the square's
own table, kept between ticks. See [`crates/lineit-rt/src/lua.rs`](crates/lineit-rt/src/lua.rs).

**Subgraphs** are `.graph.lua` canvases, **record types** `.type.lua` files
(`return { fields = { { "x", "float" }, { "y", "float" } } }`, which brings
`record/make/NAME` and `record/split/NAME`), and **Rust modules** are types
in a package's crate: see below.

# Packages

A package is a folder with a `package.lua` (its name and version), its
modules in `modules/`, and optionally a cargo crate in `native/` with Rust
modules. `lineit pack` zips it, and anyone can install it, on any OS:
everything in it is source, and Rust modules are built by cargo the first
time a canvas uses them.

    lineit new-package mypackage --native   # a folder to fill in, with a Lua and a Rust module
    lineit pack mypackage                   # mypackage-0.1.0.lipkg
    lineit install mypackage-0.1.0.lipkg

Its modules are `mypackage/...` in the search. A package is code: only
install the ones you trust.

# The command line

    lineit [FILE...]                    the editor, with these canvases
    lineit run FILE [--for SECONDS]     run a canvas, without the editor
    lineit check FILE                   what's wrong with a canvas, if anything
    lineit compile FILE [-o OUT]        a program of the canvas, next to it
    lineit new-package NAME [--native]  make a package
    lineit pack DIR / install FILE      share one / install one
    lineit modules                      every module lineit finds
    lineit config                       where the config goes, and every setting

**Compile needs Rust**, from <https://rustup.rs> (1.85 or newer). The first
compile downloads and builds what the program needs; the next ones only
build the program. Lua modules in a compiled program need a C compiler, as
Rust on Windows already does.

# Configuration

**File > Settings** (Ctrl+,) has every setting, searchable, and applies
changes at once: they're saved to `settings.lua` in the config folder.
`config.lua` there, if you write one, is read before it, and is real Lua:
`lineit config` prints one with every setting. Themes: `dark`, `light`,
`nord`, `gruvbox`, or your own in `themes`.

Canvases are Lua tables, read without running anything that could do harm
(no libraries, limits on time and memory): opening a canvas someone sent is
safe. Running it runs what it says, as any program.

# Building from source

With Rust 1.88 or newer:

    git clone https://github.com/hugoocoto/lineit
    cd lineit
    cargo run --release -- examples/hello.lua

On Linux it needs the X11, xkbcommon and OpenGL development files
(`libx11-dev libxkbcommon-dev libgl1-mesa-dev` on Debian and Ubuntu). Lua is
built from source, with the C compiler Rust uses.

# More

- [`docs/internals.typ`](docs/internals.typ): how it works inside.
- [`docs/roadmap.md`](docs/roadmap.md): what was decided, what's done, what's next.
- [CONTRIBUTING.md](CONTRIBUTING.md): building, testing and sending
  changes. Security problems: [SECURITY.md](.github/SECURITY.md).
- License: GPLv3 or later ([LICENSE](LICENSE)). Your canvases are yours; a
  compiled program includes lineit's runtime, so it's GPL too.

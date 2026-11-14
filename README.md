# lineit

**A visual programming language: you wire squares on a canvas, run the
program in the editor, step through it, or compile it to a native program.**

[![CI](https://github.com/hugoocoto/lineit/actions/workflows/ci.yml/badge.svg)](https://github.com/hugoocoto/lineit/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/hugoocoto/lineit)](LICENSE)

![A program made in lineit: an entry ticks a loop from 1 to 5, typed in
its fields; each index goes into both inputs of a multiply, added from the
search with a wire from the loop, and a print shows the product. A post-it
note explains it. Run prints 1, 4, 9, 16 and 25](assets/demo.gif)

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

![The images example in the lineit editor: a photo opened, resized, made
gray with more contrast, cropped, tinted and mirrored, each square showing
its image, and two saves ticked by an entry; the output
below](assets/screenshot.png)

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
| Add a `note` square, double click it | A post-it to write on: Enter is a new line, Esc done |
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

## Vim mode

Everything above can be done from the keyboard. Turn on **Editor > Vim
mode** in the settings (or `:set vim_mode` in a canvas, `editor = {
vim_mode = true }` in `config.lua`) and the keys are vim's. The square with
a ring is the one they act on; the bottom bar shows the mode, what's half
typed and the command line. `:help` lists every key.

| Key                          | Does                                                            |
|------------------------------|-----------------------------------------------------------------|
| h j k l, arrows              | Go to the nearest square left, down, up, right (`3j` counts)    |
| w, b, `;`, `,`               | Downstream, upstream, next / previous square of the same source |
| 0, $, gg, G, Tab             | Start / end of the chain, first / last square, next square      |
| `/text`, n, N                | Find squares by module, field value or note                     |
| i, Enter, `3i`               | Type in a field (Tab for the next one, Esc or Enter to finish)  |
| a, o, O + h j k l            | Add a square beside this one (module search): free, after, before; `oj` / `ok` leave room for a second one, `oh` and `Ol` touch |
| dd, yy, p, P, u, Ctrl+R      | Delete, copy, paste, undo, redo                                 |
| v, V, then h j k l d y       | Select by moving, or the whole chain, and act on it             |
| H J K L                      | Move the selected squares a grid step                           |
| f, then 1-9, then h j k l, 1-9 | Wire: pick a pin of this square, go to the other, pick its pin; `x` removes the wires of a pin |
| gd, gt, gT                   | Open the subgraph, next / previous tab                          |
| `]c`, `[c`                   | Next / previous change in a diff view (see below)               |
| Ctrl+E/Y/D/U, zh zl zz zf, + - = | Pan, center on the focus, fit everything, zoom              |
| `:`                          | Commands: `:w` `:q` `:e` `:tabnew` `:run` `:step` `:compile` `:add math/add` `:set key=value` `:settings` `:subgraph` `:help` (Tab completes) |

`:q` refuses to close a tab with changes (`:q!` does it anyway); on the last
tab it quits. Ctrl+D and Ctrl+R are vim's here (duplicate is `yy` `p`, or
`:dup`); every other Ctrl shortcut above keeps working, and so does the
mouse. In the Settings tab, j and k move between the settings, Space flips
one, h and l change it, `i` types in a text, `r` resets, `/` searches.

# The language

![FizzBuzz in the lineit editor: a loop sends each index through three
modulo and equal checks, and three ifelse squares pick what to print; the
output below](assets/fizzbuzz.png)

**Squares and wires.** Each square is a module. Wires carry values, or
ticks: the signal that says what runs next. A program starts at its
`flow/entry` squares, and its `flow/clock` squares tick it again and again
until it's stopped (or a `flow/stop` square ends it).

**One wire into a data input.** A data input takes one wire (a tick input
takes any number): a canvas with two is an error, shown in red on the
square, under it, and on the wires, and `check`, Run and Compile refuse it.

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

**Functions.** `list/map`, `list/filter` and `list/reduce` call a subgraph for
each item of a list: its path is their `fn` param, which the editor lets you
pick from the subgraphs with the right pins (map: one input, one output;
filter: one input, a `bool` output; reduce: the total so far and the item in,
the new total out, starting from `initial`). Their types come from the
subgraph's ports, and a mismatch is an error on the square. A subgraph with a
`tick` input runs on ticks, once per item, and can have branches and variables
of its own (kept from item to item, one set for each square that uses it);
one without is read like a pure square. See `examples/list_functions.lua`.

## Shaders

`image/shader` runs a Godot shader (`.gdshader`, `shader_type canvas_item;`)
over an image, on the CPU, with no GPU or Godot needed. Its `file` param is
the shader's path (relative to the canvas); the shader's `uniform`s are the
inputs of the square (`source_color` vectors take `#rrggbb` or a color name,
other vectors take numbers: `0.5, 1`; a `sampler2D` takes an image), next to
`TEXTURE`, the image, and `TIME`, what `TIME` reads. Edit the file with any
editor: the square follows when it's saved, and what's wrong with it is written
under the square, with the line. `examples/shader.lua` runs a Bayer dither
palette filter.

What a `fragment()` sees is what Godot gives it: `UV`, `COLOR`, `TEXTURE`,
`TEXTURE_PIXEL_SIZE`, `FRAGCOORD`, `SCREEN_UV`, `TIME`..., the types (`vecN`,
`ivecN`, `uvecN`, `bvecN`, `matN`, arrays, structs), functions with `in`,
`out` and `inout`, loops, `switch`, `discard`, and the built-in functions of
the language (`mix`, `smoothstep`, `texture`, `texelFetch`, `pow`...).
`#define`, `#if` and the like work; `#include` doesn't. It reads `filter_nearest`
and `repeat_enable` on samplers (the default is linear, clamped, as in Godot's
2D). Not supported: other shader types (`spatial`, `particles`, `sky`), the
`vertex()` and `light()` functions (read, not run), samplers that aren't
`sampler2D`, `determinant` and `inverse`, `modf`, uniform arrays, and
overloads that differ only by their types. Where Godot and this disagree on
what is valid, this is the more lenient (`float x = 1;` works).

## Tests

A canvas tests itself with two squares:

- **`test/case`** is a test: it starts the program, like `flow/entry`, but
  only when its test runs, and nowhere else then. It has a field for each
  variable of the canvas: what's typed in one is what that variable starts with
  (empty: as it starts anyway); a list or a record is wired in. A normal run
  doesn't start there. Its menu (right click) fills its fields from the run
  (the values the variables have now: after a run, or stepping through one)
  or from how they start, to write a test from there.
- **`test/expect`** checks that its `value` is `expected` (typed in, or wired:
  from a variable the case gives, so one check serves every case) when a tick
  gets there, and goes on. In a normal run it only goes on.

**Run tests** (F6, in the Run button's menu, `:test` in vim mode, or
`lineit test FILE`) runs the program once for each `test/case`, and the tests
of its code squares. The **Tests** panel, next to Output, says how they went:
what failed first, each with what its case gives, what each failed check
expected and got, what went wrong, and what it printed; what passed below, a
line each. A click on a test or a check shows its square. Each `test/case`
says under it whether it passed.

**Debug**, on a test of the panel (`:debug NAME`, or `:debug` for the first
that failed), runs that test by steps from what it gives: F10 steps, F5 runs
on. While it runs, each variable shows its value under its `var/var` square and
on the pins of `var/get`; each `test/expect` it gets to says how it went, red
if it failed. [`tests/testing.lua`](tests/testing.lua) has two cases.

## Code squares

`code/function` is a function you write, in Lua, Python, Rust, C or C#, in its
square (click the code to type; Esc keeps it) or in a file next to the canvas
(its `file` param). It's read the way its language reads: its parameters are
its inputs, what it returns its outputs.

```lua
count = 0  -- a global: the square's state

--- Halves even numbers; counts and passes on the odd ones.
---@param n integer
---@param even fun(half: integer)
---@param odd fun(calls: integer, n: integer)
function collatz(n, even, odd)
    Shared.total = Shared.total + n
    if n % 2 == 0 then
        return even(n // 2)
    end
    count = count + 1
    return odd(count, n)
end
```

| | Lua | Python | Rust | C | C# |
|---|---|---|---|---|---|
| The function | the one not `local` | the one not `_named` | the one `pub fn` | the one not `static` | the one `public static` |
| Types | LuaLS `---@param` (none: what's wired) | hints (none: what's wired) | `i64`, `f64`, `String`, `Vec<T>`, records... | `int64_t`, `double`, `const char *`, `list_int`, `map_str`, records | `long`, `double`, `string`, `List<T>`, `Dictionary<string, T>`, records... |
| Outputs | `---@return T name` each | `-> int`, `-> tuple[...]`, a `NamedTuple` | a value, a tuple, a struct | a value, a struct | a value, a tuple, a class or record |
| Exits | `fun(half: integer)` | `Out["half": int]` | `impl FnMut(i64)`, `impl FnMut(Struct)` | `void (*even)(int64_t half)` | `Action<long>`, `Action<(long half, string tag)>`, or a `delegate` of the code |
| State | globals | module variables (`global count`) | the struct of `&mut self` | variables at the top | the `static` fields of its class |
| Globals `::total` | `Shared.total` | `Shared.total` | `shared: &mut Shared` | `Shared *shared` | `Shared.total` |
| Runs in | lineit (Lua 5.4) | `python3` | cargo | the C compiler, through cargo | `dotnet run` (.NET 10 SDK) |

- **Exits**: a parameter that is a function is a tick output, with its
  values as outputs. Calling it fires it, when the function returns, in the
  order called: called in a loop, it fires once for each, each with its own
  values. A function with exits returns nothing.
- **State**: each square keeps its own. lineit can read it and set it, so a
  square's result depends on its state and inputs only.
- A function with state, exits or globals to set (or nothing to return)
  runs on ticks: a `tick` input and, without exits, a `then` output. The
  others are pure, like `math/add`: they run each time their value is read.
- A file in another language than the square's, code that doesn't read,
  and what the compiler says are shown with the file's name and line.

**Tests.** Click the line under a code square's code to open its tests: a
table where each row is a call, with what it's given (inputs, state,
`::globals`) and what it should give (what it `fires`, its outputs, its state
after). A row with no state goes on from the one before, so rows test a
sequence. Values are typed as in fields; lists, maps and records as Lua
tables (`{1, 2}`, `{a = 1}`). The line says how they went; `lineit test FILE`
runs them all.

**More languages, from a file.** A language that runs with an interpreter
can be added without building lineit: a `NAME.lua` file in lineit's
`languages` folder (next to `modules`, in its data folder) or in a
`languages` folder next to the canvas. It names the interpreter and has a
small program, in that language, that tells lineit a function's signature
(the language reads its own code: lineit parses nothing) and runs it. See
[`examples/languages/javascript.lua`](examples/languages/javascript.lua),
which adds JavaScript (with Node.js) and is used by
[`examples/code_js.lua`](examples/code_js.lua), and
`crates/lineit-core/src/registry/code/file.rs` for what the program answers.

**Crashes and Stop.** In the editor and `lineit run`, Rust and C code runs
in a process of its own, as Python and C# and languages from files do: a crash in
it ends the run with an error, not lineit, and Stop ends a square stuck in a
loop (its process is killed; a Lua square, which runs in lineit, checks for
Stop as it goes). In the editor, what those processes write to stdout and
stderr shows in the Output.

**Printing** from a code square (`print`, `println!`, `printf`) goes to the
program's output, in order with its own lines.

`tests/code.lua`, `code_python.lua`, `code_rust.lua`, `code_c.lua` and
`code_csharp.lua` are the same canvas in each. Python needs `python3` (or
`python`) on the machine, Rust and C need cargo (and C a C compiler), C#
needs the .NET 10 SDK (`dotnet`), when running and in the compiled program.
The C# canvases are skipped by `cargo test` without `dotnet`.

**C#** code is a `.cs` file as it is: `using`s, namespaces, other classes and
`delegate`s of its own stay, and the one `public static` method is the
function (its tuple's names, or the fields of a class or record it returns,
name its outputs). `dotnet run` builds it the first time (several seconds, the
first ever longer) and keeps the build; each square then runs in an assembly
context of its own, so each has its own `static` state. `Shared.NAME` is a
static field lineit declares for each variable the code uses, wherever it
uses it. `#:package Name@version` lines at the top of the code (`dotnet run`'s
own) bring in NuGet packages, such as MessagePack. A compiled program runs
the file lineit kept in its cache folder (which isn't deleted while a
program or the editor is running it). Starts of the same code, by any
lineit, go one at a time (a lock file in that folder), as `dotnet run` can't
build it twice at once; lineit sets `DOTNET_NOLOGO`,
`DOTNET_CLI_TELEMETRY_OPTOUT` and the like for it, so a first run says nothing
of its own.

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
    lineit test FILE                    run the tests of its code squares
    lineit diff A B [--view]            what changed between two canvases
    lineit show FILE|REV:PATH...        open canvases, or files as git has them, read-only
    lineit text FILE                    a canvas as text, for git (see Diffs)
    lineit compile FILE [-o OUT]        a program of the canvas, next to it
    lineit new-package NAME [--native]  make a package
    lineit pack DIR / install FILE      share one / install one
    lineit modules                      every module lineit finds
    lineit config                       where the config goes, and every setting

**Compile needs Rust**, from <https://rustup.rs> (1.85 or newer). The first
compile downloads and builds what the program needs; the next ones only
build the program. Lua modules in a compiled program need a C compiler, as
Rust on Windows already does.

# Diffs

`lineit diff A B` says what changed from canvas A to canvas B, matching
squares by their `id`: squares added and removed, a square's module, fields
and params that changed, wires added and removed. Only the program counts:
the camera doesn't, and a square that only moved is listed apart ("moved
only"), not as a change. A field that has several lines (the `code` of a
code square, the `text` of a note) gets a line diff, with two lines of
context. The exit code is that of `diff(1)`: 0 the same (moves aside), 1
different, 2 an error.

    removed square 4 io/print
    added square 5 math/add
        field a = "1"
    changed square 2 io/print
        field value: "hi" -> "bye"
    changed square 3 note
        param text changed:
              f
            - g
            + G
              h
    removed wire 1.then -> 2.tick
    added wire 1.then -> 5.tick
    moved only (1): 1 flow/entry

A or B can be `REV:path` (`HEAD:examples/hello.lua`): the file as git has it
(`git show REV:path`: the path is from the root of the repository, and
`REV:./path` is relative to where you are). Options other than `--view` are
errors.

**Subgraphs.** When the files are `.graph.lua` subgraphs, the diff ends with
where the new one is used: the canvases with squares of that module, in the
folder that has the `modules` folder (or the file's own), down to four
folders. Those squares are what a change to the subgraph reaches.

**In the editor**, `lineit diff --view A B` opens both on one canvas, in a
read-only tab: added squares and wires are green, removed ones red, changed
ones amber (theme colors `added`, `removed`, `changed`), squares with a wire
added or removed are outlined paler amber ("wires changed"), and squares that
only moved are dimmed. Squares and wires that changed are drawn where they
are in B, removed ones where they were in A. In vim mode, `]c` and `[c` go
to the next and previous change (left to right, top to bottom, wires
changed included; `3]c` counts). `lineit show REV:path` opens a file as git has it in a read-only
tab. Both open a new editor window, and neither can be changed, saved, run or compiled (the bar says so), nor
is kept when the editor rebuilds itself.

**With git.** Three ways, all in your git config:

    # git diff / git log -p print what changed in the program, not the Lua
    # (.gitattributes)
    *.lua diff=lineit
    # (git config)
    git config diff.lineit.textconv "lineit text"

    # git difftool: the diff in the editor, one canvas
    git config difftool.lineit.cmd 'lineit diff --view "$LOCAL" "$REMOTE"'
    git difftool -t lineit HEAD~1 -- examples/hello.lua

    # a file at a revision, read-only
    lineit show HEAD~3:examples/hello.lua

`lineit text FILE` prints a canvas as text: a line per square and wire in the
order of the ids, and multi-line fields on lines of their own, without
positions or camera, so git's line diff shows only program changes. Only use
`*.lua diff=lineit` in a project where every `.lua` is a canvas: others (and
`*.type.lua`, `*.native.lua`, other Lua modules) aren't, and `lineit text`
fails on them; name the canvases instead, like `*.canvas.lua`.

**Ids.** The diff is only as good as the ids, so the editor keeps them
stable. It never renumbers a square. A pasted square keeps its id if no
square on the canvas has it, so cut and paste changes nothing in a diff;
a copy (the original is still there) gets a new one. An id is never given
twice: the canvas file keeps `next_id`, one more than the highest id ever
used, when a square was deleted (older files, and files without deletions,
don't have it: the highest id plus one is it). So a square that replaces
another is an addition and a removal, never a "change of module". Ctrl+G
takes squares out of the canvas into the subgraph, keeping their ids there,
and puts one new square in.

# Configuration

**File > Settings** (Ctrl+,) opens the Settings tab: every setting, in
sections, with a search (Ctrl+F) and a Reset for each. It applies
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

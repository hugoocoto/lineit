# lineit in Rust: the roadmap

The C version (up to c344064) proved the idea: wire squares on a canvas,
run them, step through them, compile them. This rewrite keeps the language
and redoes the inside, with three aims:

1. **Linux, macOS and Windows** from the same code, with as few external
   dependencies as possible.
2. **A real compiler.** A canvas goes through a typed IR. Run and Step
   interpret that IR, and Compile turns it into Rust that cargo optimizes. A
   compiled program is the program itself, not an interpreter carrying a
   canvas.
3. **Packages that are easy to make and work everywhere**: zips of Lua
   modules, subgraphs and types, with optional Rust modules built from source.

## Decisions

| Question                | Decision                                                                    |
|-------------------------|-----------------------------------------------------------------------------|
| Language                | Rust (stable), one cargo workspace                                          |
| GUI                     | egui / eframe with the glow (OpenGL) backend                                |
| Lua                     | mlua with a bundled Lua 5.4: config, canvases, Lua modules                  |
| Canvas files            | Lua tables, read in a sandbox (no libraries, limits on time and memory)     |
| Config                  | `config.lua` and `settings.lua`, ordinary Lua                               |
| Compile                 | IR to Rust, built with `cargo build --release` against `lineit-rt`          |
| User modules            | Lua modules, subgraphs, record types, and Rust modules (optional)           |
| Packages                | `.lipkg` zip files, made with `lineit pack`                                 |
| Types                   | Static: inferred over the wires, checked before running                     |
| Execution model         | Ticks push and data is pulled, as before; fan-outs are parallel branches    |
| Old canvases or modules | Not supported: a clean break                                                |
| Distribution            | GitHub Actions on all 3 OSes; release archives, AppImage, Docker (noVNC)    |
| File dialogs            | A path prompt inside the editor with Tab completion (no native dialog crate)|

Dependencies: `eframe` (the editor), `mlua` (Lua), `zip` (packages), `image`
(only to read and write image files, as an optional feature of
`lineit-rt`), `libloading` (Rust modules in the editor). Everything else is
std: the argument parser, the file prompt, threads and the codegen. A
compiled program depends only on `lineit-rt`, plus `image` if it opens or
saves images, `mlua` if it uses a Lua module, and the crates of the Rust
modules it uses.

## Workspace

```
Cargo.toml                 workspace
crates/lineit-rt/          the runtime: values, strings, lists, images, branches,
                           Lua and Rust modules. Shared by the interpreter and the
                           compiled programs.
crates/lineit-core/        everything but the GUI:
    model.rs               the canvas (the AST): squares, links, fields, params
    file.rs                reads and writes canvases (sandboxed Lua tables)
    registry/              the modules: built-in, Lua and Rust, subgraphs, records
    types.rs               the inference over the wires
    lower.rs               canvas -> IR (subgraphs inlined, types resolved)
    ir.rs                  the IR
    interp.rs              runs the IR: Run and Step, cooperative branches
    codegen.rs             IR -> a Rust crate, built by cargo
    package.rs, native.rs  .lipkg packages; Rust modules built and loaded
    config.rs, paths.rs    config.lua and settings.lua; where things go per OS
crates/lineit/             the binary: the CLI and the editor (editor/)
examples/, tests/          canvases, with the expected output (.out)
scripts/                   install.sh, the release archives, the AppImage, Docker's entrypoint
```

## The language

**Squares** are instances of modules. A module declares **pins** (inputs and
outputs) and **params** (settings at the bottom of the square). Pins carry
**ticks**, the signal for what runs next, or **data** of a type. A pin's
types can depend on the square's params: a `value/constant` set to `float`
outputs a float, and a `string/format` with 3 args has 3 inputs.

**Types**: `int` (i64), `float` (f64), `bool`, `str`, `list<T>`,
`map<T>` (keys are strings), `image`, and records (`point`, ...).
Modules can be generic: `math/add` is `(T, T) -> T` with `T` a number,
`io/print` takes any `T`. Inference collects what flows into each type
variable (wires, and the literals typed in fields: `2` is an int, `2.5` a
float), takes the join (`int ⊔ float = float`, which widens the int), and
checks every wire. An error shows on the square and the wire, and Run and
Compile refuse the canvas until it's fixed.

**Ticks push.** A square's tick input runs its code, which sets its outputs
and fires its tick outputs. Firing an output with one wire runs the square at
the other end, then returns. **Data is pulled.** Reading an input computes
the pure squares behind it, every time it's read (they're inlined as
expressions). A square that runs on ticks keeps its outputs in slots that
hold their last value.

**Branches.** A tick output with several wires runs each target as a
branch, in parallel, and goes on once they've all finished. A square that
two or more branches of the same fan-out can reach is where they **meet**:
a branch that gets there stops, and once all of them are done the square
runs once, if any of them got there. The two sides of an `ifelse` that wire
into the same square aren't branches, so they don't wait for each other.

**Loops.** `flow/loop`, `flow/while` and `list/foreach` loop inside one
square. A tick wired back into a square it came from is a loop too: tail
calls don't grow the stack, in the interpreter or in compiled code.

**Variables.** `var/var` declares one: its name, its type and its initial
value. `var/get`, `var/set` and the `list/` and `map/` squares use it by
name. Names starting with `::` are global; the rest belong to the canvas
they're on, so each copy of a subgraph has its own.

**Subgraphs.** A canvas with `graph/input` and `graph/output` squares is a
module. Each square of it is inlined at lowering, which makes it as fast as
if it were drawn in place. A subgraph that contains itself is an error.

**Clocks.** `flow/clock` fires every `interval` seconds. A program with no
clock ends once its entries are done; one with a clock runs until it's
stopped.

## The IR

```text
Program  { slots, handlers, starts, clocks }
Slot     { type, init, name }               node output slots, variables
Handler  { node, body: Block, in_cycle }    one per (square, tick input) reached
Stmt     Enter(node)                        a square starts: Step stops here
         Set(slot, Expr)
         Op(RtOp, [Expr], [slot])           runtime calls with effects: print, push...
         If(Expr, Block, Block)
         While(Expr, Block)
         For(slot, Expr, Expr, Block)
         Fire([handler])                    one: a call; several: branches, then meet
         Ext(...)                           a Lua or native module
Expr     Const | Slot | Unary | Binary | Call(RtFn, [Expr]) | Convert | Ext
```

Built-in modules don't run code of their own: they **lower** to IR. A
`flow/loop` becomes `For`, a `math/add` becomes `Binary(Add)`, a
`string/upper` becomes `Call(upper)`. That's why the compiled program is
plain Rust loops and arithmetic: there's nothing left to interpret. The
interpreter and the generated code call the same functions in `lineit-rt`
for anything that isn't a basic operation.

**Interpreter** (Run, Step). Each branch is a task with its own stack of
frames over the IR. Tasks run in turns on one worker thread, so stepping is
deterministic: Step lets every live task run up to its next `Enter`, and the
squares they stop at get an outline. Run goes on until the program ends or
Stop is pressed. The editor gets output, the current squares and the values
of slots for tooltips and previews through a channel. Real parallelism is
the compiled program's job.

**Codegen** (Compile). Each handler becomes a function. A set of handlers
that tick each other in a cycle becomes one function looping over a `match`,
so a tick back into the cycle is a jump. Fan-outs become
`std::thread::scope`. A program with no fan-out keeps its state as plain
fields in a struct passed as `&mut`; with fan-outs, numbers and bools are
atomics (relaxed order) and the rest are behind `Mutex`es. Types are
concrete: `i64`, `f64`, `String`, `Vec<T>`, generated structs for records.
Output: a crate in `~/.cache/lineit/build/<name>`, built with `cargo build
--release`, and the binary copied next to the canvas.

## Modules and packages

**Built-in modules**, in Rust, in `lineit-core/src/registry/builtin/`:

- `flow`: entry, clock, loop, while, ifelse, counter, select, sequence
- `math`: add, subtract, multiply, divide, modulo, power, absolute, min, max,
  clamp, random, negate, floor, ceil, round, sqrt
- `compare`: equal, not_equal, less, less_equal, greater, greater_equal
- `logic`: and, or, not, xor, nand, nor, xnor
- `value`: constant, convert
- `var`: var, get, set
- `list`: push, pop, get, set, length, clear, remove, foreach
- `map`: set, get, has, delete, keys, length
- `string`: concat, length, upper, lower, find, substring, split, join, format
- `io`: print, display
- `image`: open, save, new, size, pixel, grayscale, invert, flip, rotate,
  resize, crop, blur, brightness, contrast, tint
- `graph`: input, output (the pins of a subgraph)
- `record`: make and split, for every record type

**Lua modules**, in a `.lua` file that returns a table:

```lua
return {
    doc = "Reverses a string",
    inputs = { { "text", "str", default = "" } },
    outputs = { { "result", "str" } },
    eval = function(i) return { result = i.text:reverse() } end,
}
```

A module with tick pins has `tick(self, input, i)` instead, which returns
the outputs to set and the names of the tick outputs to fire; `self` is the
square's own table, kept between ticks. Types are concrete (no generics).
In a compiled program, mlua runs them.

**Rust modules** (in packages): types in the crate of the package's
`native/` folder that implement `lineit_rt::native::Native`, listed with
`lineit_rt::export_modules!`. Their pins are in `modules/NAME.native.lua`
(the same table as a Lua module's, with `native = "Type"`), so lineit knows
them without building anything. Compile adds the crate as a dependency of
the program and calls it directly. The editor builds it as a `cdylib` with
cargo the first time it runs one, and loads it through the C ABI that
`export_modules!` makes, values as bytes.

**Packages**: `name.lipkg`, a zip of

```text
package.lua      name, version, description, author, license
modules/         *.lua (Lua modules), *.graph.lua (subgraphs), *.type.lua (records)
native/          optional: a cargo crate
```

`lineit new-package NAME [--native]` makes a folder ready to fill in,
`lineit pack DIR` zips it into `NAME-VERSION.lipkg`, `lineit install FILE`
puts it in the packages folder (`~/.local/share/lineit/packages`, the OS's
data folder on macOS and Windows), and lineit loads every package there.
Only Rust modules need building, by cargo, cached. No checksums of our own:
the zip checks every file against its CRC, and a checksum inside the same
file can't stop anyone who changes it.

## Milestones

Each milestone ends with a build, tests that pass, and a working tree to
commit. All of them are done (November 2026), checked by the tests (every
canvas in `tests/` and `examples/` interpreted and compiled, on Linux here;
CI does the three OSes) and by driving the editor on a virtual screen.

**M0: workspace.** Cargo workspace, the three crates, CI on 3 OSes
(`cargo test`, `cargo clippy`), the old C tree removed.

**M1: model and files.** Canvas types; reading and writing canvases as
sandboxed Lua tables; `config.lua`/`settings.lua`; tests that a canvas
written and read again is the same.

**M2: registry and types.** The module trait (pins and params from the
params), the core built-ins with their signatures, type inference and its
errors, `lineit check FILE`.

**M3: IR and interpreter.** Lowering (pure squares inlined, slots, handlers,
meets, cycles, subgraphs inlined), the interpreter with tasks, `lineit run
FILE`, the canvases in `tests/` with their `.out` files, and the examples.

**M4: codegen.** IR to Rust, the runtime crate embedded in the binary and
written out next to the generated crate, `lineit compile FILE`, a test that
compiles every test canvas and compares its output with the interpreter's.

**M5: the editor.** eframe app: canvas (pan, zoom, grid), squares, pins,
wires, fields and params, the module search (right click, drag a wire to
the canvas), the context menus, the bottom bar (Run/Step/Compile/Stop,
Output, File), the path prompt, type errors shown on the canvas, Step
outlines, tooltips with values.

**M6: editing.** Selection (box and Shift), move, delete, copy, cut, paste,
duplicate, undo and redo, tabs, themes (dark, light, nord, gruvbox),
settings from the GUI saved in `settings.lua`, autosave.

**M7: subgraphs, records, Lua modules.** Ctrl+G makes a subgraph of the
selection, `.type.lua` records with their make/split squares, Lua modules in
the editor and in compiled programs.

**M8: images.** The `image` family, previews on squares, and dropping image
files on the canvas.

**M9: packages.** `.lipkg`, `lineit new-package`, `lineit pack`, loading and
checksums, Rust modules (build, load, link when compiling).

**M10: shipping.** README, docs (`docs/internals.typ` rewritten), the
release workflow (archives for 3 OSes, AppImage, Docker with noVNC),
`install.sh`.

## Later

- A faster interpreter: today it's about 60 ns a loop iteration and 230 ns
  a square called (compiled programs are about a thousand times faster).
  Registers and typed slots instead of `Value`s would help most.
- Branches of compiled programs on a thread pool, not a thread per branch
  of each fan-out
- Opening a file in a running editor's new tab (`lineit -t`)
- Vim mode
- Real threads in the interpreter
- Editing while running applied to the running program (today the program
  goes on with the canvas it started with)
- `list/map`, `filter`, `reduce` with a subgraph as the function
- FFI to C libraries
- A native file dialog (today: a path prompt with Tab completion)
- On Windows, no console window when the editor is opened by a double click

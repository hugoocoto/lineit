// lineit internals: how a canvas becomes something that runs.
// Build it with: typst compile docs/internals.typ

#set document(title: "lineit internals", author: "Hugo Coto Flórez")
#set page(paper: "a4", margin: (x: 2.2cm, y: 2.4cm), numbering: "1")
#set text(size: 10.5pt, lang: "en")
#set par(justify: true)
#set heading(numbering: "1.1")
#show heading.where(level: 1): it => {
  pagebreak(weak: true)
  v(0.6em)
  it
  v(0.3em)
}
#show raw.where(block: true): block.with(fill: luma(245), inset: 8pt, radius: 3pt, width: 100%)
#show raw.where(block: false): box.with(fill: luma(240), inset: (x: 2pt), outset: (y: 2pt), radius: 2pt)

#align(center)[
  #text(size: 22pt, weight: "bold")[lineit internals]
  #v(0.2em)
  #text(size: 12pt)[How a canvas becomes something that runs]
  #v(0.2em)
  #text(size: 10pt, fill: luma(90))[For lineit 0.1 (the Rust one)]
]

#v(1em)

This document explains how lineit works inside: what a canvas is, how it's
checked and lowered to an IR, how the interpreter runs that IR (Run and
Step), how the codegen turns it into a Rust program (Compile), and how
modules that aren't built in (Lua, Rust, subgraphs, records) fit in. It
follows the source: when it names a function or a type, it's the one in the
code, so you can read them side by side.

#outline(depth: 2, indent: auto)

= Overview

Three crates:

- `lineit-rt`, the runtime: what the interpreter and compiled programs share.
  Values and how they print, strings, lists, images, branches, Lua modules,
  Rust modules. It must build on its own, with no dependency unless a feature
  asks for one (`image`, `lua`, `shader`), because lineit writes its source out next to
  every program it compiles (`codegen::write_runtime`).
- `lineit-core`, everything but the GUI.
- `lineit`, the command line and the editor (egui).

A canvas goes through these steps, all in `lineit-core`:

+ *Read* (`file::read_canvas`): the Lua table of the file, in a sandbox, into
  a `model::Canvas`.
+ *Flatten* (`lower::flatten`): subgraphs replaced by their squares, wires
  joined through their ports, each square's signature from its params.
+ *Types* (`types::infer`): what flows on each wire.
+ *Lower* (`lower::lower`): the `ir::Program`.
+ *Run* it (`interp::Interp`) or *compile* it (`codegen::compile`).

The editor runs the first four every time the canvas changes: they're how it
knows a square's pins, the types on its wires and what's wrong with it.

= The canvas <sec-canvas>

`model::Canvas` is the program's syntax tree: squares (`Square`: an id, a
module path, a position, its params and the text in its fields) and links
(`Link`: from an output to an input, both by name). Pins are named, not
numbered, so a canvas still opens after a module gains or reorders pins; a
link to a pin that isn't there is left out when lowering, not an error.

Ids are stable, so two versions of a canvas can be compared by them (see
@sec-diff). `Canvas::next` is one more than the highest id any square has had:
`Canvas::add` raises it and removing a square doesn't lower it, so
`Canvas::next_id` never gives an id twice. The editor's paste keeps a square's
id when no square on the canvas has it, and undo and redo keep `next` as it is.

== Files

A canvas file is Lua that returns a table:

```lua
return {
        lineit = 1,
        doc = "What it does",
        camera = { x = 0, y = 0, zoom = 1 },
        squares = {
                { id = 1, module = "flow/entry", x = 0, y = 0 },
                { id = 2, module = "io/print", x = 280, y = 0, fields = { value = "Hi" } },
        },
        links = { { 1, "then", 2, "tick" } },
}
```

A canvas that had squares deleted also has `next_id = N` (the id after the
highest one used): older lineits ignore it, and a file without it means the
highest id plus one, so the format number didn't change.

`file::with_data` reads it: a Lua state with no libraries at all, an empty
environment, a memory limit (`file::MEMORY`) and an instruction limit
(`file::INSTRUCTIONS`, with a hook every 10 000 instructions). So opening a
canvas can't do anything but describe one. The same reads record types and
package manifests. `lineit` (the format) is `file::FORMAT`: a file from a
newer lineit says so instead of being misread.

= Modules <sec-modules>

A module (`registry::Module`) says:

- its *params* (`ParamDef`: a name, a kind, a default);
- from their values, its *signature* (`Signature`): its inputs and outputs,
  each a tick or data of a type expression (`TExpr`), with a field and its
  default text for the data inputs that can be typed;
- how it *lowers*: `eval` gives the expression of a data output, `tick` the
  statements a tick input runs, `start` what it runs when the program
  starts, `clock` the interval and statements of a clock.

The signature can depend on the params and on the variables in scope
(`SigCx`): `value/constant` set to `float` outputs a float, `string/format`
has an input per `{}`, `list/push` takes a value of the type of the list it
names. A type expression is a type, or a type variable of the square
(`Var(n)`, with a `Class`: any, number, ordered), or a list or map of one.

The `Registry` has every module by path. `Registry::builtin` makes the
built-in ones (`registry/builtin.rs`); `Registry::load_dir` loads a folder:
`*.type.lua` are record types, `*.graph.lua` subgraphs, `*.native.lua` the
pins of Rust modules, and the other `*.lua` Lua modules.
`lineit_core::registry` puts together the ones a canvas sees: built in,
installed packages, the user's modules folder, and `modules/` next to the
canvas.

== Built-in modules

None of them runs code of its own: each lowers its square to IR.
`flow/loop` is a `For` statement, `math/add` a `Bin(Add)` expression,
`string/upper` a `Call(Upper)`. That's what makes Compile real: there's
nothing left to interpret, only IR, which becomes plain Rust. Anything that
isn't arithmetic is a function in `lineit-rt`, which the interpreter and the
generated code both call.

= Flattening <sec-flatten>

`lower::flatten` walks the canvas and every subgraph in it into one graph
(`GNode`s). A subgraph square is replaced by the squares of its canvas, in a
scope of their own; its `graph/input` and `graph/output` squares become
*pass* nodes, whose `value` goes out as it comes in. A wire into the
subgraph square's input `x` goes into the pass node of the port `x`; a wire
from its output `y`, out of the pass node of `y`. A value typed in the
subgraph square's field is the pass node's field, and keeps the port's type
(`Source::Field(text, Some(type))`): `3` typed into a `float` port is a float
inside.

Then every real node gets its signature (after the variables: `var/var`
squares, by scope), and each data input its `Source`: a wire, followed back
through pass nodes to a real output; a field; or nothing. Each tick output
gets its targets, followed forward. A data input (of a real square, or a
subgraph's, which is a pass node) with more than one wire is an error:
`Canvas::connect` replaces the old wire, so only a file written by hand, or
an old one, can have two. Tick inputs take any number. A subgraph that contains itself is an
error on its square.

What goes wrong is a `Diag` on a square of the canvas: the square itself, or
the subgraph square it's inside of.

*Functions.* `list/map`, `list/filter` and `list/reduce` have a param of kind
`ParamKind::Function { args }`: the path of a subgraph. `Flattener::function`
inlines it once, in a scope of its own (so its variables are per square that
uses it), right where the square is. Its data `graph/input` ports get their
`incoming` from the calling square's outputs named in `args` (`value`, or
`acc` then `value`), and a `tick` port from its `call` output: from then on
`source()` and `targets()` walk through them as through any subgraph. Its data
`graph/output` ports go to `FNode::fn_outs` (the `Source`, the port's type and
name), which `types::infer` checks (`the function's output y is int, and it
gets str`). The signature comes from the subgraph's ports
(`SigCx::function`, `Subgraph::fn_ports`), all of it concrete types: wrong
port counts, a filter that doesn't give a bool or a reduce whose accumulator
and output differ are errors on the calling square. A function with no tick
input is *pure*: the loop reads `Lx::fn_result` after setting the `value`
slot. One with a tick input runs on ticks: the loop `Fire`s `call`, which
returns when everything it ticked has finished (branches and meets as
anywhere), and then reads the outputs. A square with a tick pin inside a
function with no tick input is an error, as nothing would run it.

= Types <sec-types>

`types::infer` gives every pin of every flat node a type:

+ Each type variable gets the *join* of what flows into it: the types of the
  outputs wired to it, and the literals typed in fields (`2` is an int,
  `2.5` a float, `true` a bool, the rest a string). An int and a float join
  as a float; anything else that differs is an error.
+ This goes on until nothing changes, through squares (an `add` of two
  `add`s).
+ A variable nothing flows into takes the type of what it flows to (a
  `random` wired into a float input is a float), and failing that a default:
  int for numbers, str for anything.
+ Every variable is checked against its class, and every wire and field
  against its input: a wire can carry its own type, or an int into a float
  (it widens).

= The IR <sec-ir>

`ir::Program`:

- *slots*: state. The outputs of squares that run on ticks keep their value
  in one, and so does each variable.
- *handlers*: one for each tick input something fires, with its locals and
  its statements.
- *starts*: what runs when the program starts, and *clocks*.
- the Lua and Rust modules it uses (`exts`) and their instances.

Statements (`Stmt`) set slots and locals, print, change lists and maps,
branch (`If`), loop (`While`, `For`), fire handlers (`Fire`), stop the
program, and call Lua or Rust modules (`Ext`). Expressions (`Expr`) are
constants, slots, locals, operators and calls. Every expression has a type
(`Expr::ty`), which the codegen needs.

== Lowering

`lower::lower` asks each module to lower its square, through `Lx`:

- `Lx::input` is the expression of a data input: what's wired to it, pulled
  (`Lowerer::pull`: the source square's `eval`, kept so it's built once), or
  its field. A pure square becomes an expression inside the statements that
  read it. A square that runs on ticks is a slot read.
- `Lx::fire` fires a tick output: its targets become handlers
  (`Lowerer::handler`), lowered from a work list, so cycles are fine.
- `Lx::out_slot`, `Lx::var`, `Lx::local` give state.

== Branches and meets

A `Fire` with several targets runs them as branches. Its *meets*
(`Program::find_meets`) are the handlers that two or more of them can reach:
a branch that gets to one stops there, and once they're all done, each meet
that any reached runs once. Meets behind other meets are left out: a branch
stops at the first meet it gets to, so it can't reach the ones after. This
is what makes three pushes wired into one print run the print once, after
all three, while the two sides of an `ifelse` wired into one square don't
wait for each other (they're not branches).

= Running <sec-interp>

`interp::Interp` runs a `Program`. Each handler is first compiled into ops
with jumps (`compile`): `If`, `While` and `For` become `JumpIfNot`, `Jump`,
`ForInit`, `ForCheck` and `ForNext`, with a hidden local for the end of each
`For`. A branch is a `Task`: a stack of frames, each a handler, an op index
and its locals. Tasks take turns on one thread, so the same canvas always
runs the same way, and Step can stop each before its next square (`Enter`).

`run_ops` is the loop: one task, until its budget is spent, it's done, or it
fans out. Calls push a frame; a call that's the last thing its handler does
replaces it (a tail call), so a loop made of wires runs in constant space.
A fan-out (`Interp::fire`) makes a task per target and parks the caller
until they're done (`settle`), then runs the meets they reached.

The program's state is `World`: the slots, the Lua and Rust modules, what it
printed. Expressions are evaluated recursively (`World::eval`), with the
common cases small so the recursion is cheap.

The editor runs it on a thread of its own (`editor/runner.rs`), and reads
what it prints, which squares it's stopped before and the values of the
slots from a `View` a few times a second.

== Editing while running

When the canvas changes under a live run, `Tab::offer_edit` compares it with
the canvas the run goes by (`diff::diff`: moving squares isn't an edit; a
file the canvas depends on that changed on disk is, `Tab::files_generation`).
If it lowers, the runner gets the new `Program` (`Runner::offer`, a
`Cmd::Swap` to wake it); if not, the view says the old program goes on. The
canvas the run goes by only changes when the runner says it applied the edit
(`View::applied`), so a failed or discarded edit is not taken for done.

The runner thread calls `Interp::swap` as soon as `Interp::can_swap` (no
live task, not stopped): after a `run`, while it waits for a clock, or after
a Step. Branches on threads are scoped to `fan_out`, so they're all joined by
then: nothing else touches the `World`, and a read-and-change can't be cut in
two. A program that never goes idle gets "apply now" (`Runner::restart`): it
sets `World::interrupt`, which every branch checks like Stop but without
ending the program; `fan_out` joins them, `Interp::abandon` drops the tasks,
and `swap` and `Interp::restart` (the entries again) follow.

`swap` works everything out before it gives up the old program, and undoes
what it can if something fails. Slots are told apart by `slot_keys`: where
they show on a square (`SlotDef::shows`) and the variable's name
(`Program::vars`), counted when a name repeats; a slot with neither starts
again (and is counted in `Swapped`). The same type: the value moves; another:
`registry::builtin::convert` (what `value/convert` lowers to) is evaluated on
it, or it starts again. Modules (`ExtDef`) with the same path and kind move
over with their `Handles`, so a Python process isn't restarted by an edit
elsewhere, and an instance with the same square and params keeps its state.
The modules `closes_first` (a C# build holds a lock, a Rust dylib is
loaded by path) end before their replacements open, and if that fails they
open again without their state. Clocks are told apart by `Clock::square`:
they keep their next time, or now plus the new interval if that's sooner.
Entries that weren't there (`new_entries`) run. `Interp::live` is the cell
the thread that shows what the program does, and kills its processes on
Stop, reads the world from; it follows each swap.

= Compiling <sec-codegen>

`codegen::generate` turns a `Program` into the `main.rs` of a crate that
depends on `lineit-rt`; `codegen::compile` writes the runtime and the crate
in the cache folder and builds them with `cargo build --release` (fat LTO,
one codegen unit, `panic = "abort"`). All programs share one target folder,
so the runtime and its dependencies are built once; each crate's name has a
hash of where its canvas is, so two canvases with the same name don't share
a binary.

- Each handler is a function. Handlers that fire each other in a cycle (the
  strongly connected components of the calls, `cycles`) are one function
  that loops over a `match`: a tail call inside the cycle is
  `st = H; continue`, so it doesn't grow the stack.
- Types are Rust's: `i64`, `f64`, `bool`, `String`, `Vec<T>`,
  `BTreeMap<String, T>`, `lineit_rt::Image`, and a struct per record type.
- A program with no fan-out keeps its state as fields of a struct passed as
  `&mut`, `slot::Local<T>`. With fan-outs, branches run on threads
  (`branch::fan_out`, `std::thread::scope`) and the state is shared: numbers
  and bools as relaxed atomics, the rest behind mutexes (`slot::Shared`).
  The generated code is the same either way; `with` and `with_mut` release
  their lock before they return, and arguments are computed first, so no
  statement holds two locks.
- Meets: each thread keeps the fan-outs it's inside of
  (`branch::arrive`), and a branch that gets to a meet stops there.

What isn't plain Rust calls the same functions the interpreter calls, so the
two can't disagree: `tests/` and `examples/` are run both ways in CI, and
must print the same.

= Lua and Rust modules <sec-ext>

Both are `lineit_rt::Ext`: make an instance per square, `eval` a pure one,
`tick` one with tick pins, by the names of their pins.

*Lua modules* (`lineit_rt::lua`) are a file that returns a table: its pins,
and `eval`, `tick` or `start`. The interpreter and compiled programs run
them with mlua (Lua 5.4, built from source), converting values both ways by
their declared types.

*Rust modules* are types in a package's crate that implement
`lineit_rt::native::Native`, listed with `export_modules!`. Their pins are in
the package's `NAME.native.lua`, so lineit knows them without building
anything. A compiled program depends on the crate and calls it directly
(`native::Host`). The interpreter builds it as a dynamic library, the first
time it's used (`lineit_core::native::build`), and calls it through a small
C ABI: values go as bytes (`native::encode`, `native::decode`), so the
editor and the library need the same `lineit-rt` (lineit patches it in, with
cargo's `--config`), not the same compiler. `native::ABI` is the version of
that ABI.

= Packages <sec-packages>

A package (`package.rs`) is a folder with `package.lua` (name, version,
description), `modules/` and optionally `native/`; `lineit pack` zips it
into a `.lipkg`. Installed packages are in the packages folder; each is
unzipped once into the cache (its folder named with a hash of the file) and
its modules loaded as `NAME/...`. A folder with a `package.lua` in the
packages folder is loaded as it is, for making one. The zip checks every
file with its CRC as it's read.

= Diffs <sec-diff>

`diff::diff` compares two canvases by square id and returns a `Diff`: squares
added and removed, squares in both whose module, fields or params changed
(each with the entries that did), squares that only moved, and wires added and
removed. Camera, `doc` and positions aren't the program. `diff::lines` is the
line diff of a multi-line value, by the `similar` crate, with a CRLF and a
missing last newline made visible; `diff::outline` is a canvas as text (no
positions), which `lineit text` prints for git's textconv, and keeps those
differences too, so `git diff` and `lineit diff` agree on what changed.

`diff::merge` puts both on one canvas for the editor: the squares and wires of
the new canvas, and the ones only the old had, with a `Mark` for each square
(added, removed, changed, wired: a wire to or from it changed, moved). A tab
made by `Tab::view` has `label` (it's read-only) and `diff` (the merge). The
removed wires are drawn, but lowering leaves them out, and the diagnostics of
removed squares, so they can't show false errors. `]c` and `[c` go through
`nav::change` over the squares the merge calls changes, in the order of
`nav::order`.

`diff::uses` finds where a subgraph is: it reads the canvases under a folder
(four levels, no index) for squares of that module; `lineit diff` calls it
for `.graph.lua` files. `lineit diff A B` also takes `REV:path`, read with
`git show` and placed in the working tree from `git rev-parse --show-toplevel`
(or from here for `REV:./path`).

= The editor <sec-editor>

`editor/app.rs` is the window: tabs, the bar, the output, the module search,
the path prompt and the settings. `editor/canvas.rs` draws a tab's canvas
and handles the mouse. Squares, pins, fields and the canvas are egui
widgets, made in that order each frame, so egui gives a click to the topmost
one. `editor/tab.rs` keeps a tab's history: a change goes in when nothing is
being dragged or typed, and the canvas is lowered again when it changed.
Previews of the images pure squares make are worked out on a thread from the
expressions lowering hands out (`Lowered::previews`), without running
anything.

A square's pins can depend on a file: `image/shader` has the uniforms of its
shader as inputs. Signatures read it through `SigCx::read`, which resolves the
path against `Canvas::dir` (the folder of the canvas's file, set by whoever
opens it, not saved) and notes the file's modification time in
`Lowered::deps`; each frame `Tab::update` compares those times and lowers
again when one changed.

The shader (`lineit-rt/src/shader*`) is a small interpreter. `prep.rs` is the
text pass: the preprocessor, and the two things the tree-sitter grammar can't
read (`1e-3`, `float[3] a`). The grammar (the `tree-sitter-gdshader` crate)
parses; `parse.rs` turns its tree into `ast.rs`, where every name is a slot
and constants are folded; `eval.rs` runs `fragment()` for each pixel, over
`val.rs`'s values (components kept as `f64`, rounded to `f32` or wrapped at
32 bits after each operation). Math goes through `libm`, so the pixels are the
same on every machine. A shader is parsed once per text and tried on one pixel,
so what's wrong with it is told when its square is made, not for each pixel;
rows are shared among threads. Note that this grammar ends a `for` at its `)`,
so the statement after it is the body.

Vim mode is three files. `editor/vim.rs` is a state machine with no window:
keys (`Input`) in, actions (`Act`) out, and the parser of `:` commands; its
modes are Normal, Visual, Pin (wiring from the keyboard) and the command
line, and typing in a field is egui's text box, as without vim mode.
`editor/nav.rs` works out where an action goes (the square nearest in a
direction, along the wires, in order, matching a search; where the camera
has to be to show one) from the rectangles `canvas.rs` drew the frame
before, which it keeps in `CanvasState`. `editor/app/vimctl.rs` turns egui's
events into `Input`s, before the shortcuts read them, and does what the
actions say with the tabs, the search, the prompts and the settings. The
mouse and the keyboard make wires by the same rule (`canvas::can_wire`).

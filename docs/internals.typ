// lineit internals: how the editor, the modules, Run and Compile work.
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
#show raw.where(block: true): block.with(
  fill: luma(245),
  inset: 8pt,
  radius: 3pt,
  width: 100%,
)
#show raw.where(block: false): box.with(
  fill: luma(240),
  inset: (x: 2pt),
  outset: (y: 2pt),
  radius: 2pt,
)

#align(center)[
  #text(size: 22pt, weight: "bold")[lineit internals]
  #v(0.2em)
  #text(size: 12pt)[How the editor, the modules, Run and Compile work]
  #v(0.2em)
  #text(size: 10pt, fill: luma(90))[For lineit 0.0.1 (`LI_VERSION`)]
]

#v(1em)

This document explains how lineit is built: what a module is to the editor,
how a canvas becomes something that runs, how Compile turns it into a native
program, and how the editor reloads modules, its config and itself while it
runs. It follows the source closely; when it names a function or a type, it
is the one in the source, so you can read the code next to it. The module API
itself is documented in `src/lineit.h`, and this document doesn't repeat it
call by call.

#outline(depth: 2, indent: auto)

= Overview

lineit is a graphical programming language. A program is a _canvas_: squares
placed on a plane and wired together. Each square is an instance of a
_module_, a small C program with a fixed set of callbacks. Wires carry either
values (data) or _ticks_, the signal that says "run now".

The same canvas can be used in two ways:

- *Run*, inside the editor. The editor loads every module as a shared
  object, keeps one private copy per square, and calls the callbacks while
  you keep editing.
- *Compile*, outside of it. The editor writes the canvas as C, with the C of
  each module in it, and builds a native program linked with a small runtime
  that implements the same module API without the editor.

Both start from the same intermediate form, the _program_ (see
@sec-program), so they behave the same. The test suite checks exactly that:
every example canvas is run in the editor and compiled, and both have to
print the same lines.

== Pieces

#table(
  columns: (auto, 1fr),
  stroke: 0.5pt + luma(200),
  inset: 6pt,
  [*Path*], [*What it is*],
  [`src/main.c`], [The editor: window, canvas, menus, tabs, the module
    loader, the program lowering, Run, Compile, undo, hot reload. One file.],
  [`src/lineit.h`], [The module API. The only header a module includes.],
  [`src/value.c`, `src/value.h`], [What values need at run time: records,
    variables, lists, trees and `li_format`. Shared by the editor and the
    compiled programs.],
  [`src/rt/rt.c`, `src/rt/rt.h`], [The runtime of compiled programs: the
    module API without the editor.],
  [`modules/`], [The standard modules, in folders (`math/add.c`,
    `flow/clock.c`...), plus subgraphs (`.lua`) and custom types
    (`.type.lua`).],
  [`thirdparty/`], [Git submodules: `cum.h` (dynamic arrays, `Da`), `bt.h`
    (red-black trees), `conf.h` (Lua config files), `flag.h` (command line
    flags).],
  [`raylib-6.0_linux_amd64/`], [raylib, vendored: window, input, drawing,
    fonts, files.],
  [`tools/`], [`run_tests.c`, `test_extract.c`, `gen_examples.c` and
    `lineit-pack` (makes packages).],
  [`config.lua`], [Every setting of the editor, commented.],
  [`scripts/`], [`install.sh` (installs a release), `release.sh` (tags
    one), the git hooks and `docker-entrypoint.sh`.],
  [`Dockerfile`], [The Docker image: lineit on a virtual screen (Xvnc),
    shown in the browser by noVNC.],
  [`assets/`], [The icon and the desktop entry.],
)

== What `make` builds

- `lineit`, the editor, from `src/*.c`. It is linked with `-rdynamic` so that
  the `li_*` functions it defines are exported: the modules it loads call
  them directly, they are not in any library.
- `build/modules/**/X.so`, one shared object per `modules/**/X.c`, built
  with `-fPIC -shared` and `-I./src`. Every one of them is rebuilt when any
  header under `modules/` changes, since several modules share headers
  (`math/arith.h`, `compare/compare.h`, `logic/logic.h`...).
- `build/modules/**/X.lua`, symbolic links to the subgraphs and types in
  `modules/`. They are linked, not copied, so saving one in the editor is
  seen without running `make`.
- `build/rt/liblineit-rt.a`, the runtime of compiled programs (`value.c`
  and `rt/rt.c`), and `build/rt/include/`, the headers those programs and
  their modules are compiled with.

Everything is built with `-O2`, hardened (`_FORTIFY_SOURCE=3`, stack
protector, stack clash protection, a position independent editor and
read-only relocations), and with `-Wall -Wextra -Wformat=2 -Wshadow`;
`make WERROR=1` makes the warnings errors, as CI does. raylib and
`thirdparty/` are included with `-isystem`, so their warnings are theirs.
When the flags change (`make debug`, `WERROR`), everything is rebuilt:
`build/.flags` remembers the last ones.

`make test` and `make test-asan` build the test tools (see @sec-tests), and
`make install`, `make appimage` and `make dist` what's released (see
@sec-install).

= Values and types <sec-values>

== `Li_Type` and `Li_Value`

Everything that travels through a wire is a `Li_Value`: a type tag and a
union.

```c
typedef enum Li_Type {
        LI_NONE,   /* no value */
        LI_TICK,   /* activation, carries no value */
        LI_ANY,    /* pin accepts any data type */
        LI_BOOL, LI_INT, LI_FLOAT, LI_STRING, LI_PTR,
        LI_TYPE_COUNT,
        /* custom types come after: LI_TYPE_COUNT and up */
} Li_Type;

typedef struct Li_Value {
        Li_Type type;
        union { bool b; long i; double f; const char *s; void *p; };
} Li_Value;
```

`LI_TICK` and `LI_ANY` are pin types, never the type of a value: a tick
carries nothing, and `LI_ANY` only says "this pin takes any data type". A
value of type `LI_NONE` means "nothing here": an input with nothing plugged
in, an output never set, or a computation with no result (a division by
zero gives `LI_NONE`, not a crash).

== Ownership

A string or pointer in a `Li_Value` belongs to whoever set it. When a module
calls `li_set` with a string, the pointer is stored as it is; the module
keeps the buffer alive (a static buffer or its state) and the readers only
use it until the next change.

Whoever keeps a value longer than the module that made it copies it with
`value_dup`, which duplicates strings and records recursively, and frees it
with `value_drop`. That is what variables, lists and trees do (see
@sec-vars), so a module can store a string it got from a wire and forget
about it.

== Records: custom types

Custom types (see @sec-types) are records. A value of one has the type id of
the custom type and `.p` pointing to a `Record`:

```c
typedef struct Record {
        Li_Type type;
        int count;
        Li_Value fields[];
} Record;
```

`value.c` doesn't know which ids are records nor what their fields are
called: whoever links it provides `type_is_record` and `type_field_name`.
The editor answers from its list of loaded types, compiled programs from the
`rt_types` table that Compile writes into `main.c`. That is how the same
`value.c` serves both.

== Text

`li_format` writes a value for people: `42`, `2.5`, `true`, the string
itself, `-` for `LI_NONE`, and `{x: 1, y: 2}` for records. The editor has
its own `value_text` for params and typed fields. With `exact` it writes a
float with the fewest digits (from 6 to 17) that read back as the same
number: `0.1`, not `0.10000000000000001`. So saving and loading a canvas
never changes a number, and the file stays readable.

`value_parse_as` goes the other way: text typed in a field, read as a given
type. For `LI_ANY` inputs the type comes from what is typed: `2` is an int,
`2.5` a float, `true` a bool, anything else a string. Since `"2"` typed in an
`any` input could also be the string `"2"`, the canvas file saves the type
next to the value.

= Modules <sec-modules>

== What a module is

A module is a C file that includes `lineit.h` and defines some callbacks.
Only `module_init` is required. The editor finds them with `dlsym` by name
and keeps them in a `ModuleFns`:

#table(
  columns: (auto, 1fr),
  stroke: 0.5pt + luma(200),
  inset: 6pt,
  [*Callback*], [*When it's called*],
  [`module_init(n)`], [A square of it is made. It declares the pins and
    params and sets up its state.],
  [`module_param(n, i)`], [Param `i` changed: from the settings panel, on
    load, and once per param right after `module_init`, so defaults take the
    same path as changes.],
  [`module_load(n, state)`], [The square came from a saved canvas, with the
    text `module_save` returned. After `module_init` and its params.],
  [`module_start(n)`], [Run was pressed. Once, before the first frame.],
  [`module_frame(n)`], [Every frame while running. Where tick chains start
    on their own (a clock).],
  [`module_tick(n, input)`], [A tick reached tick input `input`.],
  [`module_eval(n)`], [Someone is about to read one of its outputs. Pure
    squares compute there and need no ticks.],
  [`module_draw(n)`], [Every frame, running or not, before drawing: update
    what the square shows with `li_text`.],
  [`module_save(n)`], [The canvas is being saved: return the state as text,
    or `NULL`.],
  [`module_emit(n)`], [Only for Compile: C to put the square inline (see
    @sec-inline).],
  [`module_destroy(n)`], [The square is removed. Free the state.],
)

The `Li_Node` a callback gets is opaque to the module. In the editor it is
the square itself (`typedef struct Li_Node Square`); in a compiled program it
is a much smaller struct of the runtime. Modules can't tell the difference,
which is what lets the same C run in both.

== Pins

`li_input` and `li_output` add a pin to the square and return its index;
inputs and outputs are numbered separately, from 0, in the order they are
declared. In the editor each pin is a `PinInfo`:

```c
typedef struct PinInfo {
        char *name;
        Li_Type type;
        PinKind kind;   /* PIN_WIRE, PIN_VALUE or PIN_OPTIONS */
        Li_Value value; /* outputs: last set; PIN_VALUE: typed; PIN_OPTIONS: .i chosen */
        char *str;      /* owns value.s when it's a typed string */
        char **options;
        int option_count;
} PinInfo;
```

Inputs come in three kinds:

- `PIN_WIRE` (`li_input`): only a wire.
- `PIN_VALUE` (`li_input_value`): a wire, or, while nothing is plugged in, a
  value typed in a field drawn next to the pin. A bool is a checkbox. The
  default comes from the module.
- `PIN_OPTIONS` (`li_input_options`): no wire at all. The user picks one of
  a list (`"a|b|c"`); `li_get` returns its index as an `LI_INT`, `li_option`
  its name.

An output's `value` is simply the last thing `li_set` put there. Reading it
from another square (through `li_get`) returns that, after giving the square
a chance to compute it (see @sec-get).

`li_output_type` changes the type of an output after `init`. A constant
whose type is chosen in the square uses it. Wires are checked against pin
types every time the program is lowered, so a wire that becomes invalid
stops carrying values (and is drawn as an error) without being deleted.

== Params

A param is a setting, edited in the settings panel (double click a square).
`li_param` declares it with a default whose type is the param's type (int,
float, bool or string). The editor keeps a `Param` per param, with the
string copied when it is one. Changing it parses the text as that type and,
if it parses, calls `module_param`. Params are saved with the canvas and
copied into the compiled program.

== One copy of the module per square <sec-copies>

The API promises that every square has its own globals: a module can keep
its state in plain `static` variables instead of `li_set_data`. The clock,
for instance, keeps `static double last; static long count;`.

`dlopen` on a file that is already loaded doesn't load it again, it only
bumps a reference count, so every square of a module would share its
globals. To avoid that, `square_open` copies the `.so` to a private file
under a temporary folder (`$TMPDIR/lineit-XXXXXX/ID-LABEL.so`), opens the copy
with `RTLD_NOW | RTLD_LOCAL`, and removes the file right away: once it is
mapped, the file isn't needed. Every square ends up with its own mapping, and
so its own data and BSS segments. The temporary folder is removed when the
editor quits.

Built-in modules (subgraphs, ports, the make/break squares of custom types)
have their callbacks in the editor itself (`Module.builtin`), and don't go
through any of this.

== Finding modules

`modules_scan` walks two folders recursively for `.so`, `.lua` and `.li`
files: `modules.path`, lineit's own modules (read only, `build/modules` in
the source tree), and `modules.user`, the user's (where Make subgraph and
New type write, `modules/` in the source tree, see @sec-install).
`module_scan_file` looks at each file. Each module is a `Module`:

```c
struct Module {
        char *path;               /* the .so, or the .lua of a subgraph */
        const ModuleFns *builtin; /* callbacks in the editor itself */
        char *lib;                /* a package: its .so for here, taken out */
        Li_Type type;             /* make_/break_ of a custom type: which */
        char *name;               /* "math/add" */
        const char *label;        /* "add" */
        long long mtime;
        int dir;                  /* 0: modules.path, 1: modules.user */
        bool gone, seen;
};
```

The name is the path inside its folder without its extension, so
`build/modules/math/add.so` is `math/add`. That name is what canvases save
for each square, and its folders are the submenus of _Add module_. A name in
both folders is the first one's: `modules.user` can't replace lineit's
modules, and in the source tree, where `build/modules` links the subgraphs
of `modules/`, each is found once.

Before a `.so` is accepted, `module_check` opens a copy of it and makes sure
it has `module_init`. A `.lua` must be a canvas (`canvas_check`), and a `.li`
must have a library for this platform or be buildable (see
@sec-packages). Whatever fails is skipped with the reason on stderr.

Custom types (`.type.lua`) are read first by `types_scan`, since modules
and subgraphs may use them.

== Reloading modules while running

Every half second (`hot_reload`), `modules_scan(true)` looks at the folder
again:

- A file whose modification time changed is a rebuilt module. It is only
  used once it has been left alone for 300 ms (`MODULE_SETTLE_NS`), since
  `make` may still be writing it. If the new build doesn't load, the old one
  is kept and the bar says so; the new one is tried again when it changes
  again.
- If it loads, `module_reload_squares` gives every square of that module, in
  every tab and inside every subgraph, a fresh copy. Each square is saved to
  text and read back with the same id, so it keeps its place, its params,
  its typed inputs, its wires and its `module_save` state.
- A new file is a new module, added to the menu.
- A file that is gone marks its module `gone`: it leaves the menu, but the
  squares that use it keep their copies, so they go on working.

= Squares and wires

== Squares

A square is a `Li_Node` (called `Square` inside the editor). Besides its
module and pins, it has its id (unique in its canvas, saved), its position
(the top-left corner in world space), its own `dlopen` handle and
`ModuleFns`, its params, its state pointer (`li_set_data`), the text it
shows (`li_text`, 64 bytes), and things only the editor uses: whether it is
selected, when a tick last reached it (to make it glow), cached sizes, and
for subgraphs, its `Graph`.

Squares are always square. `square_side` makes them as tall as their text
and pins need and as wide as their pin names and fields need, and takes the
largest. Inputs go on the left, outputs on the right, evenly spaced under a
band at the top where the text is.

The canvas is a list of squares in draw order (the last one on top) and a
list of links.

== Wires

```c
typedef struct Link {
        int from, from_pin; /* square id, output index */
        int to, to_pin;     /* square id, input index */
        double flash;       /* last time something went through it */
} Link;
```

A wire goes from an output to an input of another square.
`types_compatible` decides what can be wired: ticks only to ticks, and data
to the same type, or from or to `LI_ANY`. A data input takes one wire: wiring
another one replaces it. A tick input takes as many as you want.

Links refer to squares by id, not by pointer, which is what lets the canvas
be saved, pasted and restored by undo without fixing pointers.

= Running: ticks and pulls

lineit mixes two ways to compute things.

*Ticks push.* A tick output, when its square calls `li_tick`, calls
`module_tick` of every square wired to it, right away, in the order of its
wires. That square may set outputs and tick further. A tick chain is plain C
recursion: one `li_tick` call runs the whole chain below it before it
returns.

*Data is pulled.* A square that reads an input with `li_get` gets the value
of the output wired to it. If that output's square has `module_eval`, it is
called first, so it computes the value from its own inputs, which may pull
from further back. Pure squares (math, comparisons, constants) need no ticks
at all: they compute exactly when, and only if, someone reads them.

A typical canvas: `flow/entry` ticks once when Run is pressed; the tick
reaches `io/print`, which pulls its `value` input; that comes from a
`math/add`, which pulls its two inputs and adds them. Nothing computes
until the print asks.

== The program <sec-program>

Run doesn't follow the wires of the canvas as they are drawn. Every frame,
`program_lower` turns the canvas into a _program_, a flat list of the
squares that run, with every wire already resolved. It is to the canvas what
a resolved syntax tree is to the source of a language.

```c
typedef struct Program {
        Da(Node) nodes;     /* in the order they get start and frame */
        Da(Source) sources; /* per node, one per input */
        Da(Out) outs;       /* per node, one per output */
        Da(Target) targets; /* what tick outputs tick */
        Da(Wire) wires;     /* what to light up */
        long long version;  /* squares_version it was lowered for */
} Program;
```

- A *node* is a square that runs, with the scope whose variables it uses
  (see @sec-scopes). Subgraph squares and port squares are not nodes: the
  squares inside a subgraph are, in its place (see @sec-subgraphs).
- A *source* says where an input gets its value: `SRC_OUTPUT` (output `pin`
  of node `node`), `SRC_TYPED` (the field of an input of some square: its
  own, or that of the subgraph square around it), or `SRC_NONE`
  (unplugged, or wired to an incompatible type).
- An *out* says which *targets* a tick output reaches: a range in
  `targets`, each one an input of a node. Data outputs have none.
- *Wires* are the links a source or target goes through, possibly several
  when it crosses subgraph ports, so that all of them glow when something
  goes through.

Lowering walks the canvas recursively. `lower_squares` makes the nodes,
going into subgraphs as it finds them and remembering each canvas as a
_level_ (with the level it is in). Then for every input of every node,
`lower_source` finds the last link into it in its level. If the other end
is a normal square, that is the source. If it is a subgraph square, it
continues from the matching `subgraph/output` inside it. If it is a
`subgraph/input` port, it continues from the matching input of the subgraph
square, one level up. `lower_targets` does the same for tick outputs,
forwards. Ports deeper than 64 levels (`LOWER_DEPTH`) mean a subgraph wired
to itself, and give nothing.

Squares don't move in memory, but they can be freed. Every time a square is
made or freed `squares_version` goes up, and a square's node (`Square.ir`,
its index in `prog.nodes`) is only trusted while the program's `version` is
the current one. Lowering every frame is what makes editing while running
work: rewire anything and the next frame runs the new graph.

== `li_get` <sec-get>

`li_get(n, input)` in the editor:

+ Options inputs return the chosen index directly.
+ If the square has no node yet (it is being made or loaded), it returns
  its typed value, or nothing.
+ Otherwise it looks up the input's source in the program. Typed: the
  value in that field. Output: the output's current type is checked again
  (it may have changed since lowering); if the square has `module_eval` and
  is not already evaluating, `module_eval` runs, in the square's scope, and
  then the output's value is returned.

The `evaluating` flag breaks cycles: a pure square that ends up reading its
own output gets the value it had, instead of recursing forever.

When `li_get` is called inside a tick chain, the wires it reads through
light up too. Reads done every frame by `module_draw` don't, or every data
wire would glow all the time.

== `li_tick`

`li_tick(n, output)` records when the output fired (for the tooltip), finds
the out of that output and, for each target, makes its wires and square glow
and calls the target's `module_tick` in the target's scope.

Since tick chains are recursion, a loop in the graph (a tick that comes
back to where it started) would overflow the stack. A depth counter stops
any chain deeper than 256 squares (`TICK_DEPTH_MAX`) with a warning. Loops
that should repeat use `flow/loop` or `flow/while`, which iterate inside
one `module_tick` instead of ticking themselves.

== A frame

Every frame of the editor, `run_frame`:

+ lowers the canvas shown,
+ if it is running, calls `module_frame` of every node, in order,
+ calls `module_draw` of every square, running or not,
+ lowers and runs a frame of every other tab that is running (see
  @sec-tabs).

Pressing Run sets `running`, clears the output panel if
`output.clear_on_run` says so, lowers the canvas and calls `module_start` of
every node once, before the first frame, so entry squares go first. Ticks
run to the end at once (they are calls), so if no node has `module_frame`
nothing can happen after that: the run is over, and `running` is cleared
right away, as a compiled program ends after start. With one (a clock) it
runs until Stop. Stop only clears `running`: the squares keep their state.

= Variables, lists and trees <sec-vars>

== Storage

`value.c` implements three kinds of named storage for modules:

- *Variables* (`li_var_set`, `li_var_get`): a dynamic array of name and
  value.
- *Lists* (`li_list_*`): growable arrays of values, by name. Negative
  indices count from the end.
- *Trees* (`li_tree_*`): string keys to values, in key order, a red-black
  tree from `bt.h`. `li_tree_keys` writes the keys, in order, to a list.

Lists and trees themselves are found by name in two more red-black trees
(`lists` and `trees`). Every value stored is a `value_dup` copy, so it
outlives the module that set it, and is freed with `value_drop` when it is
replaced or cleared. None of this is saved with the canvas: it starts empty,
and Clear all or Load empty it.

== Scopes <sec-scopes>

Variables, lists and trees are _local_ to the canvas a square is in, like a
C function's locals. A square inside a subgraph can't see the ones of the
canvas around it, and the canvas can't see the subgraph's. Each subgraph
square has its own, kept between ticks. `li_global_*` variables are shared
by every canvas of the tab.

This is done without passing a scope around. The globals `vars`, `lists`
and `trees` always hold the locals of the canvas that is running _now_.
Each subgraph's `Graph` holds its own while it isn't running. `scope_enter(g)`
swaps them: it gives the current ones back to their owner and puts `g`'s in
the globals, returning whose were there so the caller can restore them.
Every call into a module (`module_tick`, `module_eval` through `li_get`,
`module_start`, `module_frame`) is wrapped in `scope_enter` of that node's
scope. Swapping is three pointer assignments, so it costs nothing.

Compiled programs do the same with an array of locals indexed by a scope
number (`rt_scope`): 0 is the canvas, and each subgraph square gets the next
one.

= Subgraphs <sec-subgraphs>

== A canvas as a module

A `.lua` canvas under the modules folder is a module. A square of it, a
_subgraph_, reads its own private copy of that canvas when it is made
(`graph_init`) and keeps it in a `Graph`:

```c
typedef struct Graph {
        SquareList squares; /* while swapped in: the ones outside */
        LinkList links;
        VarList vars;       /* its locals */
        BT *lists, *trees;
        Da(Square *) inputs;  /* subgraph/input squares inside, by port */
        Da(Square *) outputs; /* subgraph/output squares inside */
} Graph;
```

Inside, `subgraph/input` and `subgraph/output` squares are its ports. Each
has an options input to choose its type (tick, any, bool, int, float,
string) and a `name` param. The subgraph square gets one input per
`subgraph/input` and one output per `subgraph/output`, named after them, in
the order they are inside from top to bottom (left to right on the same
row). A data input gets a typed field with a default for its type, like any
`li_input_value`.

All the code that reads and changes a canvas works on the globals `squares`
and `links`. To load the canvas of a subgraph, `graph_swap` puts the
subgraph's lists (and locals) there, `canvas_read_file` reads into them, and
a second `graph_swap` puts everything back. The same trick serves Make
subgraph and module reloads.

A subgraph can't contain itself: `graph_init` keeps a stack of the subgraph
modules being made (`graph_making`), and a square of one of them inside
itself shows "inside itself" instead of recursing. The stack is 16 deep.

== Running a subgraph

A subgraph square does nothing itself. When the canvas is lowered, its inner
squares become nodes, and the wires through its ports are followed so that,
for example, a tick going into the subgraph square goes straight to the
squares inside that its `subgraph/input` is wired to. Like a C compiler
inlining a function: what remains is the body, in place, with the arguments
wired in.

Only its scope stays: every inner node is tagged with the subgraph's
`Graph`, so its variables are the subgraph's own.

`graph_draw` calls `module_draw` of the squares inside, so their text stays
current, and "Open in a tab" opens the subgraph's file to edit it. Saving it
reloads every square of that subgraph like any rebuilt module.

== Make subgraph

Ctrl+G (or _Make subgraph_ in the square menu) is "extract function". The
selected squares and the wires between them are written to a new `.lua`
under `modules.user`, which is one of the folders modules are loaded from,
so it shows up right away, and one square of the new subgraph takes their
place.

The ports are found from the wires that cross the selection:

- each output outside that feeds squares inside becomes a `subgraph/input`
  of that output's type; one output feeding several squares inside is one
  input,
- each output inside that is used outside becomes a `subgraph/output`.

They keep the vertical order of the pins they replace, and the wires around
are plugged into the new square where they were, so the canvas does the
same as before. `tools/test_extract.c` checks exactly that on the example
canvases.

= Custom types <sec-types>

A custom type is a record, like a C struct, made in the editor (File > New
type) or by hand as `NAME.type.lua` under the modules folder:

```lua
lineit_type = "0.0.1"
fields = {
        { name = "x", type = "float" },
        { name = "y", type = "float" },
}
color = { 251, 146, 60 } -- of its wires, optional
```

Fields are bool, int, float, string, any, or another custom type by its
name. `modules/geo/point.type.lua` is the type `geo/point`, and `types_scan`
adds two built-in modules next to it:

- `geo/make_point`: an input per field (typed fields, with defaults), one
  output with the record. Its `module_eval` builds a `Record`.
- `geo/break_point`: one input with a record, an output per field.

Each type gets an id from `LI_TYPE_COUNT` up, in the order types are found.
Those ids are what flows in `Li_Value.type`; their names are what canvases
save. Compile writes every type into `rt_types`, in the same order, so the
ids in the program are the editor's, and implements make and break with
`rt_make_eval` and `rt_break_eval`.

Types are reloaded like modules: an edited type file updates the pins of its
make and break squares.

= The canvas file

== Format

A canvas is a Lua file, read with `conf.h`:

```lua
lineit_canvas = "0.0.1"

camera = { x = 114.05, y = 12, offset_x = 640, offset_y = 344, zoom = 1 }

squares = {
        { id = 0, module = "flow/entry", x = -35, y = -35 },
        { id = 1, module = "io/print", x = 116.9, y = -73.1,
          inputs = { { name = "value", type = "string", value = "Hello, world!" }, } },
}

links = {
        { from = 0, from_pin = 0, to = 1, to_pin = 0 },
}
```

Squares are in draw order. Each one has its id, module name and position,
and, when it has them, its `params` and `inputs` (typed values and chosen
options, by name, with the type for typed values) and `state`, what
`module_save` returned. Links refer to squares by id and to pins by index.
Floats are written with `%.9g`, so positions survive a round trip. Strings
are escaped as Lua literals.

Params and inputs are matched by name when read, so a module can add,
remove or reorder them and old canvases still load what still exists.
Squares of modules that aren't there are skipped, with their wires.

== One writer, four readers

`canvas_write` writes the canvas, with flags for what to include: the
camera, the state, only the selection. Everything that needs a copy of a
canvas uses it:

- *Save* writes the camera and the state.
- *Copy* writes only the selected squares and the links between them.
- *Undo* snapshots write neither camera nor state (see @sec-undo).
- *Module reloads* and the *session* of an editor reload write what they
  need to rebuild squares.

`canvas_read` reads it back in one of four modes: `READ_LOAD` replaces the
canvas and the camera; `READ_RESTORE` makes the canvas match, keeping the
squares that are still there as they are (undo); `READ_PASTE` adds the
squares with new ids, selected, at the mouse; `READ_GRAPH` reads into a
subgraph being made. Since `conf.h` only reads files, reading from text
writes it to a temporary file first.

= Versions

There is one version, `LI_VERSION` in `lineit.h`, `MAJOR.MINOR.PATCH`. It is
the version of the editor, of the module API, and of every file lineit
writes: canvases say `lineit_canvas`, type files `lineit_type`, packages
`api` in their manifest.

Something made with version V works with a lineit of version H if they have
the same major and V's minor is not newer than H's (`version_ok`). The minor
goes up when something is added, so what older ones made still works; the
major when something changes or goes away. The patch is never checked.
Versions are parsed as integers, never as floats, so `0.10` is newer than
`0.9`.

= Compile <sec-compile>

Compile (in the bar, or `lineit --compile FILE` with no window) makes a
native program of the canvas. It lowers the canvas to a program, exactly as
Run does, and writes that program as C in
`compile.output/NAME.build/`, `NAME` being the canvas file's name. Then it
runs `make` there, with what it prints in the output panel, and the program
ends up in `compile.output/NAME`.

== The files

#table(
  columns: (auto, 1fr),
  stroke: 0.5pt + luma(200),
  inset: 6pt,
  [*File*], [*What's in it*],
  [`nI.c`], [One per node `I` that isn't inline. Its module's C, found in
    `modules.source/NAME.c` (or next to its `.so` in `modules.user`, for
    the user's own), `#include`d after a `#define` per callback
    (`#define module_tick n3_tick`...).],
  [`nI.o`], [For packages, instead: a copy of the package's object with its
    callbacks renamed, made by the Makefile with `objcopy`.],
  [`main.c`], [The nodes, the wires as functions, the inline squares, the
    types, and `setup`, `start`, `frame` and `main`.],
  [`Makefile`], [Builds the program with `compile.cc` and `compile.cflags`,
    linking `liblineit-rt.a` and what the packages need.],
)

Each `nI.c` is its own object, so its `static` globals are its own: every
square has its own globals, as in the editor (see @sec-copies). This is
also why everything in a module except its callbacks has to be `static`:
there is one copy of the module per square in a single program, and
non-static symbols would clash at link time. Squares of the same module
share the same source file, but each includes it into its own object.

Leftovers from an earlier compile of the same canvas (`nI.c`, `.o`, `.d`
for nodes that aren't there any more) are removed, so `make` only rebuilds
what changed.

== `main.c`

For node `I`, Compile writes:

- `nI_inK()` for each input `K`: where it gets its value, as a function.
  For a typed value it returns the constant. For an output of node `J`, it
  calls `rt_pull(&N[J], nJ_eval, pin)`, which runs `module_eval` of `J` (in
  its scope, guarded like the editor's) and returns the output. For an
  inline node it calls `nJ_outP()` directly. Unplugged, it returns
  `LI_NONE`.
- `nI_tickK()` for each tick output `K` that reaches something: a call to
  `nJ_tick` of every target, in order, through `rt_call_tick` so it runs in
  the target's scope.
- `nI_get[]` and `nI_ticks[]`: those functions, by pin.

Then the array of nodes:

```c
static Li_Node N[3] = {
        { .module = "flow/entry", .square = 0, .scope = 0,
          .tick = n0_ticks, .tick_count = 1 },
        { .module = "io/print", .square = 1, .scope = 0,
          .get = n1_get, .get_count = 1, .tick = n1_ticks, .tick_count = 1 },
        ...
};
```

In the runtime, `li_get(n, k)` is just `n->get[k]()` and `li_tick(n, k)` is
`n->tick[k]()`. There is no graph left at run time: every wire has become a
direct function call, which the C compiler can see through.

`setup()` brings every node to the state its square has in the editor
_now_: it calls `nI_init`, sets the chosen options (`rt_option`), sets each
param (`rt_param`) and calls `nI_param`, and calls `nI_load` with what
`module_save` returns at compile time. So a compiled program starts exactly
where the canvas is, counters and accumulators included.

`start()` calls `nI_start` of every node that has one. `frame()`, only
written if some node has `module_frame`, calls them all.

== The runtime

`src/rt/rt.c` is the module API for compiled programs. Its `Li_Node` holds
what `main.c` gives (module name, square id, scope, the `get` and `tick`
tables) and what the module makes (pins, params, data). It implements the
`li_*` functions without anything of the editor: `li_text` does nothing,
`li_print` writes to stdout, `li_time` is seconds since the start.
Variables, lists, trees and records come from the same `value.c` the editor
uses.

`rt_main` runs the program: `setup`, `start`, and then, if there is a
`frame`, one frame every 1/60 s until it is killed. A program with no
`module_frame` anywhere ends right after `start`, since nothing could start
a tick chain any more. With `--frames N` it runs only `N` frames of fake
time (frame `i` is at `i/60` s) as fast as it can, which is what the tests
use.

== Inline squares <sec-inline>

A call per pure square works, but stops the C compiler from folding
`2 + 3`. A pure module (it has `module_eval` and no `module_tick`,
`module_frame` nor `module_start`) can define `module_emit` to be inlined
instead. It returns C that defines, for each output `K`,
`static Li_Value $_outK(void)`, computing it like `module_eval` does, using
`$_inK()` for its inputs. Compile replaces `$_` with a name for the square
(`n4_`) and `$source` with the path of the module's C file.

`math/arith.h` does it by including its own source again with `LI_INLINE`
defined, which leaves out the callbacks and keeps only the operation:

```c
#define LI_INLINE
#define int_op $_int_op
#define float_op $_float_op
#include "$source"
...
static Li_Value
$_out0(void)
{
        const char *why;
        return arith($_in0(), $_in1(), $_int_op, $_float_op, &why);
}
```

An inline node gets no `nI.c` and no entry in `setup`. The inputs that read
it call `nI_outK()` directly, all of it `static` in one file, so constants
fold and calls disappear. `module_emit` is called in the editor, so it can
read the square's params and options and write C for just those.

Squares that read each other in a loop through inline squares are left as
calls (`inline_loop`): inlined, the loop would have no end. Packages are
never inline.

== Packages in a compiled program

A package has no C file under `modules.source`. For each package used,
Compile takes it out to `NAME.build/packages/`, runs its Makefile's
`lineit-o` rule to build its sources as one relocatable object, and
`lineit-libs` to know what that object needs to link (`-lcurl`...). Then
for each square of it, the generated Makefile makes `nI.o` from that object
with `objcopy`: first `--keep-global-symbol` for the callbacks, making
every other symbol local, then `--redefine-sym module_X=nI_X`. Each square
gets its own copy with its own globals, like the C modules.

= Packages <sec-packages>

== What a package is

A package (`.li`) is a way to give a module to others: one file with the
module built for one or more platforms, its source, its license, and a
Makefile that builds it. It is a zip:

#table(
  columns: (auto, 1fr),
  stroke: 0.5pt + luma(200),
  inset: 6pt,
  [`manifest.lua`], [`name`, `version`, `license`, `api` (the
    `LI_VERSION` it was built with), and `platforms`: platform to library
    inside, like `["linux-x86_64"] = "lib/linux-x86_64.so"`.],
  [`lib/`], [The libraries.],
  [`src/`], [Its C.],
  [`Makefile`], [Rules `lineit-so` (the library), `lineit-o` (one object,
    for Compile) and `lineit-libs` (what that object links with).],
  [`LICENSE`], [For people; not read by lineit.],
)

`tools/lineit-pack` makes them from a module's own Makefile; `lineit-pack
--howto` documents what that Makefile has to do.

== Loading one

A `.li` in the modules folder loads like a `.so`. The editor reads the zip
itself (`zip_directory`, `zip_member`, inflating with zlib), so it needs no
`unzip`. `package_lib`:

+ reads `manifest.lua` (written to a temporary file, since `conf.h` reads
  files),
+ checks `api` with `version_ok`,
+ checks every file against its SHA-256 (see below),
+ looks up the library for this platform (`LI_PLATFORM`, like
  `linux-x86_64`, from the compiler's macros),
+ writes that member to the temporary folder and returns its path.

From there squares copy it like any `.so`. The library stays in the
temporary folder while the module is loaded.

If the package has no library for this platform, `package_build` builds it:
it takes the package out (everything but `lib/`) to
`modules.cache/NAME-CRC.build/` and runs `make lineit-so` there, with
`LINEIT_INCLUDE` pointing to the runtime's headers and the configured
compiler. The result is cached by the CRC-32 of the whole `.li`, so it is
built once, and again only when the package changes.

Taking a package out checks every name stays inside the folder
(`zip_name_safe`: no absolute paths, no `..`), and only writes files whose
contents changed, so `make` doesn't rebuild what didn't.

== Checksums

`lineit-pack` writes, at the end of `manifest.lua`, the SHA-256 of every
other file of the package:

```lua
sha256 = {
        { file = "Makefile", sum = "f189a8a8..." },
        { file = "lib/linux-x86_64.so", sum = "4cdbcd04..." },
        { file = "src/reverse.c", sum = "cd2c0a07..." },
}
```

A list and not a table by file name, since `conf.h` takes dots in a key as
fields (`lib/linux-x86_64.so`). `package_sums` reads it and checks it names
every file of the zip but `manifest.lua`, only them, and that each matches,
with OpenSSL's libcrypto (`EVP_Digest`). It runs before anything of a
package is used: loading its library, building it (`package_build`, even
when the build is cached) and compiling it (`package_extract`, for
Compile). A package without the list, with a file it doesn't name, or with
one that doesn't match, is skipped with the reason.

It finds files that are broken, or that were changed after `lineit-pack`
made the package. It doesn't say who made it: whoever can change the files
can write a new `manifest.lua` too. Packages are native code that runs as
the user, so only the ones from someone trusted should be used.

= The editor

== The main loop

`main` parses the flags, loads the config, opens the window, loads the
fonts and the modules, opens the first tab, and loops until it quits.
Each frame:

+ `keys_poll` reads every key pressed this frame from raylib's queue, so
  quick taps are not missed.
+ Input goes to the first thing that takes it, in this order: the dialog
  asking about unsaved changes, the path prompt, the field being typed in a
  square, the settings panel, the output panel, the bar, the tab strip, the
  menu. If none took it, keyboard shortcuts and then the mouse on the canvas.
+ `run_frame` (see above).
+ `history_check` takes an undo snapshot if the canvas changed.
+ `autosave_tick`, then `hot_reload`.
+ Everything is drawn, from back to front: canvas, tooltip, panel, output,
  tabs, bar, menu, prompt, dialog.

All the state of the editor is in globals. That keeps every function short
and is what the swaps (`graph_swap`, `tab_swap`, `scope_enter`) rely on.

== Drawing

The canvas is a raylib `Camera2D`: right drag pans, the wheel zooms around
the mouse, between `canvas.zoom_min` and `canvas.zoom_max`. The font is
loaded twice: at the UI size for the bar and menus, and big for the canvas
so it stays sharp when zoomed in. Fonts are found by name with fontconfig,
keeping only `.ttf` and `.otf` (raylib can't load collections).

Wires are cubic Bézier curves with horizontal tangents at both ends. Each is
drawn in the color of its type, and glows for a moment after something goes
through it. A wire between incompatible types is drawn as an error.

Hovering an output shows a tooltip with its current value, or when it last
fired for a tick output.

== Menus and search

Right click opens a context menu. Menus nest up to 8 levels deep and scroll
when they don't fit. The menu of the empty canvas starts with "Add X", X
being the module placed last, and has a search box. Typing filters the
modules, ignoring case, with spaces matching underscores (`not equal`
finds `not_equal`), and sorts them by how well they match: the exact name
first, then names that start with the text, names that contain it, full
paths that contain it (`math/`), and last, paths that have its letters in
order (fuzzy), the tighter the better.

The square menu acts on the whole selection when the square clicked is
selected.

== Undo <sec-undo>

Undo is snapshots, not commands. Every frame, unless something is half done
(a drag, text being typed), `history_check` writes the canvas to text
without camera or state and compares it with the last snapshot. If it
changed, the last one goes to the undo stack (at most 200) and the redo
stack is emptied. Undo reads the previous snapshot back with
`READ_RESTORE`, which only remakes the squares that changed, so the others
keep their state and their place in a running program.

This means any change, made by any code, is undoable without that code
knowing anything about undo.

== Tabs <sec-tabs>

Each tab is a canvas file with everything that goes with it: squares,
links, camera, variables and lists, undo history, program, whether it is
running, and its path. The tab shown keeps all of it in the globals, so the
rest of the editor doesn't know tabs exist; the others keep theirs in their
`Tab`, and `tab_swap` trades them. Tabs that are running keep running when
they are not shown: `tabs_run_background` swaps each one in, runs a frame,
and swaps it out.

A tab is dirty when its canvas text differs from what it was when last
loaded or saved. Closing it, or quitting, asks about unsaved changes. Closed
tabs can be reopened (Ctrl+Shift+T) with their canvas as it was.

== One editor

`lineit -t FILE` opens `FILE` in a new tab of the editor already running.
The first editor listens on a Unix socket
(`$XDG_RUNTIME_DIR/lineit.sock`, or `/tmp/lineit-UID.sock`); a second one
connects, sends the path and exits. If none is listening, it becomes the
editor itself with that file.

== Autosave

When on (File > Autosave, or `save.autosave`), every
`save.autosave_interval` seconds every tab with changes and a file is
saved. Not in the middle of a drag or of typing: then it waits for the next
frame.

== Hot reload

Every half second, `hot_reload` checks three things.

*The config.* If `config.lua` or `settings.lua` changed, they are read
again from the defaults, so keys that were removed go back to them. Colors, sizes and fonts change
at once. The window size and the module and save paths are only read at
start. If it doesn't load, the old config stays.

*The modules.* See @sec-modules.

*The editor itself.* If the `lineit` binary was rebuilt, the editor hands
over to the new one. It writes a session file with everything, for every
tab: the canvas with camera and module state, the window position and size,
the selection, the output panel, and the undo and redo stacks. It forks and
runs the new binary with `--restore SESSION`, passing it the listening
socket of `-t` (`LINEIT_SOCKET_FD`) and a pipe (`LINEIT_READY_FD`). The new
editor restores the session, draws three frames and writes a byte to the
pipe. Only then does the old one quit. If the new one crashes or doesn't
answer within 10 seconds, it is killed and the old one stays, trying again
when the binary changes again. Running canvases start again in the new one,
since variables and lists are not in the session.

= Paths and installing <sec-install>

== Where lineit finds things

lineit runs from its source tree, installed with `make install`, or as an
AppImage, and the defaults of its paths depend on which (`paths_default`,
from where its binary is, `/proc/self/exe`):

#table(
  columns: (auto, 1fr, 1fr),
  stroke: 0.5pt + luma(200),
  inset: 6pt,
  [], [*Source tree* (next to `src/lineit.h`)], [*Installed* (`PREFIX/bin`, with `PREFIX/lib/lineit`)],
  [`modules.path`], [`build/modules`], [`PREFIX/lib/lineit/modules`],
  [`modules.user`], [`modules`], [`~/.local/share/lineit/modules`],
  [`modules.source`], [`modules`], [`PREFIX/share/lineit/modules`],
  [`modules.cache`], [`build/cache`], [`~/.cache/lineit/packages`],
  [`compile.output`], [`build/bin`], [`~/.local/share/lineit/bin`],
  [`compile.runtime`], [`build/rt`], [`PREFIX/lib/lineit/rt`],
  [config], [`config.lua`], [`~/.config/lineit/config.lua`],
)

The source tree's are relative to the tree, not to the folder lineit runs
in. The `~/` ones follow `$XDG_DATA_HOME`, `$XDG_CACHE_HOME` and
`$XDG_CONFIG_HOME`. A binary that is neither (the test tools, in `build/`)
keeps the built-in defaults, relative to the current folder. The config can
set any of them.

The config is the one given with `-c`, or the default above, which may not
be there: then the defaults are used (`config_start`). A `config.lua` in the
current folder is never read on its own: `conf.h` runs it as Lua, with its
standard library, so running lineit in a folder someone gave you would run
their code. The same goes for canvases and type files: they are Lua.

== make install

`make install` copies, under `PREFIX` (`~/.local`, or `/usr/local` as
root) and `DESTDIR`: the editor and `lineit-pack` to `bin/`, the modules
(subgraphs and types as files, not links) to `lib/lineit/modules`, the
runtime and its headers to `lib/lineit/rt`, the C of the modules to
`share/lineit/modules`, `config.lua` and the examples to `share/lineit`,
and the desktop entry and icon. `lineit-pack` finds `lineit.h` in the
source tree or, installed, in `lib/lineit/rt/include`.

== The AppImage

`make appimage` is `make install` into `build/AppDir` with `PREFIX=/usr`,
plus the shared libraries lineit needs (but glibc, X11 and GL), gathered by
linuxdeploy, and packed by appimagetool. Both tools and the AppImage
runtime are pinned and checked against their SHA-256 before every use. From
inside the mounted AppImage, `usr/bin/lineit` finds `usr/lib/lineit` next to
it, so it works as an installed lineit. It's x86_64 only, as is the raylib
in the repository. Compile needs a C compiler and make on the machine.

== Releases

There's one version, `LI_VERSION` (see Versions). `scripts/release.sh` tags
main with it, once it was bumped, and pushes the tag. CI publishes a
release for a `v*` tag that is `LI_VERSION`, and refreshes the `nightly`
one on every push to main, only if every check passed. A release has the
AppImage, the source with the submodules (`make dist`, reproducible: the
same commit gives the same bytes), the docs as PDF, `install.sh` and
`SHA256SUMS`, all attested as built by the workflow.

`scripts/install.sh` installs the AppImage of a release, checking it
against `SHA256SUMS`, and `SHA256SUMS` against its attestation when `gh` is
there, and adds it to the applications menu.

The Docker image (`Dockerfile`) is `make install PREFIX=/usr` on Debian,
with a C compiler and make for Compile. Its entrypoint,
`scripts/docker-entrypoint.sh`, starts a virtual screen with its VNC
server (Xvnc), a window manager (matchbox) and noVNC (websockify) on port
6080, then lineit, again whenever it quits, as the owner of `/work`, which
is also `HOME`. The page asks Xvnc for a screen the size of the browser's
window, and matchbox keeps lineit as big as the screen. CI builds it, checks
that noVNC reaches the VNC server, that lineit stays up and that Compile
works, and pushes it to `ghcr.io` with the release: `nightly`, or the
version and `latest`.

= Configuration

`config.lua` is a Lua file read with `conf.h` into one struct, `cfg`, whose
initial values are the defaults (kept in `cfg_defaults`). Every key is
optional: a missing key keeps its default, a key with the wrong type is
reported and also keeps it. Colors are `{ r, g, b }` or `{ r, g, b, a }`,
and pin colors are per type name (`colors.types.int`).

Every key is a `Setting` in one table, `settings`: its key, type, where it
goes in `cfg`, its section and subsection, a description, the range a
number can have, and whether it's only read at start. `load_config` reads
them all from it, and the settings tab (File > Settings) shows them in its
order. After the config, `load_config` reads `settings.lua`, in the same
folder, written by the settings tab with what it changed: it wins, and the
config itself is never written. A change in the tab writes it
(`settings_write`, the keys nested in tables like the config's) and calls
`config_reload`.

The colors are a `Colors`. `theme` is read first: one of the built-in
`themes`, or one of the config's `themes` list (`{ name, base, colors }`,
its base with its colors over it). Then the `colors.*` of the config and of
`settings.lua` change some of them.

The settings tab is a `Tab` with `settings` set: its canvas is empty and
isn't shown, `draw_settings` draws instead, and only the shortcuts of the
tabs work there, the rest of the keys go to its search box.

The sections are `theme`, `themes`, `window`, `canvas`, `colors`, `font`, `modules` (where they
are loaded from, where the user's go, where their sources are for Compile,
and the cache for packages), `save`, `output`, `compile` (output folder,
runtime, compiler, flags, objcopy) and `menu`. The paths are commented out
in `config.lua`: their defaults depend on where lineit is (see
@sec-install).

= Tests <sec-tests>

`make test` runs three things, all with the real editor: the test tools
`#include "../src/main.c"` with its `main` renamed and `GetTime` replaced
by a fake clock, so they run the same code without user input.

- `run_tests` loads every canvas in `examples/` and `tests/`, presses Run,
  runs 600 frames (10 s) of fake time at 60 per second, and compares what
  was printed and the text of every square with `X.out` next to it. A
  canvas that takes longer than 10 real seconds fails. `-u` rewrites the
  `.out` files.
- `run_tests -c` compiles the same canvases instead, runs the programs with
  `--frames 600`, and compares what they print with the same `.out`. Run
  and Compile have to agree.
- `test_extract` makes subgraphs out of parts of the example canvases and
  checks they print the same as before and have the ports they should.

`make test-asan` does it all again with AddressSanitizer and UBSan: any
memory error, leak or undefined behavior fails.

CI (`.github/workflows/ci.yml`) runs on every push to main and every pull
request:

- both test runs, built with gcc and with clang, `WERROR=1`, under Xvfb
  since the tests open a hidden window,
- `make check-format` and shellcheck on the scripts,
- `make analyze`, clang's static analyzer, which fails on any finding,
- `make docs`,
- `make appimage dist`, on the oldest runner (an AppImage needs a glibc at
  least as old as the one it was built with), then checks the AppImage
  compiles and runs a canvas from outside the source tree, that the
  tarball builds on its own and is the same bytes made again,
- `install.sh` on that AppImage, served as a release from a folder,
- and `ok`, which passes only if all of them did: the release waits for
  it.

`make examples` regenerates the canvases in `examples/` with
`tools/gen_examples.c`.

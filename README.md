# lineit

**A visual programming language: you wire squares on a canvas, run the
program in the editor, step through it, or compile it to a native binary.**

[![CI](https://github.com/hugoocoto/lineit/actions/workflows/ci.yml/badge.svg)](https://github.com/hugoocoto/lineit/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/hugoocoto/lineit)](LICENSE)

![A program made in lineit: an entry ticks a loop from 1 to 4, typed in
its fields, whose body ticks a print of "Help, I am stuck in a loop!", added
from the search with a wire from the loop. Run prints it three
times](assets/demo.gif)

# Try it

On x86_64 Linux:

    curl -fsSL https://github.com/hugoocoto/lineit/releases/latest/download/install.sh | bash
    lineit

The canvas starts with an `entry` square. Drag a wire from its output to
an empty spot, type `print` and press Enter, type `Hello, world!` and press
Enter. Then, in the bar at the bottom, open the `v` next to **Step** and
pick **Run**.

On Mac or Windows, with [Docker](https://docs.docker.com/get-docker/), run
this in the folder to work in and open <http://localhost:6080>:

    docker run --rm -p 127.0.0.1:6080:6080 -v "$PWD":/work ghcr.io/hugoocoto/lineit:nightly

lineit is in alpha: expect breaking changes with any commit.

# What it does

- **Run, step or compile the same canvas.** Run it in the editor, go
  through it a square at a time, or compile it to a program that runs
  without the editor.
- **Parallel by default.** A tick wired to several squares runs them as
  branches, on threads, and they meet again where their wires do.
- **Calls C directly.** `ffi/call` calls any function of any C library from
  its prototype, without a module written for it.
- **Extended in C, or with squares.** A module is a C file, rebuilt and
  reloaded while the editor runs; a selection of squares becomes a module
  of its own (a subgraph) with Ctrl+G.

![FizzBuzz in the lineit editor: a loop square sends each index to three
modulo and equal checks, and three ifelse squares pick what to
print](assets/screenshot.png)

The canvases in [`examples/`](examples/README.md) show the rest, each
explained in a comment at its top.

# Using the editor

| Do this                           | To                                                      |
|-----------------------------------|---------------------------------------------------------|
| Right click on the canvas         | Add a module: type to search, or browse "Add module"    |
| Right click on a square           | Configure, disconnect, duplicate, delete, make subgraph |
| Choose file... in a square's menu | Its path, relative to the canvas; Tab completes         |
| Drag from an output to an input   | Wire them (drag a plugged input to move its wire)       |
| Drag from an output to the canvas | Search a module to add there, wired to that output      |
| Drag a plugged wire to the canvas | Remove it                                               |
| Drop image files on the canvas    | An `image/open` for each (on a square: its path)        |
| Click a wire, then Delete         | Remove it (Shift+click selects more)                    |
| Click a field next to an input    | Type a value used while nothing is plugged in           |
| Click a field low in a square     | Its settings (params); double click shows them too      |
| Tab, Shift+Tab in a field         | The square's next / previous field                      |
| Space or Enter in a field         | Toggle a checkbox, open a list                          |
| Drag the canvas, or Shift+click   | Select with a box, or add to the selection              |
| Right drag / mouse wheel          | Pan / zoom                                              |
| Delete, Backspace                 | Delete the selection                                    |
| Ctrl+C, Ctrl+X, Ctrl+V, Ctrl+D    | Copy, cut, paste, duplicate                             |
| Ctrl+Z, Ctrl+Y (or Ctrl+Shift+Z)  | Undo, redo                                              |
| Ctrl+A, Escape                    | Select all, select none                                 |
| Ctrl+S, Ctrl+Shift+S, Ctrl+O      | Save, save as, open                                     |
| Ctrl+T, Ctrl+Shift+T, Ctrl+W      | New tab, reopen closed tab, close tab                   |
| Ctrl+Tab, Ctrl+PageUp/PageDown    | Next / previous tab                                     |
| Ctrl+G                            | Make a subgraph of the selection                        |
| Ctrl+,                            | Settings                                                |

The bar at the bottom has the run button, **Output** (what the program
prints) and **File**. The run button does what its arrow chose last:
**Step**, **Run** or **Compile** (and **Stop** while it runs). Stepping, the
square that runs next has a yellow outline. Hovering a pin shows what it
carries.

    lineit -f file.lua            # open a canvas
    lineit -t file.lua            # open it in a new tab of the lineit already open
    lineit --compile file.lua     # compile it without opening a window

# The language

**Squares and wires.** Each square is a module. Wires carry values, or
ticks: the signal that says what runs next. A program starts at its `entry`
squares (and its clocks, every frame).

**Branches.** A tick output wired to several inputs runs each of them as a
branch, in parallel, and goes on once they're all done. Where branches meet
again (a tick input with more than one wire), the square runs once, after
all of them. As in any parallel program, two branches that read and change
one variable at once can lose a change.

**Variables.** A `var/var` square declares one: its name, its type, and
the value it starts with on Run. Wire it to `var/get`, `var/set` and the
`list/` and `map/` squares, or type its name in their field (`::name` for
a global one). `var/var` squares with the same name are the same variable.
Inside a subgraph, variables are its own, like a C function's locals.

**Calling C.** In `ffi/call`, set `lib` (`m` for libm, `c`, or a path to a
`.so`) and `fn`, a C prototype such as `double pow(double x, double y)`, and
the square gets an input per argument and a `result`. Structs are custom
types, `ffi/callback` turns squares into a C function pointer, and
`ffi/alloc`, `ffi/read`, `ffi/write` and `ffi/free` handle memory; the top of
[`modules/ffi/call.c`](modules/ffi/call.c) says how each type is passed. It's
C: a wrong prototype or pointer crashes lineit.

# Writing modules

A module is a C file in `modules/` that includes
[`src/lineit.h`](src/lineit.h), which documents the whole API. `make`
builds it, and the editor reloads it while it runs. The smallest,
`modules/io/print.c`:

```c
#include "lineit.h"

void module_init(Li_Node *n) {
        li_input(n, "tick", LI_TICK);
        li_input_value(n, "value", LI_ANY, li_string(""));
        li_output(n, "then", LI_TICK);
}

void module_tick(Li_Node *n, int input) {
        char buf[64];
        li_format(li_get(n, 1), buf, sizeof buf);
        li_print(n, "%s", buf);
        li_tick(n, 0);
}
```

To share one, make a package, a `.li` file with the module built and its
source, and put it with your modules (`~/.local/share/lineit/modules`):

    tools/lineit-pack -o reverse.li examples/package/Makefile
    tools/lineit-pack --howto    # what a package needs

Packages are native code: only use the ones you trust.

# Install

**The install script** puts the AppImage in `~/.local/bin` (`/usr/local`
as root) with a menu entry, and checks it against the release's checksums.
It needs FUSE (`fuse` or `fuse3`); Compile needs a C compiler and make.

    ... | bash -s -- v0.0.1       # a version, instead of the latest (nightly)
    ... | bash -s -- uninstall
    ... | bash -s -- --help

**Docker.** The editor runs in a virtual screen shown in the browser: some
input lag, copy and paste through noVNC's clipboard panel (the tab on the
left), and compiled programs are Linux ones. The folder is mounted as
`/work`, where your canvases, config and modules are kept. Update it with
`docker pull ghcr.io/hugoocoto/lineit:nightly`. Keep the `127.0.0.1:`:
without it, anyone who reaches port 6080 gets the editor, and Compile runs
commands.

**From source**, on Debian or Ubuntu (on Arch:
`base-devel lua51 fontconfig libx11 libxrandr libxinerama libxcursor libxi
zlib openssl libffi zip curl`):

    sudo apt install build-essential pkg-config liblua5.1-0-dev lua5.1 \
        libfontconfig1-dev libx11-dev zlib1g-dev libssl-dev libffi-dev zip curl \
        libxrandr-dev libxinerama-dev libxcursor-dev libxi-dev
    git clone --recursive https://github.com/hugoocoto/lineit
    cd lineit
    make -j"$(nproc)"
    ./lineit -f examples/hello.lua

`make install` installs it in `~/.local` (`PREFIX`). The first `make`
downloads raylib's source (checked against its SHA-256) and builds it, so
the `image/` squares read png, bmp, gif, qoi, jpg, tga, psd, hdr, pic and
pnm, and write png, bmp, qoi, jpg and tga. Offline, `make RAYLIB=repo` uses
the raylib in the repo instead: it needs glibc 2.38 (not Ubuntu 22.04 or
Debian 12) and doesn't need the `libx*-dev` above but `libx11-dev`, and its
images are only png, bmp, gif and qoi (and it writes png, bmp, qoi).

# Configuration

**File > Settings** (Ctrl+,) has every setting, searchable, and applies
changes at once. They are saved to `settings.lua`, next to `config.lua`,
which has them all commented (`~/.config/lineit/config.lua` installed).
Themes: `dark`, `light`, `nord`, `gruvbox`, or your own.

If text looks wrong, the font isn't installed: the default is Iosevka NFM,
and `font.name` takes another (see `fc-list`).

Canvases and configs are Lua: only open the ones you trust.

# More

- [`docs/internals.typ`](docs/internals.typ): how it works inside. Its PDF
  is in the [docs release](https://github.com/hugoocoto/lineit/releases/tag/docs).
- [CONTRIBUTING.md](CONTRIBUTING.md): building, testing and sending
  changes. Security problems: [SECURITY.md](.github/SECURITY.md).
- License: GPLv3 or later ([LICENSE](LICENSE)). Your canvases are yours; a
  compiled program includes lineit's runtime and modules, so it's GPL too.

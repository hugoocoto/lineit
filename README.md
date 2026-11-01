# lineit

[![CI](https://github.com/hugoocoto/lineit/actions/workflows/ci.yml/badge.svg)](https://github.com/hugoocoto/lineit/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/hugoocoto/lineit)](LICENSE)

A graphical yet compiled modular programming language.


You write programs by placing squares on a canvas and wiring them together. Each
square is a module, and wires carry either values or ticks, the signal that
controls flow. The same canvas can be run inside the editor or compiled to a
native program.

![FizzBuzz in the lineit editor: a loop square sends each index to three
modulo and equal checks, and three ifelse squares pick what to
print](assets/screenshot.png)

![Writing Hello, World! from the entry module: a wire dragged from it to
the canvas opens the module search, print is added wired to it, its value
set to "Hello, World!", then Run, and Output shows it](assets/hello.gif)

# Disclaimer

This is the Alpha version of the editor/compiler. You must expect breaking
changes on every commit. I appreciate any kind of collaboration, from feedback
to features, but don't try to write anything on this language with the aim of
preserving it along the versions.

# Install

On x86_64 Linux:

    curl -fsSL https://github.com/hugoocoto/lineit/releases/latest/download/install.sh | bash

It installs the AppImage as `~/.local/bin/lineit` (`/usr/local` as root),
with its entry in the applications menu. It asks what and where first, with
its suggestion filled in (Enter takes it), and checks the download against
the release's `SHA256SUMS`, and `SHA256SUMS` against the release's
attestation (proof that CI built it from this repo) if
[`gh`](https://cli.github.com) is installed and logged in. Arguments go
after `bash -s --`:

    ... | bash -s -- v0.0.1       # that version, instead of the latest (nightly)
    ... | bash -s -- uninstall
    ... | bash -s -- --help       # the rest

The AppImage needs FUSE (the `fuse` or `fuse3` package). Compile, in the
editor, needs a C compiler and make. The latest
[release](https://github.com/hugoocoto/lineit/releases) is nightly, built
from every push to main; versions (`v0.0.1`) are releases of their own.
Both have the AppImage, the source with the submodules, `install.sh` and
`SHA256SUMS`. The docs as PDF are in the
[docs](https://github.com/hugoocoto/lineit/releases/tag/docs) release, also
from main, and in each version's.

## Mac and Windows: Docker

On any system with [Docker](https://docs.docker.com/get-docker/), lineit
runs in a container and shows up in the browser. From the folder you want
to work in:

    docker run --rm -p 127.0.0.1:6080:6080 -v "$PWD":/work ghcr.io/hugoocoto/lineit:nightly

and open <http://localhost:6080>. On Windows, in PowerShell, it's
`-v "${PWD}:/work"`. The folder is mounted as `/work`: lineit saves your
canvases there, and keeps its config, your modules and what Compile makes
in it too (`.config/lineit`, `.local/share/lineit`, `.cache/lineit`), so
they're still there next time. Arguments after the image go to lineit
(`... ghcr.io/hugoocoto/lineit:nightly -f examples/hello.lua`, a path in
the folder). `:nightly` is the build of the last commit on main;
versions, once there are any, are `:v0.0.1` and so on, and `:latest`.

Docker downloads the image the first time and then keeps using that copy,
so to update it, download it again and start lineit as before:

    docker pull ghcr.io/hugoocoto/lineit:nightly

(or add `--pull always` to `docker run`, to check every time).
`docker image prune` removes the old one. What's in your folder isn't
touched.

The catches:

- Some input lag, and it's drawn by the CPU: it's a virtual screen sent to
  the browser (Xvnc and noVNC). The screen takes the size of the browser's
  window, and lineit fills it.
- Copy and paste between lineit and your system go through noVNC's
  clipboard panel, in the tab on the left of the page.
- The programs Compile makes are Linux ones: on Mac and Windows they only
  run in the container (`docker exec`).
- `127.0.0.1:` keeps the page to your machine. Without it anyone who can
  reach port 6080 gets the editor, and Compile runs commands.
- On Linux it runs as the owner of the folder; if that doesn't work, add
  `--user "$(id -u):$(id -g)"`.

The image is built from the `Dockerfile` (`docker build -t lineit .`).

## From source

raylib 6.0 comes with the repository, built with glibc 2.38. With an older
one (Ubuntu 22.04, Debian 12) `make RAYLIB=source` builds it from its source,
which also needs `libxrandr-dev libxinerama-dev libxcursor-dev libxi-dev`.
The rest are system packages. On Debian or Ubuntu:

    sudo apt install build-essential pkg-config liblua5.1-0-dev lua5.1 \
        libfontconfig1-dev libx11-dev zlib1g-dev libssl-dev zip

On Arch: `base-devel lua51 fontconfig libx11 zlib openssl zip`. `zip` and
`lua5.1` are only needed to make packages (see Packages). LuaJIT works
instead of Lua 5.1: `make LUA=luajit`.

The small libraries in `thirdparty/` are git submodules:

    git clone --recursive https://github.com/hugoocoto/lineit
    cd lineit
    make

If you cloned without `--recursive`, run `git submodule update --init`
first. Then:

    ./lineit                          # opens canvas.lua (save.path in the config)
    ./lineit -f examples/hello.lua    # opens that canvas
    ./lineit -t examples/clock.lua    # in a new tab of the lineit already open
    ./lineit --compile examples/hello.lua   # compile it without opening a window

`make install` puts it in `~/.local` (or `PREFIX`), and `make uninstall`
takes it out. Run from the source tree, lineit uses the tree (`build/`,
`modules/`, its `config.lua`); installed, it finds what comes with it next
to itself, and keeps your own things in `~/.config/lineit`,
`~/.local/share/lineit` and `~/.cache/lineit`.

| Command             | What it does                                                   |
|---------------------|----------------------------------------------------------------|
| `make`              | the editor, the modules and the runtime of Compile             |
| `make WERROR=1`     | the same, failing on any warning, like CI                      |
| `make debug`        | without optimizations, with AddressSanitizer and UBSan         |
| `make test`         | run every canvas in `examples/` and `tests/`, and compile them |
| `make test-asan`    | the same, with AddressSanitizer and UBSan                      |
| `make analyze`      | clang's static analyzer                                        |
| `make format`       | format the C with clang-format (`make check-format` checks)    |
| `make docs`         | `docs/*.typ` to PDF (needs typst)                              |
| `make install`      | install it to `~/.local` (`PREFIX`, `DESTDIR`)                 |
| `make appimage`     | `lineit-x86_64.AppImage` (downloads its pinned tools)          |
| `make dist`         | `lineit-VERSION.tar.gz`, the source with the submodules        |
| `make hooks`        | run `make test` before every `git push`                        |
| `make clean`        | remove what the build made                                     |

The default font is Iosevka NFM. If you don't have it, set `font.name` in
the config to one you do have (see `fc-list`; it has to be a .ttf or .otf).

# Using the editor

| Do this                           | To                                                      |
|-----------------------------------|---------------------------------------------------------|
| Right click on the canvas         | Add a module: type to search, or browse "Add module"    |
| Right click on a square           | Configure, disconnect, duplicate, delete, make subgraph |
| Drag from an output to an input   | Wire them (drag a plugged input to move its wire)       |
| Drag from an output to the canvas | Search a module to add there, wired to that output      |
| Drag a plugged wire to the canvas | Remove it                                               |
| Double click a square             | Its settings (params)                                   |
| Click a field next to an input    | Type a value used while nothing is plugged in           |
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
| Ctrl+,                            | Settings, in a tab                                      |

The bar at the bottom has **Run**, **Compile**, **Output** (what the program
prints) and **File** (tabs, save, new type, autosave, settings). Editing a
module's source and running `make` reloads it in the editor while it runs,
and so does saving `config.lua`.

Start with the canvases in `examples/`: each begins with a comment that
explains it, and `examples/README.md` lists them.

How it all works inside (modules, Run, Compile, packages, hot reload) is in
`docs/internals.typ`. Its PDF is in the
[docs](https://github.com/hugoocoto/lineit/releases/tag/docs) release (it
follows main), or make it with `make docs` (needs typst).

# Modules

A module is a C file in `modules/` that uses `src/lineit.h`, which documents
the whole API. The smallest one, `modules/io/print.c`:

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

# Packages

To give a module to others, make a package: a `.li` file with the
module built for one or more platforms, its source, a license, and a
Makefile that builds it. Put the `.li` with your own modules
(`~/.local/share/lineit/modules` installed, `modules/` in the source tree:
`modules.user` in the config) and it loads like a `.so`.

    tools/lineit-pack -o reverse.li examples/package/Makefile
    tools/lineit-pack --howto    # what a package has to have

`lineit-pack` writes the SHA-256 of every file in it, and lineit loads,
builds or compiles nothing of a package until they match. That finds a
broken or changed file, not who made it: packages are native code, so only
use the ones you trust.

# Configuration

`config.lua` has every setting, commented: window, canvas sizes, theme and
colors, fonts, where modules and saves go, the output panel and how to
compile. Delete a key to use its default. Saving it applies the changes
while lineit runs, except the window size and the modules and save paths,
read at start.

**File > Settings** (Ctrl+,) opens them all in a tab: type to search, the
list on the left goes to a section, and each one says what it does and
where its value comes from. A change applies at once, and is saved to
`settings.lua`, next to `config.lua`: lineit reads it after the config, so
what's there wins, and never rewrites your `config.lua`. **Reset** takes a
setting out of `settings.lua`.

The colors come from a theme, `theme = "dark"`: `dark`, `light`, `nord`,
`gruvbox`, or your own, in `themes` (an example is in `config.lua`). The
colors set in `colors` change the theme's.

lineit reads the one given with `-c`, or else the `config.lua` next to it in
its source tree, or, installed, `~/.config/lineit/config.lua`
(`$XDG_CONFIG_HOME`): copy the one in `share/lineit/` there to start. With
none, it uses the defaults. A `config.lua` in the folder you run it from is
never read on its own: it's Lua, and so are canvases, so only open the ones
you trust.

# Contributing

Bug reports, ideas and pull requests are welcome: see
[CONTRIBUTING.md](CONTRIBUTING.md). Security problems go privately, see
[SECURITY.md](.github/SECURITY.md).

# License

GPLv3 or later, see [LICENSE](LICENSE). In short: you can use, study,
change and share lineit for any purpose, commercial too. If you give anyone
a copy, changed or not, you must also give them its source, under this same
license and with no extra restrictions. It comes with no warranty.

Your canvases are yours. The programs Compile makes include lineit's runtime
and modules, so if you give someone a compiled program, it's covered by the
GPL too.

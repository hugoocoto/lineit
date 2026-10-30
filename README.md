# lineit

A programming graphical compiled language.

You write programs by placing squares on a canvas and wiring them together. Each
square is a module, and wires carry either values or ticks, the signal that
controls flow. The same canvas can be run inside the editor or compiled to a
native program.

# Disclaimer

This is the Alpha version of the editor/compiler. You must expect breaking
changes on every commit. I appreciate any kind of collaboration, from feedback
to features, but don't try to write anything on this language with the aim of
preserving it along the versions.

# Build

lineit runs on Linux (x86_64). raylib 6.0 comes with the repository; the
rest are system packages. On Debian or Ubuntu:

    sudo apt install build-essential pkg-config liblua5.1-0-dev lua5.1 \
        libfontconfig1-dev libx11-dev zlib1g-dev zip

`zip` and `lua5.1` are only needed to make packages (see Packages). LuaJIT
works instead of Lua 5.1: `make LUA=luajit`.

The small libraries in `thirdparty/` are git submodules:

    git clone --recursive https://github.com/hugoocoto/lineit
    cd lineit
    make

If you cloned without `--recursive`, run `git submodule update --init`
first. `make` builds:

- `lineit`, the editor
- `build/modules/`, every module in `modules/` (one `.so` per `.c`)
- `build/rt/`, the runtime that compiled programs link with

Then:

    ./lineit                          # opens canvas.lua (save.path in the config)
    ./lineit -f examples/hello.lua    # opens that canvas
    ./lineit -t examples/clock.lua    # in a new tab of the lineit already open
    ./lineit --compile examples/hello.lua   # compile it without opening a window

The default font is Iosevka NFM. If you don't have it, set `font.name` in
`config.lua` to one you do have (see `fc-list`; it has to be a .ttf or .otf).

# Using the editor

| Do this                           | To                                                      |
|-----------------------------------|---------------------------------------------------------|
| Right click on the canvas         | Add a module: type to search, or browse "Add module"    |
| Right click on a square           | Configure, disconnect, duplicate, delete, make subgraph |
| Drag from an output to an input   | Wire them (drag a plugged input to move its wire)       |
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

The bar at the bottom has **Run**, **Compile**, **Output** (what the program
prints) and **File** (tabs, save, new type, autosave). Editing a module's
source and running `make` reloads it in the editor while it runs, and so
does saving `config.lua`.

Start with the canvases in `examples/`: each begins with a comment that
explains it, and `examples/README.md` lists them.

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

To give a module to others, make a package: a `.li` file (a zip) with the
module built for one or more platforms, its source, a license, and a
Makefile that builds it. Put the `.li` in `build/modules` and it loads like
a `.so`.

    tools/lineit-pack -o build/modules/string/reverse.li examples/package/Makefile
    tools/lineit-pack --howto    # what a package has to have

# Configuration

`config.lua` has every setting, commented: window, canvas sizes, colors,
fonts, where modules and saves go, the output panel and how to compile.
Delete a key to use its default. Saving it applies the changes while lineit
runs, except the window size and the modules and save paths, read at start.

# License

GPLv3 or later, see [LICENSE](LICENSE).


# TODO

- VIM MODE!
- Modules are weird
- UI and modules are ugly as fuck
- Edit config from the GUI
- Proper setup: make install, ~/.config/lineit, releases...
- Proper documentation


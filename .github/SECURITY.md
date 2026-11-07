# Security

## Reporting a vulnerability

Please don't open a public issue. Report it privately instead, from the
repo's Security tab: [Report a vulnerability](https://github.com/hugoocoto/lineit/security/advisories/new).

Say what you found, how to reproduce it and which version (`lineit
--version`, and the commit if you built it). You'll get an answer within a
week. Once it's fixed, the advisory is published with credit to you, unless
you'd rather not.

## Supported versions

lineit is in alpha: fixes go on main as soon as they're ready, and there
are no older versions to patch.

## What's covered

The editor, the interpreter, the runtime that compiled programs are built
with (`lineit-rt`), the modules in this repo, and how lineit reads canvases
and loads packages (`.lipkg`).

Canvases (and record types, and package manifests) are read in a sandbox:
Lua with no libraries and limits on time and memory. A canvas file that
gets out of it, or makes lineit do anything but show a canvas, is a
vulnerability. Running the canvas is another thing: its program does what
its squares do, as any program would.

Keep in mind what the rest is: the config is Lua that lineit runs, and Lua
modules and Rust modules (in packages) are code that runs with your
permissions. Installing a package from someone you don't trust is like
running their program. That isn't a vulnerability; lineit doing something
you didn't ask for with files you trust is.

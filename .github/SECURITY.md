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

The editor, the runtime that compiled programs link with, the modules in
this repo, and how lineit loads packages (`.li`).

Keep in mind what lineit is: modules and packages are native code that
runs with your permissions, and canvases, type files and the config are Lua
that lineit runs to read them. Opening a canvas or loading a module from
someone you don't trust is like running their program. That isn't a
vulnerability; lineit doing something you didn't ask for with files you
trust is.

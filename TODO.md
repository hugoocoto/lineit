# TODO

## Features

- VIM MODE!
- Modules are weird
- UI and modules are ugly as fuck - Themes?
- Edit config from the GUI
- Proper documentation
- Fully parallel squares: today a square runs one tick or read at a time
  (a lock each), so two branches that reach it take turns there. Letting
  them run it at once needs every module thread-safe (no static buffers,
  li_data per call...)
- list/map, list/filter, list/reduce: a list and a function (a subgraph, one
  item in, one result out). Sequential first; later on every core when the
  function is pure (no ticks out, no writes to shared variables), reduce as a
  tree when its operation is associative


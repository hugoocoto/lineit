-- math/accumulate: a subgraph, a canvas used as a module (see Subgraphs in
-- src/main.c). On tick, total = total + value, then ticks then. total is a
-- local variable: each accumulate square has its own, like a C function's
-- static local, and the canvas around it doesn't see it.
-- Open it with Open in a new tab (right click on the square) to change it.
lineit_canvas = "0.0.1" -- the lineit that wrote it, see LI_VERSION

camera = { x = 250, y = 100, offset_x = 640, offset_y = 344, zoom = 1 }

squares = {
        { id = 0, module = "subgraph/input", x = 0, y = 0, params = { { name = "name", value = "tick" }, }, inputs = { { name = "type", value = "tick" }, } },
        { id = 1, module = "subgraph/input", x = 0, y = 100, params = { { name = "name", value = "value" }, }, inputs = { { name = "type", value = "any" }, } },
        { id = 2, module = "var/get", x = 100, y = 200, params = { { name = "name", value = "total" }, { name = "global", value = "false" }, } },
        { id = 3, module = "math/add", x = 220, y = 150 },
        { id = 4, module = "var/set", x = 340, y = 0, params = { { name = "name", value = "total" }, { name = "global", value = "false" }, } },
        { id = 5, module = "subgraph/output", x = 520, y = 0, params = { { name = "name", value = "then" }, }, inputs = { { name = "type", value = "tick" }, } },
        { id = 6, module = "subgraph/output", x = 520, y = 100, params = { { name = "name", value = "total" }, }, inputs = { { name = "type", value = "any" }, } },
}

links = {
        { from = 0, from_pin = 0, to = 4, to_pin = 0 },
        { from = 2, from_pin = 0, to = 3, to_pin = 0 },
        { from = 1, from_pin = 0, to = 3, to_pin = 1 },
        { from = 3, from_pin = 0, to = 4, to_pin = 1 },
        { from = 4, from_pin = 0, to = 5, to_pin = 1 },
        { from = 4, from_pin = 1, to = 6, to_pin = 1 },
}

-- math/clamp: a subgraph, a canvas used as a module (see Subgraphs in
-- src/main.c). value = x, but at least lo and at most hi: min(max(x, lo), hi).
-- Open it with ./lineit -f modules/math/clamp.lua to change it; `make` copies
-- it to build/modules and the squares using it pick the change up.
lineit_canvas = 1 -- format version, see CANVAS_VERSION

camera = { x = 300, y = 100, offset_x = 640, offset_y = 344, zoom = 1 }

squares = {
        { id = 0, module = "subgraph/input", x = 0, y = 0, params = { { name = "name", value = "x" }, }, inputs = { { name = "type", value = "any" }, } },
        { id = 1, module = "subgraph/input", x = 0, y = 100, params = { { name = "name", value = "lo" }, }, inputs = { { name = "type", value = "any" }, } },
        { id = 2, module = "subgraph/input", x = 0, y = 200, params = { { name = "name", value = "hi" }, }, inputs = { { name = "type", value = "any" }, } },
        { id = 3, module = "math/max", x = 200, y = 50 },
        { id = 4, module = "math/min", x = 400, y = 120 },
        { id = 5, module = "subgraph/output", x = 600, y = 120, params = { { name = "name", value = "value" }, }, inputs = { { name = "type", value = "any" }, } },
}

links = {
        { from = 0, from_pin = 0, to = 3, to_pin = 0 },
        { from = 1, from_pin = 0, to = 3, to_pin = 1 },
        { from = 3, from_pin = 0, to = 4, to_pin = 0 },
        { from = 2, from_pin = 0, to = 4, to_pin = 1 },
        { from = 4, from_pin = 0, to = 5, to_pin = 1 },
}

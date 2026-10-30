-- log: a subgraph, a canvas used as a module (see Subgraphs in src/main.c).
-- On tick, prints "label: value" (string/format + print), then ticks then.
-- Open it with ./lineit -f modules/log.lua to change it; `make` copies it to
-- build/modules and the squares using it pick the change up.
camera = { x = 300, y = 100, offset_x = 640, offset_y = 344, zoom = 1 }

squares = {
        { id = 0, module = "subgraph/input", x = 0, y = 0, params = { { name = "name", value = "tick" }, }, inputs = { { name = "type", value = "tick" }, } },
        { id = 1, module = "subgraph/input", x = 0, y = 100, params = { { name = "name", value = "label" }, }, inputs = { { name = "type", value = "string" }, } },
        { id = 2, module = "subgraph/input", x = 0, y = 200, params = { { name = "name", value = "value" }, }, inputs = { { name = "type", value = "any" }, } },
        { id = 3, module = "string/format", x = 200, y = 150, inputs = { { name = "template", type = "string", value = "{}: {}" }, } },
        { id = 4, module = "io/print", x = 400, y = 0 },
        { id = 5, module = "subgraph/output", x = 600, y = 0, params = { { name = "name", value = "then" }, }, inputs = { { name = "type", value = "tick" }, } },
}

links = {
        { from = 1, from_pin = 0, to = 3, to_pin = 1 },
        { from = 2, from_pin = 0, to = 3, to_pin = 2 },
        { from = 0, from_pin = 0, to = 4, to_pin = 0 },
        { from = 3, from_pin = 0, to = 4, to_pin = 1 },
        { from = 4, from_pin = 0, to = 5, to_pin = 1 },
}

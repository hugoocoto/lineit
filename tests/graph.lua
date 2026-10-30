-- graph: subgraphs (modules/math/clamp.lua, modules/log.lua), checked by
-- `make test` (see tools/run_tests.c)
squares = {
        { id = 0, module = "math/clamp", x = 0, y = 0, inputs = { { name = "x", type = "int", value = "15" }, { name = "lo", type = "int", value = "0" }, { name = "hi", type = "int", value = "10" }, } },
        { id = 1, module = "math/clamp", x = 0, y = 150, inputs = { { name = "x", type = "float", value = "-2.5" }, { name = "lo", type = "int", value = "0" }, { name = "hi", type = "int", value = "10" }, } },
        { id = 2, module = "io/display", x = 200, y = 0 },
        { id = 3, module = "io/display", x = 200, y = 150 },
        -- log the clamped value of every index of a loop -5..14, then "done"
        { id = 4, module = "flow/entry", x = 0, y = 300 },
        { id = 5, module = "flow/loop", x = 150, y = 300, inputs = { { name = "from", type = "int", value = "-5" }, { name = "to", type = "int", value = "15" }, } },
        { id = 6, module = "math/clamp", x = 150, y = 450, inputs = { { name = "lo", type = "int", value = "0" }, { name = "hi", type = "int", value = "9" }, } },
        { id = 7, module = "io/log", x = 300, y = 300, inputs = { { name = "label", type = "string", value = "clamped" }, } },
        { id = 8, module = "io/log", x = 450, y = 300, inputs = { { name = "label", type = "string", value = "after" }, } },
        { id = 9, module = "io/print", x = 600, y = 300, inputs = { { name = "value", type = "string", value = "done" }, } },
}

links = {
        { from = 0, from_pin = 0, to = 2, to_pin = 0 },
        { from = 1, from_pin = 0, to = 3, to_pin = 0 },
        { from = 4, from_pin = 0, to = 5, to_pin = 0 },
        { from = 5, from_pin = 1, to = 6, to_pin = 0 },
        { from = 5, from_pin = 0, to = 7, to_pin = 0 },
        { from = 6, from_pin = 0, to = 7, to_pin = 2 },
        { from = 5, from_pin = 2, to = 8, to_pin = 0 },
        { from = 5, from_pin = 1, to = 8, to_pin = 2 },
        { from = 8, from_pin = 0, to = 9, to_pin = 0 },
}

-- scope: variables are local to their canvas, like in C, checked by
-- `make test` (see tools/run_tests.c). Two math/accumulate squares (a
-- subgraph with a local "total") count on their own, and this canvas sees
-- neither total. ::g is global, and g, a local, isn't it.
lineit_canvas = "0.0.1" -- the lineit that wrote it, see LI_VERSION

squares = {
        { id = 0, module = "flow/entry", x = 0, y = 0 },
        { id = 1, module = "var/set", x = 150, y = 0, inputs = { { name = "value", type = "int", value = "100" }, } },
        { id = 2, module = "var/set", x = 300, y = 0, inputs = { { name = "value", type = "int", value = "7" }, } },
        { id = 3, module = "flow/loop", x = 450, y = 0, inputs = { { name = "from", type = "int", value = "0" }, { name = "to", type = "int", value = "5" }, } },
        { id = 4, module = "math/accumulate", x = 600, y = 0 },
        { id = 5, module = "math/accumulate", x = 750, y = 0, inputs = { { name = "value", type = "int", value = "10" }, } },
        { id = 6, module = "io/display", x = 600, y = 200 },
        { id = 7, module = "io/display", x = 750, y = 200 },
        { id = 8, module = "var/get", x = 0, y = 300 },
        { id = 9, module = "var/get", x = 150, y = 300 },
        { id = 10, module = "var/get", x = 300, y = 300 },
        { id = 11, module = "var/var", x = 0, y = 150, params = { { name = "name", value = "total" }, }, inputs = { { name = "type", value = "int" }, { name = "start", type = "int", value = "0" }, } },
        { id = 12, module = "var/var", x = 150, y = 150, params = { { name = "name", value = "g" }, { name = "global", value = "true" }, }, inputs = { { name = "type", value = "int" }, { name = "start", type = "int", value = "0" }, } },
        { id = 13, module = "var/var", x = 300, y = 450, params = { { name = "name", value = "g" }, }, inputs = { { name = "type", value = "any" }, { name = "start", type = "none", value = "" }, } },
}

links = {
        { from = 0, from_pin = 0, to = 1, to_pin = 0 },
        { from = 11, from_pin = 0, to = 1, to_pin = 1 },
        { from = 12, from_pin = 0, to = 2, to_pin = 1 },
        { from = 11, from_pin = 0, to = 8, to_pin = 0 },
        { from = 12, from_pin = 0, to = 9, to_pin = 0 },
        { from = 13, from_pin = 0, to = 10, to_pin = 0 },
        { from = 1, from_pin = 0, to = 2, to_pin = 0 },
        { from = 2, from_pin = 0, to = 3, to_pin = 0 },
        { from = 3, from_pin = 0, to = 4, to_pin = 0 },
        { from = 3, from_pin = 1, to = 4, to_pin = 1 },
        { from = 4, from_pin = 0, to = 5, to_pin = 0 },
        { from = 4, from_pin = 1, to = 6, to_pin = 0 },
        { from = 5, from_pin = 1, to = 7, to_pin = 0 },
}

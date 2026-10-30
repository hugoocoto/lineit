-- types: custom types (modules/types/*.type.lua), checked by `make test`
-- (see tools/run_tests.c). Make and break, nested types, and records kept
-- in a variable and in a list.
lineit_canvas = "0.0.1" -- the lineit that wrote it, see LI_VERSION

squares = {
        { id = 0, module = "types/make_point", x = 0, y = 0, inputs = { { name = "x", type = "float", value = "1.5" }, { name = "y", type = "float", value = "2" }, } },
        { id = 1, module = "io/display", x = 200, y = -100 },
        { id = 2, module = "types/break_point", x = 200, y = 0 },
        { id = 3, module = "math/add", x = 350, y = 0 },
        { id = 4, module = "io/display", x = 500, y = 0 },
        -- a segment of two points, and its end point back
        { id = 5, module = "types/make_point", x = 0, y = 200, inputs = { { name = "x", type = "float", value = "4" }, { name = "y", type = "float", value = "6" }, } },
        { id = 6, module = "types/make_segment", x = 200, y = 200 },
        { id = 7, module = "io/display", x = 350, y = 150 },
        { id = 8, module = "types/break_segment", x = 350, y = 250 },
        { id = 9, module = "types/break_point", x = 500, y = 250 },
        { id = 10, module = "io/display", x = 650, y = 250 },
        -- kept: in a variable and in a list, copies that outlive the squares' records
        { id = 11, module = "flow/entry", x = 0, y = 400 },
        { id = 12, module = "var/set", x = 150, y = 400, params = { { name = "name", value = "p" }, } },
        { id = 13, module = "list/push", x = 300, y = 400, params = { { name = "list", value = "segments" }, } },
        { id = 14, module = "var/get", x = 150, y = 550, params = { { name = "name", value = "p" }, } },
        { id = 15, module = "io/display", x = 300, y = 550 },
        { id = 16, module = "list/get", x = 450, y = 550, params = { { name = "list", value = "segments" }, }, inputs = { { name = "index", type = "int", value = "0" }, } },
        { id = 17, module = "types/break_segment", x = 600, y = 550 },
        { id = 18, module = "io/display", x = 750, y = 550 },
}

links = {
        { from = 0, from_pin = 0, to = 1, to_pin = 0 },
        { from = 0, from_pin = 0, to = 2, to_pin = 0 },
        { from = 2, from_pin = 0, to = 3, to_pin = 0 },
        { from = 2, from_pin = 1, to = 3, to_pin = 1 },
        { from = 3, from_pin = 0, to = 4, to_pin = 0 },
        { from = 0, from_pin = 0, to = 6, to_pin = 0 },
        { from = 5, from_pin = 0, to = 6, to_pin = 1 },
        { from = 6, from_pin = 0, to = 7, to_pin = 0 },
        { from = 6, from_pin = 0, to = 8, to_pin = 0 },
        { from = 8, from_pin = 1, to = 9, to_pin = 0 },
        { from = 9, from_pin = 1, to = 10, to_pin = 0 },
        { from = 11, from_pin = 0, to = 12, to_pin = 0 },
        { from = 0, from_pin = 0, to = 12, to_pin = 1 },
        { from = 12, from_pin = 0, to = 13, to_pin = 0 },
        { from = 6, from_pin = 0, to = 13, to_pin = 1 },
        { from = 14, from_pin = 0, to = 15, to_pin = 0 },
        { from = 16, from_pin = 0, to = 17, to_pin = 0 },
        { from = 17, from_pin = 1, to = 18, to_pin = 0 },
}

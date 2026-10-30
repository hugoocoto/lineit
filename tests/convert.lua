-- convert: the value/convert module, to each type, checked by `make test` (see
-- tools/run_tests.c)
lineit_canvas = 1 -- format version, see CANVAS_VERSION

squares = {
        { id = 0, module = "value/convert", x = 0, y = 0, inputs = { { name = "value", type = "string", value = "42" }, { name = "to", value = "int" }, } },
        { id = 1, module = "value/convert", x = 0, y = 100, inputs = { { name = "value", type = "string", value = " 2.9 " }, { name = "to", value = "int" }, } },
        { id = 2, module = "value/convert", x = 0, y = 200, inputs = { { name = "value", type = "float", value = "-3.7" }, { name = "to", value = "int" }, } },
        { id = 3, module = "value/convert", x = 0, y = 300, inputs = { { name = "value", type = "bool", value = "true" }, { name = "to", value = "int" }, } },
        { id = 4, module = "value/convert", x = 0, y = 400, inputs = { { name = "value", type = "string", value = "abc" }, { name = "to", value = "int" }, } },
        { id = 5, module = "value/convert", x = 0, y = 500, inputs = { { name = "to", value = "int" }, } },
        { id = 6, module = "value/convert", x = 200, y = 0, inputs = { { name = "value", type = "string", value = "2.5" }, { name = "to", value = "float" }, } },
        { id = 7, module = "value/convert", x = 200, y = 100, inputs = { { name = "value", type = "int", value = "7" }, { name = "to", value = "float" }, } },
        { id = 8, module = "value/convert", x = 200, y = 200, inputs = { { name = "value", type = "string", value = "1e3" }, { name = "to", value = "float" }, } },
        { id = 9, module = "value/convert", x = 200, y = 300, inputs = { { name = "value", type = "string", value = "2.5x" }, { name = "to", value = "float" }, } },
        { id = 10, module = "value/convert", x = 200, y = 400, inputs = { { name = "value", type = "string", value = "Yes" }, { name = "to", value = "bool" }, } },
        { id = 11, module = "value/convert", x = 200, y = 500, inputs = { { name = "value", type = "string", value = "" }, { name = "to", value = "bool" }, } },
        { id = 12, module = "value/convert", x = 400, y = 0, inputs = { { name = "value", type = "int", value = "0" }, { name = "to", value = "bool" }, } },
        { id = 13, module = "value/convert", x = 400, y = 100, inputs = { { name = "value", type = "float", value = "0.5" }, { name = "to", value = "bool" }, } },
        { id = 14, module = "value/convert", x = 400, y = 200, inputs = { { name = "value", type = "string", value = "maybe" }, { name = "to", value = "bool" }, } },
        { id = 15, module = "value/convert", x = 400, y = 300, inputs = { { name = "value", type = "float", value = "0.25" }, { name = "to", value = "string" }, } },
        { id = 16, module = "value/convert", x = 400, y = 400, inputs = { { name = "value", type = "bool", value = "false" }, { name = "to", value = "string" }, } },
        { id = 17, module = "value/convert", x = 400, y = 500, inputs = { { name = "to", value = "string" }, } },
        -- wired: "12" as an int, + 1 = 13
        { id = 18, module = "value/convert", x = 800, y = 0, inputs = { { name = "value", type = "string", value = "12" }, { name = "to", value = "int" }, } },
        { id = 19, module = "math/add", x = 900, y = 0, inputs = { { name = "b", type = "int", value = "1" }, } },
        { id = 20, module = "io/display", x = 1000, y = 0 },
}

links = {
        { from = 18, from_pin = 0, to = 19, to_pin = 0 },
        { from = 19, from_pin = 0, to = 20, to_pin = 0 },
}

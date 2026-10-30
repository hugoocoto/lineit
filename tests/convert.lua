-- convert: every convert/ module, checked by `make test` (see tools/run_tests.c)
squares = {
        { id = 0, module = "convert/int", x = 0, y = 0, inputs = { { name = "value", type = "string", value = "42" }, } },
        { id = 1, module = "convert/int", x = 0, y = 100, inputs = { { name = "value", type = "string", value = " 2.9 " }, } },
        { id = 2, module = "convert/int", x = 0, y = 200, inputs = { { name = "value", type = "float", value = "-3.7" }, } },
        { id = 3, module = "convert/int", x = 0, y = 300, inputs = { { name = "value", type = "bool", value = "true" }, } },
        { id = 4, module = "convert/int", x = 0, y = 400, inputs = { { name = "value", type = "string", value = "abc" }, } },
        { id = 5, module = "convert/int", x = 0, y = 500 },
        { id = 6, module = "convert/float", x = 200, y = 0, inputs = { { name = "value", type = "string", value = "2.5" }, } },
        { id = 7, module = "convert/float", x = 200, y = 100, inputs = { { name = "value", type = "int", value = "7" }, } },
        { id = 8, module = "convert/float", x = 200, y = 200, inputs = { { name = "value", type = "string", value = "1e3" }, } },
        { id = 9, module = "convert/float", x = 200, y = 300, inputs = { { name = "value", type = "string", value = "2.5x" }, } },
        { id = 10, module = "convert/bool", x = 400, y = 0, inputs = { { name = "value", type = "string", value = "Yes" }, } },
        { id = 11, module = "convert/bool", x = 400, y = 100, inputs = { { name = "value", type = "string", value = "" }, } },
        { id = 12, module = "convert/bool", x = 400, y = 200, inputs = { { name = "value", type = "int", value = "0" }, } },
        { id = 13, module = "convert/bool", x = 400, y = 300, inputs = { { name = "value", type = "float", value = "0.5" }, } },
        { id = 14, module = "convert/bool", x = 400, y = 400, inputs = { { name = "value", type = "string", value = "maybe" }, } },
        { id = 15, module = "convert/string", x = 600, y = 0, inputs = { { name = "value", type = "float", value = "0.25" }, } },
        { id = 16, module = "convert/string", x = 600, y = 100, inputs = { { name = "value", type = "bool", value = "false" }, } },
        { id = 17, module = "convert/string", x = 600, y = 200 },
        -- wired: "12" + 1 = 13, after convert/int
        { id = 18, module = "convert/int", x = 800, y = 0, inputs = { { name = "value", type = "string", value = "12" }, } },
        { id = 19, module = "math/add", x = 900, y = 0, inputs = { { name = "b", type = "int", value = "1" }, } },
        { id = 20, module = "display", x = 1000, y = 0 },
}

links = {
        { from = 18, from_pin = 0, to = 19, to_pin = 0 },
        { from = 19, from_pin = 0, to = 20, to_pin = 0 },
}

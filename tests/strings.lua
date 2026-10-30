-- strings: every string/ module, checked by `make test` (see tools/run_tests.c)
squares = {
        { id = 0, module = "string/concat", x = 0, y = 0, inputs = { { name = "a", type = "string", value = "n = " }, { name = "b", type = "int", value = "5" }, } },
        { id = 1, module = "string/concat", x = 0, y = 100 },
        { id = 2, module = "string/format", x = 0, y = 200, inputs = { { name = "template", type = "string", value = "{} + {} = {} {{}" }, { name = "a", type = "int", value = "1" }, { name = "b", type = "float", value = "2.5" }, { name = "c", type = "bool", value = "true" }, } },
        { id = 3, module = "string/format", x = 0, y = 300, inputs = { { name = "template", type = "string", value = "[{}] [{}] [{}] [{}]" }, { name = "a", type = "string", value = "x" }, } },
        { id = 4, module = "string/len", x = 0, y = 400, inputs = { { name = "s", type = "string", value = "hello" }, } },
        { id = 5, module = "string/len", x = 0, y = 500 },
        { id = 6, module = "string/substr", x = 200, y = 0, inputs = { { name = "s", type = "string", value = "lineit" }, { name = "start", type = "int", value = "1" }, { name = "length", type = "int", value = "3" }, } },
        { id = 7, module = "string/substr", x = 200, y = 100, inputs = { { name = "s", type = "string", value = "lineit" }, { name = "start", type = "int", value = "-2" }, { name = "length", type = "int", value = "-1" }, } },
        { id = 8, module = "string/substr", x = 200, y = 200, inputs = { { name = "s", type = "string", value = "abc" }, { name = "start", type = "int", value = "10" }, { name = "length", type = "int", value = "2" }, } },
        { id = 9, module = "string/substr", x = 200, y = 300, inputs = { { name = "s", type = "string", value = "abc" }, { name = "start", type = "int", value = "-10" }, { name = "length", type = "int", value = "2" }, } },
        { id = 10, module = "string/find", x = 200, y = 400, inputs = { { name = "s", type = "string", value = "banana" }, { name = "needle", type = "string", value = "nan" }, } },
        { id = 11, module = "string/find", x = 200, y = 500, inputs = { { name = "s", type = "string", value = "banana" }, { name = "needle", type = "string", value = "x" }, } },
        { id = 12, module = "string/upper", x = 400, y = 0, inputs = { { name = "s", type = "string", value = "Hello, World 1!" }, } },
        { id = 13, module = "string/lower", x = 400, y = 100, inputs = { { name = "s", type = "string", value = "Hello, World 1!" }, } },
        { id = 14, module = "flow/entry", x = 400, y = 200 },
        { id = 15, module = "string/split", x = 500, y = 200, params = { { name = "list", value = "parts" }, }, inputs = { { name = "s", type = "string", value = "a,b,,c" }, { name = "sep", type = "string", value = "," }, } },
        { id = 16, module = "string/join", x = 600, y = 200, params = { { name = "list", value = "parts" }, }, inputs = { { name = "sep", type = "string", value = " | " }, } },
        { id = 17, module = "io/print", x = 700, y = 200 },
        { id = 18, module = "string/split", x = 800, y = 200, params = { { name = "list", value = "chars" }, }, inputs = { { name = "s", type = "string", value = "xyz" }, { name = "sep", type = "string", value = "" }, } },
        { id = 19, module = "string/join", x = 900, y = 200, params = { { name = "list", value = "chars" }, }, inputs = { { name = "sep", type = "string", value = "-" }, } },
        { id = 20, module = "io/print", x = 1000, y = 200 },
        -- wired: upper(concat(format, len))
        { id = 21, module = "string/format", x = 400, y = 400, inputs = { { name = "template", type = "string", value = "len {}" }, } },
        { id = 22, module = "string/upper", x = 500, y = 400 },
}

links = {
        { from = 14, from_pin = 0, to = 15, to_pin = 0 },
        { from = 15, from_pin = 0, to = 17, to_pin = 0 },
        { from = 16, from_pin = 0, to = 17, to_pin = 1 },
        { from = 17, from_pin = 0, to = 18, to_pin = 0 },
        { from = 18, from_pin = 0, to = 20, to_pin = 0 },
        { from = 19, from_pin = 0, to = 20, to_pin = 1 },
        { from = 4, from_pin = 0, to = 21, to_pin = 1 },
        { from = 21, from_pin = 0, to = 22, to_pin = 0 },
}

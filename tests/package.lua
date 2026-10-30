-- package: string/reverse, from a .li made by tools/lineit-pack out of
-- examples/package, checked by `make test` (see tools/run_tests.c)
lineit_canvas = 1 -- format version, see CANVAS_VERSION

squares = {
        { id = 0, module = "string/reverse", x = 0, y = 0, inputs = { { name = "s", type = "string", value = "lineit" }, } },
        { id = 1, module = "string/reverse", x = 0, y = 150, inputs = { { name = "s", type = "int", value = "12345" }, } },
        { id = 2, module = "string/upper", x = 200, y = 0 },
}

links = {
        { from = 0, from_pin = 0, to = 2, to_pin = 0 },
}

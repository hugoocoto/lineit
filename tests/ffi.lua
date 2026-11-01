-- ffi: C functions called with ffi/call, by their prototype: pure ones
-- (pow of libm, abs and strlen of libc), a wrong one, a variadic one
-- ticked (snprintf into a buffer of ffi/alloc), and ffi/write, ffi/read
-- and ffi/free. Checked by `make test` (see tools/run_tests.c).
lineit_canvas = "0.0.1" -- the lineit that wrote it, see LI_VERSION

camera = { x = 0, y = 0, offset_x = 0, offset_y = 0, zoom = 1 }

squares = {
        { id = 0, module = "ffi/call", x = 107, y = -83, params = { { name = "lib", value = "m" }, { name = "fn", value = "double pow(double x, double y)" }, { name = "pure", value = "true" }, }, inputs = { { name = "x", type = "float", value = "2" }, { name = "y", type = "float", value = "10" }, } },
        { id = 1, module = "ffi/call", x = 107, y = 142, params = { { name = "lib", value = "" }, { name = "fn", value = "int abs(int)" }, { name = "pure", value = "true" }, }, inputs = { { name = "a1", type = "int", value = "-5" }, } },
        { id = 2, module = "ffi/call", x = 107, y = 367, params = { { name = "lib", value = "c" }, { name = "fn", value = "unsigned long strlen(const char *s)" }, { name = "pure", value = "true" }, }, inputs = { { name = "s", type = "string", value = "hello" }, } },
        { id = 3, module = "ffi/call", x = 107, y = 592, params = { { name = "lib", value = "nolib" }, { name = "fn", value = "int nothing(void)" }, { name = "pure", value = "true" }, } },
        { id = 4, module = "flow/entry", x = -35, y = 865 },
        { id = 5, module = "io/print", x = 306.898438, y = -73.1015625, inputs = { { name = "value", type = "string", value = "" }, } },
        { id = 6, module = "io/print", x = 496.898438, y = -73.1015625, inputs = { { name = "value", type = "string", value = "" }, } },
        { id = 7, module = "io/print", x = 686.898438, y = -73.1015625, inputs = { { name = "value", type = "string", value = "" }, } },
        { id = 8, module = "ffi/alloc", x = 879.6875, y = -70.3125, inputs = { { name = "size", type = "int", value = "64" }, } },
        { id = 9, module = "ffi/call", x = 1057, y = -83, params = { { name = "lib", value = "c" }, { name = "fn", value = "int snprintf(char *buf, size_t n, const char *fmt, ..., int, const char *, double)" }, { name = "pure", value = "false" }, }, inputs = { { name = "buf", type = "string", value = "" }, { name = "n", type = "int", value = "64" }, { name = "fmt", type = "string", value = "%d-%s %.2f" }, { name = "a4", type = "int", value = "42" }, { name = "a5", type = "string", value = "x" }, { name = "a6", type = "float", value = "3.14159" }, } },
        { id = 10, module = "ffi/read", x = 1251.32031, y = 71.3203125, inputs = { { name = "offset", type = "int", value = "0" }, { name = "type", value = "string" }, } },
        { id = 11, module = "io/print", x = 1446.89844, y = -73.1015625, inputs = { { name = "value", type = "string", value = "" }, } },
        { id = 12, module = "io/print", x = 1636.89844, y = -73.1015625, inputs = { { name = "value", type = "string", value = "" }, } },
        { id = 13, module = "ffi/write", x = 1817, y = -83, inputs = { { name = "offset", type = "int", value = "8" }, { name = "type", value = "int" }, { name = "value", type = "int", value = "7" }, } },
        { id = 14, module = "ffi/write", x = 2007, y = -83, inputs = { { name = "offset", type = "int", value = "16" }, { name = "type", value = "double" }, { name = "value", type = "float", value = "2.5" }, } },
        { id = 15, module = "ffi/read", x = 2201.32031, y = 71.3203125, inputs = { { name = "offset", type = "int", value = "8" }, { name = "type", value = "int" }, } },
        { id = 16, module = "ffi/read", x = 2391.32031, y = 71.3203125, inputs = { { name = "offset", type = "int", value = "16" }, { name = "type", value = "double" }, } },
        { id = 17, module = "io/print", x = 2206.89844, y = -73.1015625, inputs = { { name = "value", type = "string", value = "" }, } },
        { id = 18, module = "io/print", x = 2396.89844, y = -73.1015625, inputs = { { name = "value", type = "string", value = "" }, } },
        { id = 19, module = "ffi/free", x = 2613, y = -47 },
}

links = {
        { from = 4, from_pin = 0, to = 5, to_pin = 0 },
        { from = 0, from_pin = 0, to = 5, to_pin = 1 },
        { from = 5, from_pin = 0, to = 6, to_pin = 0 },
        { from = 1, from_pin = 0, to = 6, to_pin = 1 },
        { from = 6, from_pin = 0, to = 7, to_pin = 0 },
        { from = 2, from_pin = 0, to = 7, to_pin = 1 },
        { from = 7, from_pin = 0, to = 8, to_pin = 0 },
        { from = 8, from_pin = 0, to = 9, to_pin = 0 },
        { from = 8, from_pin = 1, to = 9, to_pin = 1 },
        { from = 9, from_pin = 0, to = 11, to_pin = 0 },
        { from = 8, from_pin = 1, to = 10, to_pin = 0 },
        { from = 10, from_pin = 0, to = 11, to_pin = 1 },
        { from = 11, from_pin = 0, to = 12, to_pin = 0 },
        { from = 9, from_pin = 1, to = 12, to_pin = 1 },
        { from = 12, from_pin = 0, to = 13, to_pin = 0 },
        { from = 8, from_pin = 1, to = 13, to_pin = 1 },
        { from = 13, from_pin = 0, to = 14, to_pin = 0 },
        { from = 8, from_pin = 1, to = 14, to_pin = 1 },
        { from = 8, from_pin = 1, to = 15, to_pin = 0 },
        { from = 8, from_pin = 1, to = 16, to_pin = 0 },
        { from = 14, from_pin = 0, to = 17, to_pin = 0 },
        { from = 15, from_pin = 0, to = 17, to_pin = 1 },
        { from = 17, from_pin = 0, to = 18, to_pin = 0 },
        { from = 16, from_pin = 0, to = 18, to_pin = 1 },
        { from = 18, from_pin = 0, to = 19, to_pin = 0 },
        { from = 8, from_pin = 1, to = 19, to_pin = 1 },
}

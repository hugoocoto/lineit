-- pi
-- Pi three ways at once, with C functions and branches in parallel.
-- flow/entry ticks three squares, so each is a branch, on a thread of
-- its own. One sums the Basel series (pi^2 / 6 = sum of 1 / k^2) with
-- pow, then takes sqrt(6 * sum); one sums Leibniz's (pi / 4 = sum of
-- (-1)^k / (2k + 1)), with pow for the sign; one asks libm for atan(1).
-- pow, sqrt and atan are libm's, called with ffi/call from their C
-- prototype, no module written for them. The three meet at ffi/alloc: it
-- waits for all of them, then snprintf (libc's, variadic) writes the
-- three in its buffer, io/print shows it and ffi/free frees it.
-- Each sum is a variable of its own branch (by name, declared by the
-- var/var that start them at 0), so they don't race.
--
-- Open it with: ./lineit -f examples/pi.lua, then press Run.
lineit_canvas = "0.0.1" -- the lineit that wrote it, see LI_VERSION

camera = { x = 1051, y = 714.210938, offset_x = 640, offset_y = 344, zoom = 0.385495007 }

squares = {
        { id = 67, module = "flow/entry", x = -35, y = 415 },
        { id = 68, module = "var/var", x = 154.898453, y = 136.898438, params = { { name = "name", value = "basel" }, { name = "global", value = "false" }, }, inputs = { { name = "type", value = "float" }, { name = "start", type = "int", value = "0" }, } },
        { id = 69, module = "flow/loop", x = 114.109375, y = -75.890625, inputs = { { name = "from", type = "int", value = "1" }, { name = "to", type = "int", value = "20001" }, } },
        { id = 70, module = "ffi/call", x = 297, y = 97, params = { { name = "lib", value = "m" }, { name = "fn", value = "double pow(double k, double e)" }, { name = "pure", value = "true" }, }, inputs = { { name = "k", type = "float", value = "0" }, { name = "e", type = "float", value = "-2" }, } },
        { id = 71, module = "var/get", x = 309.6875, y = 289.6875, inputs = { { name = "var", type = "ref", value = "basel" }, } },
        { id = 72, module = "math/add", x = 510.84375, y = 180.84375, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "0" }, } },
        { id = 73, module = "var/set", x = 684.109375, y = -75.890625, inputs = { { name = "var", type = "ref", value = "basel" }, { name = "value", type = "int", value = "0" }, } },
        { id = 74, module = "math/multiply", x = 879.6875, y = 139.6875, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "6" }, } },
        { id = 75, module = "ffi/call", x = 1057, y = 127, params = { { name = "lib", value = "m" }, { name = "fn", value = "double sqrt(double x)" }, { name = "pure", value = "true" }, }, inputs = { { name = "x", type = "float", value = "0" }, } },
        { id = 76, module = "var/var", x = 154.898453, y = 736.898438, params = { { name = "name", value = "leibniz" }, { name = "global", value = "false" }, }, inputs = { { name = "type", value = "float" }, { name = "start", type = "int", value = "0" }, } },
        { id = 77, module = "flow/loop", x = 114.109375, y = 524.109375, inputs = { { name = "from", type = "int", value = "0" }, { name = "to", type = "int", value = "20000" }, } },
        { id = 78, module = "ffi/call", x = 297, y = 697, params = { { name = "lib", value = "m" }, { name = "fn", value = "double pow(double b, double k)" }, { name = "pure", value = "true" }, }, inputs = { { name = "b", type = "float", value = "-1" }, { name = "k", type = "float", value = "0" }, } },
        { id = 79, module = "math/multiply", x = 309.6875, y = 919.6875, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "2" }, } },
        { id = 80, module = "math/add", x = 510.84375, y = 930.84375, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "1" }, } },
        { id = 81, module = "math/divide", x = 496.898438, y = 736.898438, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "0" }, } },
        { id = 82, module = "var/get", x = 499.6875, y = 1069.6875, inputs = { { name = "var", type = "ref", value = "leibniz" }, } },
        { id = 83, module = "math/add", x = 700.84375, y = 840.84375, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "0" }, } },
        { id = 84, module = "var/set", x = 874.109375, y = 524.109375, inputs = { { name = "var", type = "ref", value = "leibniz" }, { name = "value", type = "int", value = "0" }, } },
        { id = 85, module = "math/multiply", x = 1069.6875, y = 769.6875, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "4" }, } },
        { id = 86, module = "ffi/call", x = 107, y = 1177, params = { { name = "lib", value = "m" }, { name = "fn", value = "double atan(double x)" }, { name = "pure", value = "false" }, }, inputs = { { name = "x", type = "float", value = "1" }, } },
        { id = 87, module = "math/multiply", x = 309.6875, y = 1339.6875, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "4" }, } },
        { id = 88, module = "ffi/alloc", x = 1259.6875, y = 529.6875, inputs = { { name = "size", type = "int", value = "128" }, } },
        { id = 89, module = "ffi/call", x = 1437, y = 517, params = { { name = "lib", value = "c" }, { name = "fn", value = "int snprintf(char *buf, size_t n, const char *fmt, ..., double, double, double)" }, { name = "pure", value = "false" }, }, inputs = { { name = "buf", type = "string", value = "" }, { name = "n", type = "int", value = "128" }, { name = "fmt", type = "string", value = "basel %.6f, leibniz %.6f, atan %.6f" }, { name = "a4", type = "float", value = "0" }, { name = "a5", type = "float", value = "0" }, { name = "a6", type = "float", value = "0" }, } },
        { id = 90, module = "ffi/read", x = 1631.32031, y = 671.320312, inputs = { { name = "offset", type = "int", value = "0" }, { name = "type", value = "string" }, } },
        { id = 91, module = "io/print", x = 1826.89844, y = 526.898438, inputs = { { name = "value", type = "string", value = "" }, } },
        { id = 92, module = "ffi/free", x = 2043, y = 553 },
}

links = {
        { from = 67, from_pin = 0, to = 69, to_pin = 0 },
        { from = 69, from_pin = 1, to = 70, to_pin = 0 },
        { from = 71, from_pin = 0, to = 72, to_pin = 0 },
        { from = 70, from_pin = 0, to = 72, to_pin = 1 },
        { from = 69, from_pin = 0, to = 73, to_pin = 0 },
        { from = 72, from_pin = 0, to = 73, to_pin = 2 },
        { from = 71, from_pin = 0, to = 74, to_pin = 0 },
        { from = 74, from_pin = 0, to = 75, to_pin = 0 },
        { from = 67, from_pin = 0, to = 77, to_pin = 0 },
        { from = 77, from_pin = 1, to = 78, to_pin = 1 },
        { from = 77, from_pin = 1, to = 79, to_pin = 0 },
        { from = 79, from_pin = 0, to = 80, to_pin = 0 },
        { from = 78, from_pin = 0, to = 81, to_pin = 0 },
        { from = 80, from_pin = 0, to = 81, to_pin = 1 },
        { from = 82, from_pin = 0, to = 83, to_pin = 0 },
        { from = 81, from_pin = 0, to = 83, to_pin = 1 },
        { from = 77, from_pin = 0, to = 84, to_pin = 0 },
        { from = 83, from_pin = 0, to = 84, to_pin = 2 },
        { from = 82, from_pin = 0, to = 85, to_pin = 0 },
        { from = 67, from_pin = 0, to = 86, to_pin = 0 },
        { from = 86, from_pin = 1, to = 87, to_pin = 0 },
        { from = 69, from_pin = 2, to = 88, to_pin = 0 },
        { from = 77, from_pin = 2, to = 88, to_pin = 0 },
        { from = 86, from_pin = 0, to = 88, to_pin = 0 },
        { from = 88, from_pin = 0, to = 89, to_pin = 0 },
        { from = 88, from_pin = 1, to = 89, to_pin = 1 },
        { from = 75, from_pin = 0, to = 89, to_pin = 4 },
        { from = 85, from_pin = 0, to = 89, to_pin = 5 },
        { from = 87, from_pin = 0, to = 89, to_pin = 6 },
        { from = 89, from_pin = 0, to = 91, to_pin = 0 },
        { from = 88, from_pin = 1, to = 90, to_pin = 0 },
        { from = 90, from_pin = 0, to = 91, to_pin = 1 },
        { from = 91, from_pin = 0, to = 92, to_pin = 0 },
        { from = 88, from_pin = 1, to = 92, to_pin = 1 },
}

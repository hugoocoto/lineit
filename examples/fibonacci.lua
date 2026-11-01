-- fibonacci
-- The first 30 Fibonacci numbers, in a list.
-- var/var declares fib, a list: it starts empty when you press Run.
-- flow/entry pushes 0 and 1 (list/push). Then flow/loop 2..29 pushes
-- fib[-1] + fib[-2] (list/get with negative indices counts from the
-- end) and prints the new last one with io/print. io/display shows
-- list/length. The three var/var are copies of the same list.
--
-- Open it with: ./lineit -f examples/fibonacci.lua, then press Run.
lineit_canvas = "0.0.1" -- the lineit that wrote it, see LI_VERSION

camera = { x = 703.050781, y = 140.894531, offset_x = 640, offset_y = 344, zoom = 0.737333834 }

squares = {
        { id = 53, module = "flow/entry", x = -35, y = -35 },
        { id = 54, module = "var/var", x = 116.898438, y = 121.898438, params = { { name = "name", value = "fib" }, { name = "global", value = "false" }, }, inputs = { { name = "type", value = "list" }, { name = "start", type = "int", value = "0" }, } },
        { id = 55, module = "list/push", x = 306.898438, y = -73.1015625, inputs = { { name = "value", type = "int", value = "0" }, } },
        { id = 56, module = "list/push", x = 496.898438, y = -73.1015625, inputs = { { name = "value", type = "int", value = "1" }, } },
        { id = 57, module = "flow/loop", x = 703.109375, y = -75.890625, inputs = { { name = "from", type = "int", value = "2" }, { name = "to", type = "int", value = "30" }, } },
        { id = 58, module = "var/var", x = 515.898438, y = 241.898438, params = { { name = "name", value = "fib" }, { name = "global", value = "false" }, }, inputs = { { name = "type", value = "list" }, { name = "start", type = "int", value = "0" }, } },
        { id = 59, module = "list/get", x = 703.109375, y = 164.109375, inputs = { { name = "index", type = "int", value = "-1" }, } },
        { id = 60, module = "list/get", x = 703.109375, y = 359.109375, inputs = { { name = "index", type = "int", value = "-2" }, } },
        { id = 61, module = "math/add", x = 928.843689, y = 255.84375, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "0" }, } },
        { id = 62, module = "var/var", x = 914.898376, y = -253.101562, params = { { name = "name", value = "fib" }, { name = "global", value = "false" }, }, inputs = { { name = "type", value = "list" }, { name = "start", type = "int", value = "0" }, } },
        { id = 63, module = "list/push", x = 1104.89844, y = -73.1015625, inputs = { { name = "value", type = "int", value = "0" }, } },
        { id = 64, module = "io/print", x = 1294.89844, y = -73.1015625, inputs = { { name = "value", type = "string", value = "" }, } },
        { id = 65, module = "list/length", x = 1179.47656, y = 203.476562 },
        { id = 66, module = "io/display", x = 1371, y = 205 },
}

links = {
        { from = 53, from_pin = 0, to = 55, to_pin = 0 },
        { from = 54, from_pin = 0, to = 55, to_pin = 1 },
        { from = 54, from_pin = 0, to = 56, to_pin = 1 },
        { from = 55, from_pin = 0, to = 56, to_pin = 0 },
        { from = 56, from_pin = 0, to = 57, to_pin = 0 },
        { from = 58, from_pin = 0, to = 59, to_pin = 0 },
        { from = 58, from_pin = 0, to = 60, to_pin = 0 },
        { from = 59, from_pin = 0, to = 61, to_pin = 0 },
        { from = 60, from_pin = 0, to = 61, to_pin = 1 },
        { from = 57, from_pin = 0, to = 63, to_pin = 0 },
        { from = 62, from_pin = 0, to = 63, to_pin = 1 },
        { from = 61, from_pin = 0, to = 63, to_pin = 2 },
        { from = 63, from_pin = 0, to = 64, to_pin = 0 },
        { from = 59, from_pin = 0, to = 64, to_pin = 1 },
        { from = 62, from_pin = 0, to = 65, to_pin = 0 },
        { from = 65, from_pin = 0, to = 66, to_pin = 0 },
}

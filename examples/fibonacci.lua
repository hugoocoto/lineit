-- fibonacci
-- The first 30 Fibonacci numbers, in a list.
-- entry clears the list fib and pushes 0 and 1. Then loop 2..29
-- pushes fib[-1] + fib[-2] (list/get with negative indices count
-- from the end) and prints the new last one. display shows the
-- length of the list.
--
-- Open it with: ./lineit -f examples/fibonacci.lua, then press Start.
lineit_canvas = 1 -- format version, see CANVAS_VERSION

camera = { x = 703.050781, y = 207, offset_x = 640, offset_y = 344, zoom = 0.77115792 }

squares = {
        { id = 49, module = "flow/entry", x = -35, y = -35 },
        { id = 50, module = "list/clear", x = 150.6875, y = -39.3125, params = { { name = "list", value = "fib" }, } },
        { id = 51, module = "list/push", x = 306.898438, y = -73.1015625, params = { { name = "list", value = "fib" }, }, inputs = { { name = "value", type = "int", value = "0" }, } },
        { id = 52, module = "list/push", x = 496.898438, y = -73.1015625, params = { { name = "list", value = "fib" }, }, inputs = { { name = "value", type = "int", value = "1" }, } },
        { id = 53, module = "flow/loop", x = 703.109375, y = -75.890625, inputs = { { name = "from", type = "int", value = "2" }, { name = "to", type = "int", value = "30" }, } },
        { id = 54, module = "list/get", x = 703.109375, y = 164.109375, params = { { name = "list", value = "fib" }, }, inputs = { { name = "index", type = "int", value = "-1" }, } },
        { id = 55, module = "list/get", x = 703.109375, y = 314.109375, params = { { name = "list", value = "fib" }, }, inputs = { { name = "index", type = "int", value = "-2" }, } },
        { id = 56, module = "math/add", x = 928.843689, y = 255.84375, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "0" }, } },
        { id = 57, module = "list/push", x = 1104.89844, y = -73.1015625, params = { { name = "list", value = "fib" }, }, inputs = { { name = "value", type = "int", value = "0" }, } },
        { id = 58, module = "io/print", x = 1294.89844, y = -73.1015625, inputs = { { name = "value", type = "string", value = "" }, } },
        { id = 59, module = "list/len", x = 1181, y = 205, params = { { name = "list", value = "fib" }, } },
        { id = 60, module = "io/display", x = 1371, y = 205 },
}

links = {
        { from = 49, from_pin = 0, to = 50, to_pin = 0 },
        { from = 50, from_pin = 0, to = 51, to_pin = 0 },
        { from = 51, from_pin = 0, to = 52, to_pin = 0 },
        { from = 52, from_pin = 0, to = 53, to_pin = 0 },
        { from = 54, from_pin = 0, to = 56, to_pin = 0 },
        { from = 55, from_pin = 0, to = 56, to_pin = 1 },
        { from = 53, from_pin = 0, to = 57, to_pin = 0 },
        { from = 56, from_pin = 0, to = 57, to_pin = 1 },
        { from = 57, from_pin = 0, to = 58, to_pin = 0 },
        { from = 54, from_pin = 0, to = 58, to_pin = 1 },
        { from = 59, from_pin = 0, to = 60, to_pin = 0 },
}

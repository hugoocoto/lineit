-- sum_squares
-- Sum of the squares of 0..9 (285), with a variable.
-- var/var declares sum, an int that starts as 0 when you press Run.
-- flow/loop runs index from 0 to 9 and does sum = sum + index * index
-- (var/get, math/multiply, math/add, var/set): get and set read and
-- change the variable wired to them. The two var/var are the same
-- variable, sum: copies, so the wires stay short. When the loop is
-- done, io/print shows sum.
--
-- Open it with: ./lineit -f examples/sum_squares.lua, then press Run.
lineit_canvas = "0.0.1" -- the lineit that wrote it, see LI_VERSION

camera = { x = 522.550781, y = 109.5, offset_x = 640, offset_y = 344, zoom = 0.794830501 }

squares = {
        { id = 22, module = "flow/entry", x = -35, y = -35 },
        { id = 23, module = "flow/loop", x = 133.109375, y = -75.890625, inputs = { { name = "from", type = "int", value = "0" }, { name = "to", type = "int", value = "10" }, } },
        { id = 24, module = "math/multiply", x = 347.6875, y = 124.6875, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "0" }, } },
        { id = 25, module = "var/var", x = 344.898438, y = 301.898438, params = { { name = "name", value = "sum" }, { name = "global", value = "false" }, }, inputs = { { name = "type", value = "int" }, { name = "start", type = "int", value = "0" }, } },
        { id = 26, module = "var/get", x = 537.6875, y = 304.6875, inputs = { { name = "var", type = "ref", value = "" }, } },
        { id = 27, module = "math/add", x = 567.84375, y = 165.84375, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "0" }, } },
        { id = 28, module = "var/var", x = 553.898438, y = -253.101562, params = { { name = "name", value = "sum" }, { name = "global", value = "false" }, }, inputs = { { name = "type", value = "int" }, { name = "start", type = "int", value = "0" }, } },
        { id = 29, module = "var/set", x = 741.109436, y = -75.890625, inputs = { { name = "var", type = "ref", value = "" }, { name = "value", type = "int", value = "0" }, } },
        { id = 30, module = "io/print", x = 933.898499, y = 151.898438, inputs = { { name = "value", type = "string", value = "" }, } },
}

links = {
        { from = 22, from_pin = 0, to = 23, to_pin = 0 },
        { from = 23, from_pin = 1, to = 24, to_pin = 0 },
        { from = 23, from_pin = 1, to = 24, to_pin = 1 },
        { from = 25, from_pin = 0, to = 26, to_pin = 0 },
        { from = 26, from_pin = 0, to = 27, to_pin = 0 },
        { from = 24, from_pin = 0, to = 27, to_pin = 1 },
        { from = 23, from_pin = 0, to = 29, to_pin = 0 },
        { from = 28, from_pin = 0, to = 29, to_pin = 1 },
        { from = 27, from_pin = 0, to = 29, to_pin = 2 },
        { from = 23, from_pin = 2, to = 30, to_pin = 0 },
        { from = 26, from_pin = 0, to = 30, to_pin = 1 },
}

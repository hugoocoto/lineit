-- sum_squares
-- Sum of the squares of 0..9 (285), with a variable.
-- entry sets sum = 0, then loop runs index from 0 to 9 and does
-- sum = sum + index * index (var/get, math/multiply, math/add, var/set).
-- When the loop is done, print shows sum.
--
-- Open it with: ./lineit -f examples/sum_squares.lua, then press Start.
lineit_canvas = 1 -- format version, see CANVAS_VERSION

camera = { x = 523.945312, y = 205.605469, offset_x = 640, offset_y = 344, zoom = 0.995346487 }

squares = {
        { id = 22, module = "flow/entry", x = -35, y = -35 },
        { id = 23, module = "var/set", x = 114.109375, y = -75.890625, params = { { name = "name", value = "sum" }, { name = "global", value = "false" }, }, inputs = { { name = "value", type = "int", value = "0" }, } },
        { id = 24, module = "flow/loop", x = 323.109344, y = -75.890625, inputs = { { name = "from", type = "int", value = "0" }, { name = "to", type = "int", value = "10" }, } },
        { id = 25, module = "math/multiply", x = 537.6875, y = 124.6875, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "0" }, } },
        { id = 26, module = "var/get", x = 573, y = 325, params = { { name = "name", value = "sum" }, { name = "global", value = "false" }, } },
        { id = 27, module = "math/add", x = 757.843811, y = 195.84375, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "0" }, } },
        { id = 28, module = "var/set", x = 931.109436, y = -75.890625, params = { { name = "name", value = "sum" }, { name = "global", value = "false" }, }, inputs = { { name = "value", type = "int", value = "0" }, } },
        { id = 29, module = "io/print", x = 933.898499, y = 316.898438, inputs = { { name = "value", type = "string", value = "" }, } },
}

links = {
        { from = 22, from_pin = 0, to = 23, to_pin = 0 },
        { from = 23, from_pin = 0, to = 24, to_pin = 0 },
        { from = 24, from_pin = 1, to = 25, to_pin = 0 },
        { from = 24, from_pin = 1, to = 25, to_pin = 1 },
        { from = 26, from_pin = 0, to = 27, to_pin = 0 },
        { from = 25, from_pin = 0, to = 27, to_pin = 1 },
        { from = 24, from_pin = 0, to = 28, to_pin = 0 },
        { from = 27, from_pin = 0, to = 28, to_pin = 1 },
        { from = 24, from_pin = 2, to = 29, to_pin = 0 },
        { from = 26, from_pin = 0, to = 29, to_pin = 1 },
}

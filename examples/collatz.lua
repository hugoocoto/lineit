-- collatz
-- Collatz sequence of 27: while n != 1, n = n / 2 if it's even, and
-- n = 3n + 1 if it's odd. Prints every n, and the number of steps
-- (111) at the end.
-- Shows while with a comparison as its cond, ifelse, and variables.
-- Type another number than 27 in the first var/set to try it.
--
-- Open it with: ./lineit -f examples/collatz.lua, then press Start.
camera = { x = 855.050781, y = 108.105469, offset_x = 640, offset_y = 344, zoom = 0.645126045 }

squares = {
        { id = 30, module = "entry", x = -35, y = -35 },
        { id = 31, module = "var/set", x = 114.109375, y = -75.890625, params = { { name = "name", value = "n" }, }, inputs = { { name = "value", type = "int", value = "27" }, } },
        { id = 32, module = "var/set", x = 304.109375, y = -75.890625, params = { { name = "name", value = "steps" }, }, inputs = { { name = "value", type = "int", value = "0" }, } },
        { id = 33, module = "while", x = 530, y = -59, params = { { name = "max", value = "10000" }, }, inputs = { { name = "cond", type = "bool", value = "false" }, } },
        { id = 34, module = "compare/compare", x = 416, y = 151, inputs = { { name = "op", value = "!=" }, { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "1" }, } },
        { id = 35, module = "var/get", x = 211.999985, y = 325, params = { { name = "name", value = "n" }, } },
        { id = 36, module = "ifelse", x = 799.898438, y = -55.1015625, inputs = { { name = "cond", type = "bool", value = "false" }, } },
        { id = 37, module = "math/mod", x = 551.109375, y = 299.109375, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "2" }, } },
        { id = 38, module = "compare/equal", x = 760.632874, y = 168.632812, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "0" }, } },
        { id = 39, module = "math/div", x = 914.898376, y = -298.101562, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "2" }, } },
        { id = 40, module = "var/set", x = 1102.10938, y = -195.890625, params = { { name = "name", value = "n" }, }, inputs = { { name = "value", type = "int", value = "0" }, } },
        { id = 41, module = "math/mul", x = 803.6875, y = 364.6875, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "3" }, } },
        { id = 42, module = "math/add", x = 985.84375, y = 270.84375, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "1" }, } },
        { id = 43, module = "var/set", x = 1102.10938, y = 59.109375, params = { { name = "name", value = "n" }, }, inputs = { { name = "value", type = "int", value = "0" }, } },
        { id = 44, module = "var/get", x = 1219, y = 340, params = { { name = "name", value = "steps" }, } },
        { id = 45, module = "math/add", x = 1327.84375, y = 165.84375, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "1" }, } },
        { id = 46, module = "var/set", x = 1368.10938, y = -75.890625, params = { { name = "name", value = "steps" }, }, inputs = { { name = "value", type = "int", value = "0" }, } },
        { id = 47, module = "print", x = 1598.89844, y = -73.1015625, inputs = { { name = "value", type = "string", value = "" }, } },
        { id = 48, module = "print", x = 743.898499, y = -313.101562, inputs = { { name = "value", type = "string", value = "" }, } },
}

links = {
        { from = 30, from_pin = 0, to = 31, to_pin = 0 },
        { from = 31, from_pin = 0, to = 32, to_pin = 0 },
        { from = 32, from_pin = 0, to = 33, to_pin = 0 },
        { from = 35, from_pin = 0, to = 34, to_pin = 1 },
        { from = 34, from_pin = 0, to = 33, to_pin = 1 },
        { from = 33, from_pin = 0, to = 36, to_pin = 0 },
        { from = 35, from_pin = 0, to = 37, to_pin = 0 },
        { from = 37, from_pin = 0, to = 38, to_pin = 0 },
        { from = 38, from_pin = 0, to = 36, to_pin = 1 },
        { from = 35, from_pin = 0, to = 39, to_pin = 0 },
        { from = 35, from_pin = 0, to = 41, to_pin = 0 },
        { from = 41, from_pin = 0, to = 42, to_pin = 0 },
        { from = 36, from_pin = 0, to = 40, to_pin = 0 },
        { from = 39, from_pin = 0, to = 40, to_pin = 1 },
        { from = 36, from_pin = 1, to = 43, to_pin = 0 },
        { from = 42, from_pin = 0, to = 43, to_pin = 1 },
        { from = 40, from_pin = 0, to = 46, to_pin = 0 },
        { from = 43, from_pin = 0, to = 46, to_pin = 0 },
        { from = 44, from_pin = 0, to = 45, to_pin = 0 },
        { from = 45, from_pin = 0, to = 46, to_pin = 1 },
        { from = 46, from_pin = 0, to = 47, to_pin = 0 },
        { from = 35, from_pin = 0, to = 47, to_pin = 1 },
        { from = 33, from_pin = 1, to = 48, to_pin = 0 },
        { from = 44, from_pin = 0, to = 48, to_pin = 1 },
}

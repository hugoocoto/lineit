-- collatz
-- Collatz sequence of 27: while n != 1, n = n / 2 if it's even, and
-- n = 3n + 1 if it's odd. Prints every n, and the number of steps
-- (111) at the end.
-- Shows flow/while with a comparison (compare/compare) as its cond,
-- flow/ifelse, and variables: var/var declares n (an int that starts
-- as 27) and steps (0), and var/get and var/set read and change them.
-- The var/var with the same name are copies of one variable. Type
-- another start than 27 in all of n's to try it.
--
-- Open it with: ./lineit -f examples/collatz.lua, then press Run.
lineit_canvas = "0.0.2" -- the lineit that wrote it, see LI_VERSION

camera = { x = 855, y = 105.75, offset_x = 640, offset_y = 344, zoom = 0.645090818 }

squares = {
        { id = 31, module = "flow/entry", x = -32, y = -35 },
        { id = 32, module = "flow/while", x = 325.898407, y = -71, params = { { name = "max", value = "10000" }, }, inputs = { { name = "cond", type = "bool", value = "false" }, } },
        { id = 33, module = "compare/compare", x = 231.421875, y = 151, inputs = { { name = "op", value = "!=" }, { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "1" }, } },
        { id = 34, module = "var/var", x = -35.1015625, y = 289, params = { { name = "name", value = "n" }, { name = "global", value = "false" }, }, inputs = { { name = "type", value = "int" }, { name = "start", type = "int", value = "27" }, } },
        { id = 35, module = "var/get", x = 176.687485, y = 325, inputs = { { name = "var", type = "ref", value = "" }, } },
        { id = 36, module = "flow/ifelse", x = 609.898438, y = -47, inputs = { { name = "cond", type = "bool", value = "false" }, } },
        { id = 37, module = "math/modulo", x = 380.109406, y = 343, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "2" }, } },
        { id = 38, module = "compare/equal", x = 570.632812, y = 178, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "0" }, } },
        { id = 39, module = "math/divide", x = 743.898499, y = -324.5, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "2" }, } },
        { id = 40, module = "var/var", x = 743.898499, y = -161, params = { { name = "name", value = "n" }, { name = "global", value = "false" }, }, inputs = { { name = "type", value = "int" }, { name = "start", type = "int", value = "27" }, } },
        { id = 41, module = "var/set", x = 950.109375, y = -179, inputs = { { name = "var", type = "ref", value = "" }, { name = "value", type = "int", value = "0" }, } },
        { id = 42, module = "math/multiply", x = 632.6875, y = 418, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "3" }, } },
        { id = 43, module = "math/add", x = 814.84375, y = 313, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "1" }, } },
        { id = 44, module = "var/var", x = 762.898438, y = 79, params = { { name = "name", value = "n" }, { name = "global", value = "false" }, }, inputs = { { name = "type", value = "int" }, { name = "start", type = "int", value = "27" }, } },
        { id = 45, module = "var/set", x = 950.109375, y = 91, inputs = { { name = "var", type = "ref", value = "" }, { name = "value", type = "int", value = "0" }, } },
        { id = 46, module = "var/var", x = 971.898438, y = 319, params = { { name = "name", value = "steps" }, { name = "global", value = "false" }, }, inputs = { { name = "type", value = "int" }, { name = "start", type = "int", value = "0" }, } },
        { id = 47, module = "var/get", x = 1164.6875, y = 355, inputs = { { name = "var", type = "ref", value = "" }, } },
        { id = 48, module = "math/add", x = 1327.84375, y = 193, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "1" }, } },
        { id = 49, module = "var/var", x = 1180.89844, y = -206, params = { { name = "name", value = "steps" }, { name = "global", value = "false" }, }, inputs = { { name = "type", value = "int" }, { name = "start", type = "int", value = "0" }, } },
        { id = 50, module = "var/set", x = 1368.10938, y = -59, inputs = { { name = "var", type = "ref", value = "" }, { name = "value", type = "int", value = "0" }, } },
        { id = 51, module = "io/print", x = 1598.89844, y = -47, inputs = { { name = "value", type = "string", value = "" }, } },
        { id = 52, module = "io/print", x = 534.898438, y = -287, inputs = { { name = "value", type = "string", value = "" }, } },
}

links = {
        { from = 31, from_pin = 0, to = 32, to_pin = 0 },
        { from = 34, from_pin = 0, to = 35, to_pin = 0 },
        { from = 35, from_pin = 0, to = 33, to_pin = 1 },
        { from = 33, from_pin = 0, to = 32, to_pin = 1 },
        { from = 32, from_pin = 0, to = 36, to_pin = 0 },
        { from = 35, from_pin = 0, to = 37, to_pin = 0 },
        { from = 37, from_pin = 0, to = 38, to_pin = 0 },
        { from = 38, from_pin = 0, to = 36, to_pin = 1 },
        { from = 35, from_pin = 0, to = 39, to_pin = 0 },
        { from = 35, from_pin = 0, to = 42, to_pin = 0 },
        { from = 42, from_pin = 0, to = 43, to_pin = 0 },
        { from = 36, from_pin = 0, to = 41, to_pin = 0 },
        { from = 40, from_pin = 0, to = 41, to_pin = 1 },
        { from = 39, from_pin = 0, to = 41, to_pin = 2 },
        { from = 36, from_pin = 1, to = 45, to_pin = 0 },
        { from = 44, from_pin = 0, to = 45, to_pin = 1 },
        { from = 43, from_pin = 0, to = 45, to_pin = 2 },
        { from = 41, from_pin = 0, to = 50, to_pin = 0 },
        { from = 45, from_pin = 0, to = 50, to_pin = 0 },
        { from = 46, from_pin = 0, to = 47, to_pin = 0 },
        { from = 47, from_pin = 0, to = 48, to_pin = 0 },
        { from = 49, from_pin = 0, to = 50, to_pin = 1 },
        { from = 48, from_pin = 0, to = 50, to_pin = 2 },
        { from = 50, from_pin = 0, to = 51, to_pin = 0 },
        { from = 35, from_pin = 0, to = 51, to_pin = 1 },
        { from = 32, from_pin = 1, to = 52, to_pin = 0 },
        { from = 47, from_pin = 0, to = 52, to_pin = 1 },
}

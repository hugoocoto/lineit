-- fizzbuzz
-- FizzBuzz from 1 to 100: multiples of 15 print FizzBuzz, of 3 Fizz,
-- of 5 Buzz, and the rest the number. The output goes to stdout.
-- loop runs index from 1 to 100 (typed in its from and to fields).
-- For each one, three ifelse in a row check index % 15, % 3 and % 5
-- == 0 (math/modulo + compare/equal), and the first true one prints its
-- word. If none is, the last print prints the index.
--
-- Open it with: ./lineit -f examples/fizzbuzz.lua, then press Start.
lineit_canvas = 1 -- format version, see CANVAS_VERSION

camera = { x = 674.550781, y = 116.132812, offset_x = 640, offset_y = 344, zoom = 0.800479472 }

squares = {
        { id = 2, module = "flow/entry", x = -35, y = -35 },
        { id = 3, module = "flow/loop", x = 114.109375, y = -75.890625, inputs = { { name = "from", type = "int", value = "1" }, { name = "to", type = "int", value = "101" }, } },
        { id = 4, module = "math/modulo", x = 285.109375, y = 164.109375, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "15" }, } },
        { id = 5, module = "compare/equal", x = 418.632812, y = 333.632812, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "0" }, } },
        { id = 6, module = "flow/ifelse", x = 381.898438, y = -55.1015625, inputs = { { name = "cond", type = "bool", value = "false" }, } },
        { id = 7, module = "io/print", x = 477.898438, y = -238.101562, inputs = { { name = "value", type = "string", value = "FizzBuzz" }, } },
        { id = 8, module = "math/modulo", x = 570.109375, y = 164.109375, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "3" }, } },
        { id = 9, module = "compare/equal", x = 703.632812, y = 333.632812, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "0" }, } },
        { id = 10, module = "flow/ifelse", x = 666.898438, y = -55.1015625, inputs = { { name = "cond", type = "bool", value = "false" }, } },
        { id = 11, module = "io/print", x = 762.898438, y = -238.101562, inputs = { { name = "value", type = "string", value = "Fizz" }, } },
        { id = 12, module = "math/modulo", x = 855.109375, y = 164.109375, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "5" }, } },
        { id = 13, module = "compare/equal", x = 988.632812, y = 333.632812, inputs = { { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "0" }, } },
        { id = 14, module = "flow/ifelse", x = 951.898499, y = -55.1015625, inputs = { { name = "cond", type = "bool", value = "false" }, } },
        { id = 15, module = "io/print", x = 1047.89844, y = -238.101562, inputs = { { name = "value", type = "string", value = "Buzz" }, } },
        { id = 16, module = "io/print", x = 1237.89844, y = 16.8984375, inputs = { { name = "value", type = "string", value = "" }, } },
}

links = {
        { from = 2, from_pin = 0, to = 3, to_pin = 0 },
        { from = 3, from_pin = 1, to = 4, to_pin = 0 },
        { from = 4, from_pin = 0, to = 5, to_pin = 0 },
        { from = 5, from_pin = 0, to = 6, to_pin = 1 },
        { from = 3, from_pin = 0, to = 6, to_pin = 0 },
        { from = 6, from_pin = 0, to = 7, to_pin = 0 },
        { from = 3, from_pin = 1, to = 8, to_pin = 0 },
        { from = 8, from_pin = 0, to = 9, to_pin = 0 },
        { from = 9, from_pin = 0, to = 10, to_pin = 1 },
        { from = 6, from_pin = 1, to = 10, to_pin = 0 },
        { from = 10, from_pin = 0, to = 11, to_pin = 0 },
        { from = 3, from_pin = 1, to = 12, to_pin = 0 },
        { from = 12, from_pin = 0, to = 13, to_pin = 0 },
        { from = 13, from_pin = 0, to = 14, to_pin = 1 },
        { from = 10, from_pin = 1, to = 14, to_pin = 0 },
        { from = 14, from_pin = 0, to = 15, to_pin = 0 },
        { from = 14, from_pin = 1, to = 16, to_pin = 0 },
        { from = 3, from_pin = 1, to = 16, to_pin = 1 },
}

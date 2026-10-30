-- clock
-- Counts seconds from 0 to 9 forever.
-- flow/entry resets flow/clock on Run so it always starts at 0. On
-- each tick, flow/ifelse checks count >= 10 (compare/compare, with >=
-- chosen in its op list) and, if so, resets the clock again.
-- io/display shows the count. Type another number in the clock's
-- seconds field to change the speed.
--
-- Open it with: ./lineit -f examples/clock.lua, then press Run.
lineit_canvas = "0.0.1" -- the lineit that wrote it, see LI_VERSION

camera = { x = 314.050781, y = 69, offset_x = 640, offset_y = 344, zoom = 1 }

squares = {
        { id = 17, module = "flow/entry", x = -35, y = -35 },
        { id = 18, module = "flow/clock", x = 108.53125, y = -81.46875, inputs = { { name = "seconds", type = "float", value = "1" }, }, state = "0" },
        { id = 19, module = "io/display", x = 402, y = -140 },
        { id = 20, module = "compare/compare", x = 339.999969, y = 136, inputs = { { name = "op", value = ">=" }, { name = "a", type = "int", value = "0" }, { name = "b", type = "int", value = "10" }, } },
        { id = 21, module = "flow/ifelse", x = 552.898438, y = 79.8984375, inputs = { { name = "cond", type = "bool", value = "false" }, } },
}

links = {
        { from = 17, from_pin = 0, to = 18, to_pin = 0 },
        { from = 18, from_pin = 0, to = 21, to_pin = 0 },
        { from = 18, from_pin = 1, to = 20, to_pin = 1 },
        { from = 20, from_pin = 0, to = 21, to_pin = 1 },
        { from = 21, from_pin = 0, to = 18, to_pin = 0 },
        { from = 18, from_pin = 1, to = 19, to_pin = 0 },
}

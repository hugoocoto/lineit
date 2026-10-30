-- hello
-- Prints "Hello, world!" when you press Run.
-- flow/entry ticks once on Run. io/print writes its value to stdout
-- and shows it: the text is typed in its value field (click it to
-- change it).
--
-- Open it with: ./lineit -f examples/hello.lua, then press Run.
lineit_canvas = 1 -- format version, see CANVAS_VERSION

camera = { x = 114.050781, y = 12, offset_x = 640, offset_y = 344, zoom = 1 }

squares = {
        { id = 0, module = "flow/entry", x = -35, y = -35 },
        { id = 1, module = "io/print", x = 116.898438, y = -73.1015625, inputs = { { name = "value", type = "string", value = "Hello, world!" }, } },
}

links = {
        { from = 0, from_pin = 0, to = 1, to_pin = 0 },
}

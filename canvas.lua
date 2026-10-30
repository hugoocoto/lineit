-- lineit canvas
lineit_canvas = 1 -- format version, see CANVAS_VERSION

camera = { x = 365.87677, y = 80.7041626, offset_x = 719, offset_y = 556, zoom = 1.22140265 }

squares = {
        { id = 0, module = "flow/entry", x = -35, y = -35 },
        { id = 1, module = "flow/counter", x = 131.952988, y = -28.4411774, state = "128" },
        { id = 2, module = "io/print", x = 113.0392, y = 167.431641, inputs = { { name = "value", type = "string", value = "" }, } },
}

links = {
        { from = 0, from_pin = 0, to = 1, to_pin = 0 },
        { from = 1, from_pin = 0, to = 2, to_pin = 0 },
        { from = 2, from_pin = 0, to = 1, to_pin = 0 },
        { from = 1, from_pin = 1, to = 2, to_pin = 1 },
}

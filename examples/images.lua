-- images
-- Edits a photo, symi.png next to it (houses on Symi, by Jebulon, CC0:
-- see examples/README.md): made 480 pixels wide, then two ways: gray with
-- more contrast, and the houses at its middle, tinted warm and mirrored.
-- Press Run and both are saved next to it too, as symi-gray.png and
-- symi-houses.png: paths are relative to the canvas's folder.
-- image/ squares are pure but save: they compute when read, and again
-- only when an input changes. Each shows its image and its size, before
-- Run too. Drop an image file on image/open (or Choose file... in its
-- menu) to edit another one.
--
-- Open it with: ./lineit -f examples/images.lua, then press Run.
lineit_canvas = "0.0.2" -- the lineit that wrote it, see LI_VERSION

camera = { x = 648.050781, y = 311.75, offset_x = 640, offset_y = 344, zoom = 0.689283788 }

squares = {
        { id = 93, module = "image/open", x = -88, y = 150, inputs = { { name = "path", type = "string", value = "symi.png" }, } },
        { id = 94, module = "image/resize", x = 130.5, y = 150, inputs = { { name = "width", type = "int", value = "480" }, { name = "height", type = "int", value = "0" }, { name = "how", value = "smooth" }, } },
        { id = 95, module = "image/grayscale", x = 349, y = 0 },
        { id = 96, module = "image/contrast", x = 567.5, y = 0, inputs = { { name = "amount", type = "float", value = "25" }, } },
        { id = 97, module = "image/crop", x = 349, y = 300, inputs = { { name = "x", type = "int", value = "120" }, { name = "y", type = "int", value = "110" }, { name = "width", type = "int", value = "240" }, { name = "height", type = "int", value = "160" }, } },
        { id = 98, module = "image/tint", x = 567.5, y = 300, inputs = { { name = "color", type = "string", value = "#ffc080" }, } },
        { id = 99, module = "image/flip", x = 786, y = 300, inputs = { { name = "way", value = "horizontal" }, } },
        { id = 100, module = "flow/entry", x = 842, y = -112.5 },
        { id = 101, module = "image/save", x = 800.898438, y = 0, inputs = { { name = "path", type = "string", value = "symi-gray.png" }, } },
        { id = 102, module = "image/save", x = 1019.39844, y = 300, inputs = { { name = "path", type = "string", value = "symi-houses.png" }, } },
        { id = 103, module = "image/size", x = 389.320312, y = 600 },
        { id = 104, module = "string/format", x = 1011.03125, y = 570, inputs = { { name = "template", type = "string", value = "{}x{} in, saved: {}" }, { name = "a", type = "string", value = "" }, { name = "b", type = "string", value = "" }, { name = "c", type = "string", value = "" }, } },
        { id = 105, module = "io/print", x = 1237.89844, y = 570, inputs = { { name = "value", type = "string", value = "" }, } },
}

links = {
        { from = 93, from_pin = 0, to = 94, to_pin = 0 },
        { from = 94, from_pin = 0, to = 95, to_pin = 0 },
        { from = 95, from_pin = 0, to = 96, to_pin = 0 },
        { from = 94, from_pin = 0, to = 97, to_pin = 0 },
        { from = 97, from_pin = 0, to = 98, to_pin = 0 },
        { from = 98, from_pin = 0, to = 99, to_pin = 0 },
        { from = 100, from_pin = 0, to = 101, to_pin = 0 },
        { from = 96, from_pin = 0, to = 101, to_pin = 1 },
        { from = 101, from_pin = 0, to = 102, to_pin = 0 },
        { from = 99, from_pin = 0, to = 102, to_pin = 1 },
        { from = 94, from_pin = 0, to = 103, to_pin = 0 },
        { from = 103, from_pin = 0, to = 104, to_pin = 1 },
        { from = 103, from_pin = 1, to = 104, to_pin = 2 },
        { from = 102, from_pin = 1, to = 104, to_pin = 3 },
        { from = 102, from_pin = 0, to = 105, to_pin = 0 },
        { from = 104, from_pin = 0, to = 105, to_pin = 1 },
}

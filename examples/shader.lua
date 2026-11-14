-- lineit canvas
return {
        lineit = 1,
        doc = "A Godot shader applied to a photo (symi.png, next to this canvas): a Bayer dither with a palette. The shader is a file, bayer_dither_palette.gdshader (from Milk-Filter-Online by figurophobia, MIT license), set in the file param of image/shader; its uniforms (brightness, contrast, dither_strength, target_size, palette_index...) are the inputs of the square. Edit the file with any editor and the square follows. Run saves the result as symi-dithered.png.",
        camera = { x = 763.4232, y = 159.66324, zoom = 1.0537107 },
        squares = {
                { id = 1, module = "image/open", x = 0, y = 0, fields = { path = "symi.png" } },
                { id = 2, module = "image/resize", x = 280, y = 0, fields = { height = "128", width = "192" } },
                { id = 3, module = "image/shader", x = 560, y = 0, params = { file = "bayer_dither_palette.gdshader" }, fields = { highlighter = "false", palette_colors = "4", palette_index = "2", target_size = "64", texture_aspect_ratio = "1.5" } },
                { id = 4, module = "flow/entry", x = 672, y = -80 },
                { id = 5, module = "image/save", x = 840, y = 0, fields = { path = "symi-dithered.png" } },
                { id = 6, module = "string/format", x = 1056, y = -0, params = { format = "saved: {}" } },
                { id = 7, module = "io/print", x = 1264, y = -0 },
        },
        links = {
                { 1, "image", 2, "image" },
                { 2, "image", 3, "TEXTURE" },
                { 4, "then", 5, "tick" },
                { 3, "image", 5, "image" },
                { 5, "ok", 6, "1" },
                { 6, "text", 7, "value" },
                { 5, "then", 7, "tick" },
        },
}

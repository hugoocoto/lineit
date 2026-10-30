-- lineit configuration
-- Colors are { r, g, b } or { r, g, b, a }, each in 0..255.
-- Any key can be removed; the built-in default is used instead.

window = {
        width = 1280,
        height = 720,
        fps = 60,
        title = "lineit",
}

canvas = {
        grid_step = 32,   -- world units between grid lines
        grid_major = 8,   -- every N lines is drawn as a major line
        zoom_min = 0.1,
        zoom_max = 8.0,
        zoom_speed = 0.2, -- zoom change per wheel step (logarithmic)
        square_size = 64,       -- minimum side, squares grow with their pins
        pin_spacing = 24,       -- distance between pins on the same side
        pin_font_size = 14,
        name_font_size = 16,    -- module name below the square
        pin_radius = 7,     -- input/output circles
        wire_thickness = 3,
        wire_curve_distance = 200, -- shorter wires curve less (straight when very close)
        field_width = 56,          -- fields where input values are typed
        flash_time = 0.5,          -- seconds wires and squares glow after a tick (0: off)
}

bar = {
        height = 32,
        font_size = 20,
}

font = {
        -- fontconfig name (see `fc-list`), e.g. "Iosevka NFM:bold", or a .ttf/.otf path.
        -- Only .ttf/.otf work: plain "Iosevka" is installed as .ttc, use the Nerd Font one.
        name = "Iosevka NFM",
        spacing = 0,
}

modules = {
        path = "build/modules", -- every .so here (and in subfolders) is loaded, `make` builds them from modules/
        source = "modules",     -- Make subgraph (Ctrl+G) writes the new .lua here, and links it into path
}

save = {
        path = "canvas.lua", -- Save/Load (Ctrl+S/Ctrl+O) use this file, `lineit -f file` overrides it
}

output = {
        height = 200,           -- of the panel, toggled with Output in the bar
        font_size = 16,
        lines = 2000,           -- kept, older ones are dropped
        clear_on_start = false, -- empty it when Start is pressed
}

menu = {
        font_size = 20,
        padding = 8,
}

colors = {
        bg = { 24, 24, 27 },
        grid = { 39, 39, 42 },
        grid_major = { 63, 63, 70 },
        bar = { 15, 15, 17 },
        bar_border = { 63, 63, 70 },
        bar_text = { 161, 161, 170 },
        square = { 102, 191, 255 },
        square_border = { 15, 15, 17 },
        square_text = { 15, 15, 17 },
        name = { 161, 161, 170 },
        pin_hover = { 250, 204, 21 },
        selection = { 250, 204, 21 }, -- border of selected squares and the selection box
        flash = { 253, 224, 71 },     -- glow of wires and squares when a tick goes through
        output = { 20, 20, 23 },      -- output panel
        output_text = { 228, 228, 231 },
        error = { 239, 68, 68 },      -- wires between incompatible types
        error_text = { 255, 255, 255 },
        field = { 244, 244, 245 },    -- text field of editable squares (constants)
        menu = { 39, 39, 42 },
        menu_border = { 63, 63, 70 },
        menu_hover = { 63, 63, 70 },
        menu_text = { 228, 228, 231 },
        -- pins and wires, by type
        types = {
                none = { 113, 113, 122 },
                tick = { 228, 228, 231 },
                any = { 161, 161, 170 },
                bool = { 248, 113, 113 },
                int = { 74, 222, 128 },
                float = { 190, 242, 100 },
                string = { 244, 114, 182 },
                ptr = { 192, 132, 252 },
        },
}

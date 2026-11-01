-- lineit configuration
-- lineit reads the one given with -c, or this one next to it in its source
-- tree, or, installed, ~/.config/lineit/config.lua: copy this file there.
-- Colors are { r, g, b } or { r, g, b, a }, each in 0..255.
-- Any key can be removed; the built-in default is used instead.
-- File > Settings (Ctrl+,) changes them too: it writes what it changes to
-- settings.lua, next to this file, read after it, so what's there wins.

window = {
        width = 1280,
        height = 720,
        fps = 60,
        title = "lineit",
}

canvas = {
        grid_step = 32, -- world units between grid lines
        grid_major = 8, -- every N lines is drawn as a major line
        zoom_min = 0.1,
        zoom_max = 8.0,
        zoom_speed = 0.2,          -- zoom change per wheel step (logarithmic)
        square_size = 64,          -- minimum side, squares grow with their pins
        pin_spacing = 24,          -- distance between pins on the same side
        pin_font_size = 14,
        name_font_size = 16,       -- module name below the square
        pin_radius = 7,            -- input/output circles
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
        -- Only .ttf/.otf work.
        name = "Iosevka NFM",
        spacing = 0,
}

-- The paths below are found from where lineit is, so they're commented out:
-- run from its source tree, in it (build/modules, modules...), installed
-- (make install, the AppImage), in PREFIX/lib/lineit and PREFIX/share/lineit
-- for what comes with it, and in ~/.local/share/lineit and ~/.cache/lineit
-- for your own. Set them to use others.
modules = {
        -- path = "build/modules",  -- every .so, subgraph (.lua), type (.type.lua) and package (.li) here is loaded
        -- user = "modules",        -- loaded too, your own: Make subgraph (Ctrl+G) and New type write here
        -- source = "modules",      -- the C of the modules in path, for Compile (yours: next to their .so in user)
        -- cache = "build/cache",   -- packages (.li) not built for this machine are built here, from their src/
}

save = {
        path = "canvas.lua",    -- Save/Load (Ctrl+S/Ctrl+O) use this file, `lineit -f file` overrides it
        autosave = false,       -- save tabs with changes on their own (File > Autosave toggles it)
        autosave_interval = 30, -- seconds between autosaves
}

output = {
        height = 200,         -- of the panel, toggled with Output in the bar
        font_size = 16,
        lines = 2000,         -- kept, older ones are dropped
        clear_on_run = false, -- empty it when Run is pressed
}

compile = {
        -- output = "build/bin", -- Compile (in the bar) makes the program NAME here (NAME: the canvas file's), its C in NAME.build/
        -- runtime = "build/rt", -- what programs are built with, `make` builds it
        cc = "cc",
        cflags = "-O2",
        objcopy = "objcopy", -- gives each square of a package its own copy of it (llvm-objcopy works too)
}

menu = {
        font_size = 20,
        padding = 8,
}

-- The colors are a theme's, and the ones set in colors below. Built in:
-- "dark", "light", "nord" and "gruvbox". Your own go in themes: their base
-- (dark if none) with the colors they set, the keys of colors below.
theme = "dark"

-- themes = {
--         { name = "paper", base = "light", colors = { bg = { 255, 250, 240 }, square = { 250, 190, 110 } } },
-- }

-- Set here, they win over the theme's. All of them, with dark's colors:
colors = {
        -- bg = { 24, 24, 27 },
        -- grid = { 39, 39, 42 },
        -- grid_major = { 63, 63, 70 },
        -- bar = { 15, 15, 17 },
        -- bar_border = { 63, 63, 70 },
        -- bar_text = { 161, 161, 170 },
        -- square = { 102, 191, 255 },
        -- square_border = { 15, 15, 17 },
        -- square_text = { 15, 15, 17 },
        -- name = { 161, 161, 170 },
        -- pin_hover = { 250, 204, 21 },
        -- selection = { 250, 204, 21 }, -- border of selected squares and the selection box
        -- flash = { 253, 224, 71 },     -- glow of wires and squares when a tick goes through
        -- output = { 20, 20, 23 },      -- output panel
        -- output_text = { 228, 228, 231 },
        -- error = { 239, 68, 68 },      -- wires between incompatible types
        -- error_text = { 255, 255, 255 },
        -- field = { 244, 244, 245 },    -- text field of editable squares (constants)
        -- menu = { 39, 39, 42 },
        -- menu_border = { 63, 63, 70 },
        -- menu_hover = { 63, 63, 70 },
        -- menu_text = { 228, 228, 231 },
        -- pins and wires, by type
        -- types = {
                -- none = { 113, 113, 122 },
                -- tick = { 228, 228, 231 },
                -- any = { 161, 161, 170 },
                -- bool = { 248, 113, 113 },
                -- int = { 74, 222, 128 },
                -- float = { 190, 242, 100 },
                -- string = { 244, 114, 182 },
                -- ptr = { 192, 132, 252 },
        -- },
}

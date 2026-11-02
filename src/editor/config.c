/* config.c - the config: its keys, config.lua and settings.lua, read again when they change */
#define INCLUDE_CONF_IMPLEMENTATION /* conf.h: here, once */
#include "editor.h"

/* Default values, overwritten by the config file */
struct CfgState cfg = {
        .window = { .width = 1280, .height = 720, .fps = 60, .title = "lineit" },
        .canvas = {
        .grid_step           = 32.0f,
        .grid_major          = 8,
        .zoom_min            = 0.1f,
        .zoom_max            = 8.0f,
        .zoom_speed          = 0.2f,
        .square_size         = 64.0f,
        .pin_spacing         = 24.0f,
        .pin_font_size       = 14,
        .name_font_size      = 16,
        .pin_radius          = 7.0f,
        .wire_thickness      = 3.0f,
        .wire_curve_distance = 200.0f,
        .field_width         = 56.0f,
        .flash_time          = 0.5f,
        },
        .bar     = { .height = 32, .font_size = 20 },
        .menu    = { .font_size = 20, .padding = 8 },
        .font    = { .name = "Iosevka NFM", .spacing = 0 },
        .modules = { .path = "build/modules", .user = "modules", .source = "modules", .cache = "build/cache" },
        .save    = { .path = "canvas.lua", .autosave = false, .autosave_interval = 30 },
        .output  = { .height = 200, .font_size = 16, .lines = 2000, .clear_on_run = false },
        .compile = { .output = "build/bin", .runtime = "build/rt", .cc = "cc", .cflags = "-O2", .objcopy = "objcopy" },
        .theme   = "dark", /* its colors are put in config_start */
};

/* cfg as it was before reading the config, to read it again from scratch */
Typeof(cfg) cfg_defaults;

/* Report a bad value. Undefined keys are fine: the default is kept. True if
 * it was set. */
static bool
conf_check(int err, const char *path)
{
        if (err == CONF_OK || err == CONF_UNDEF) return err == CONF_OK;
        fprintf(stderr, "config: invalid value for %s, using default\n", path);
        return false;
}

static bool
conf_int(Conf conf, int *val, const char *path)
{
        return conf_check(Conf_get_int(conf, val, "%s", path), path);
}

static bool
conf_float(Conf conf, float *val, const char *path)
{
        double d;
        int err = Conf_get_num(conf, &d, "%s", path);
        if (err == CONF_OK) *val = d;
        return conf_check(err, path);
}

static bool
conf_bool(Conf conf, bool *val, const char *path)
{
        int b;
        int err = Conf_get_bool(conf, &b, "%s", path);
        if (err == CONF_OK) *val = b;
        return conf_check(err, path);
}

/* Strings are owned by Lua, so they are duplicated. */
static bool
conf_str(Conf conf, const char **val, const char *path)
{
        const char *str;
        int err = Conf_get_str(conf, &str, "%s", path);
        if (err == CONF_OK) *val = strdup(str);
        return conf_check(err, path);
}

/* Colors are written as { r, g, b } or { r, g, b, a } */
bool
conf_color(Conf conf, Color *val, const char *path)
{
        int len = 0, c[4] = { 0, 0, 0, 255 };
        int err = Conf_get_len(conf, &len, "%s", path);

        if (err == CONF_OK && len != 3 && len != 4) err = CONF_INVALID;
        for (int i = 0; err == CONF_OK && i < len; i++) {
                err = Conf_get_int(conf, &c[i], "%s.%d", path, i + 1);
                if (err == CONF_OK && (c[i] < 0 || c[i] > 255)) err = CONF_INVALID;
        }
        if (err == CONF_OK) *val = (Color) { c[0], c[1], c[2], c[3] };
        return conf_check(err, path);
}

/* The key is the field of cfg */
#define SETTING(field, t, sec, sub_, help_, ...) \
        { .key = #field, .type = t, .val = &cfg.field, .section = sec, .sub = sub_, .help = help_, __VA_ARGS__ }
#define TYPE_COLOR(t, name) \
        { .key = "colors.types." name, .type = SET_COLOR, .val = &cfg.colors.types[t], .section = "Colors", .sub = "Types", .help = "Pins and wires of type " name }

const Setting settings[] = {
        /* First: load_config reads it before the colors */
        SETTING(theme, SET_STR, "Appearance", "Theme", "dark, light, nord, gruvbox, or one of themes in the config"),
        SETTING(font.name, SET_STR, "Appearance", "Font", "fontconfig name (see fc-list) or a .ttf/.otf path; only .ttf/.otf work"),
        SETTING(font.spacing, SET_INT, "Appearance", "Font", "Extra pixels between letters", .min = -10, .max = 50),

        SETTING(window.width, SET_INT, "Interface", "Window", "Width when it opens", .min = 200, .max = 16384, .start = true),
        SETTING(window.height, SET_INT, "Interface", "Window", "Height when it opens", .min = 150, .max = 16384, .start = true),
        SETTING(window.fps, SET_INT, "Interface", "Window", "Frames per second", .min = 1, .max = 1000),
        SETTING(window.title, SET_STR, "Interface", "Window", "After the name of the tab shown", .empty = true),
        SETTING(bar.height, SET_INT, "Interface", "Bar", "Of the bar and the tab strip", .min = 12, .max = 200),
        SETTING(bar.font_size, SET_INT, "Interface", "Bar", "Text of the bar and the tabs", .min = 6, .max = 120),
        SETTING(menu.font_size, SET_INT, "Interface", "Menus", "Text of menus, panels and Settings", .min = 6, .max = 120),
        SETTING(menu.padding, SET_INT, "Interface", "Menus", "Space around the text of menus", .min = 0, .max = 100),
        SETTING(output.height, SET_INT, "Interface", "Output", "Of the panel, toggled with Output in the bar", .min = 40, .max = 4000),
        SETTING(output.font_size, SET_INT, "Interface", "Output", "Text of the output panel", .min = 6, .max = 120),
        SETTING(output.lines, SET_INT, "Interface", "Output", "Kept, older ones are dropped", .min = 1, .max = 1000000),
        SETTING(output.clear_on_run, SET_BOOL, "Interface", "Output", "Empty it when Run is pressed"),

        SETTING(canvas.grid_step, SET_FLOAT, "Canvas", "Grid", "World units between grid lines", .min = 4, .max = 1000),
        SETTING(canvas.grid_major, SET_INT, "Canvas", "Grid", "Every N lines is drawn as a major line", .min = 1, .max = 1000),
        SETTING(canvas.zoom_min, SET_FLOAT, "Canvas", "Zoom", "Furthest out", .min = 0.01f, .max = 1),
        SETTING(canvas.zoom_max, SET_FLOAT, "Canvas", "Zoom", "Closest in", .min = 1, .max = 32),
        SETTING(canvas.zoom_speed, SET_FLOAT, "Canvas", "Zoom", "Zoom change per wheel step (logarithmic)", .min = 0.01f, .max = 2),
        SETTING(canvas.square_size, SET_FLOAT, "Canvas", "Squares", "Minimum side, squares grow with their pins", .min = 8, .max = 1000),
        SETTING(canvas.pin_spacing, SET_FLOAT, "Canvas", "Squares", "Distance between pins on the same side", .min = 4, .max = 500),
        SETTING(canvas.pin_radius, SET_FLOAT, "Canvas", "Squares", "Input and output circles", .min = 1, .max = 100),
        SETTING(canvas.pin_font_size, SET_INT, "Canvas", "Squares", "Names of the pins", .min = 4, .max = 120),
        SETTING(canvas.name_font_size, SET_INT, "Canvas", "Squares", "Module name below the square", .min = 4, .max = 120),
        SETTING(canvas.field_width, SET_FLOAT, "Canvas", "Squares", "Fields where input values are typed", .min = 8, .max = 1000),
        SETTING(canvas.wire_thickness, SET_FLOAT, "Canvas", "Wires", "In world units", .min = 0.5f, .max = 50),
        SETTING(canvas.wire_curve_distance, SET_FLOAT, "Canvas", "Wires", "Shorter wires curve less (straight when very close)", .min = 1, .max = 10000),
        SETTING(canvas.flash_time, SET_FLOAT, "Canvas", "Wires", "Seconds wires and squares glow after a tick (0: off)", .min = 0, .max = 10),

        SETTING(save.path, SET_STR, "Files", "Saving", "Opened at start, lineit -f FILE overrides it", .start = true),
        SETTING(save.autosave, SET_BOOL, "Files", "Saving", "Save tabs with changes on their own (File > Autosave toggles it)"),
        SETTING(save.autosave_interval, SET_FLOAT, "Files", "Saving", "Seconds between autosaves", .min = 1, .max = 86400),
        SETTING(modules.path, SET_STR, "Files", "Modules", "Every .so, subgraph, type and package here is loaded", .start = true),
        SETTING(modules.user, SET_STR, "Files", "Modules", "Loaded too, your own: Make subgraph and New type write here"),
        SETTING(modules.source, SET_STR, "Files", "Modules", "The C of the modules in path, for Compile"),
        SETTING(modules.cache, SET_STR, "Files", "Modules", "Packages not built for this machine are built here"),

        SETTING(compile.cc, SET_STR, "Compile", "Compiler", "C compiler"),
        SETTING(compile.cflags, SET_STR, "Compile", "Compiler", "Its flags", .empty = true),
        SETTING(compile.objcopy, SET_STR, "Compile", "Compiler", "Gives each square of a package its own copy (llvm-objcopy works too)"),
        SETTING(compile.output, SET_STR, "Compile", "Paths", "Programs go here, their C in NAME.build/"),
        SETTING(compile.runtime, SET_STR, "Compile", "Paths", "What programs are built with, make builds it"),

        SETTING(colors.bg, SET_COLOR, "Colors", "Canvas", "Background"),
        SETTING(colors.grid, SET_COLOR, "Colors", "Canvas", "Grid lines"),
        SETTING(colors.grid_major, SET_COLOR, "Colors", "Canvas", "Every grid_major lines"),
        SETTING(colors.square, SET_COLOR, "Colors", "Canvas", "Squares"),
        SETTING(colors.square_border, SET_COLOR, "Colors", "Canvas", "Their border"),
        SETTING(colors.square_text, SET_COLOR, "Colors", "Canvas", "Text on squares and in fields"),
        SETTING(colors.name, SET_COLOR, "Colors", "Canvas", "Module names, hints"),
        SETTING(colors.field, SET_COLOR, "Colors", "Canvas", "Fields where values are typed"),
        SETTING(colors.pin_hover, SET_COLOR, "Colors", "Canvas", "Pin under the mouse, field being typed"),
        SETTING(colors.selection, SET_COLOR, "Colors", "Canvas", "Border of selected squares and the selection box"),
        SETTING(colors.flash, SET_COLOR, "Colors", "Canvas", "Glow of wires and squares when a tick goes through"),
        SETTING(colors.error, SET_COLOR, "Colors", "Canvas", "Wires between incompatible types, wrong values"),
        SETTING(colors.error_text, SET_COLOR, "Colors", "Canvas", "Text on errors"),
        SETTING(colors.bar, SET_COLOR, "Colors", "Bar and menus", "Bar and tab strip"),
        SETTING(colors.bar_border, SET_COLOR, "Colors", "Bar and menus", "Their border"),
        SETTING(colors.bar_text, SET_COLOR, "Colors", "Bar and menus", "Their text"),
        SETTING(colors.menu, SET_COLOR, "Colors", "Bar and menus", "Menus, panels, buttons"),
        SETTING(colors.menu_border, SET_COLOR, "Colors", "Bar and menus", "Their border"),
        SETTING(colors.menu_hover, SET_COLOR, "Colors", "Bar and menus", "Entry under the mouse"),
        SETTING(colors.menu_text, SET_COLOR, "Colors", "Bar and menus", "Their text"),
        SETTING(colors.output, SET_COLOR, "Colors", "Output", "Output panel"),
        SETTING(colors.output_text, SET_COLOR, "Colors", "Output", "Its text"),
        TYPE_COLOR(LI_NONE, "none"),
        TYPE_COLOR(LI_TICK, "tick"),
        TYPE_COLOR(LI_ANY, "any"),
        TYPE_COLOR(LI_BOOL, "bool"),
        TYPE_COLOR(LI_INT, "int"),
        TYPE_COLOR(LI_FLOAT, "float"),
        TYPE_COLOR(LI_STRING, "string"),
        TYPE_COLOR(LI_PTR, "ptr"),
        TYPE_COLOR(LI_LIST, "list"),
        TYPE_COLOR(LI_MAP, "map"),
        TYPE_COLOR(LI_REF, "ref"),
        TYPE_COLOR(LI_BYTES, "bytes"),
};
const int settings_count = countof(settings);
_Static_assert(countof(settings) <= SETTINGS_MAX, "SETTINGS_MAX in editor.h is too small");
#undef SETTING
#undef TYPE_COLOR
char setting_from[countof(settings)];

/* settings.lua, next to the config: what Settings changes, read after the
 * config so it wins. lineit writes it, the config is never touched. */
char settings_path[PATH_MAX];
char *setting_over[countof(settings)]; /* the value it has there, as Lua; NULL if none */

/* S in BASE, a whole cfg: cfg_defaults, or an old one */
static void *
setting_in(const Setting *s, void *base)
{
        return (char *) base + ((char *) s->val - (char *) &cfg);
}

static size_t
setting_size(const Setting *s)
{
        switch (s->type) {
        case SET_INT: return sizeof(int);
        case SET_FLOAT: return sizeof(float);
        case SET_BOOL: return sizeof(bool);
        case SET_STR: return sizeof(const char *);
        case SET_COLOR: return sizeof(Color);
        }
        return 0;
}

/* VAL, of the type of S, as Lua. Must be freed. */
char *
setting_lua(const Setting *s, const void *val)
{
        char *buf  = NULL;
        size_t len = 0;
        FILE *f    = open_memstream(&buf, &len);
        if (!f) return NULL;
        switch (s->type) {
        case SET_INT: fprintf(f, "%d", *(const int *) val); break;
        case SET_FLOAT: fprintf(f, "%g", *(const float *) val); break;
        case SET_BOOL: fprintf(f, "%s", *(const bool *) val ? "true" : "false"); break;
        case SET_STR: lua_quote(f, *(const char *const *) val); break;
        case SET_COLOR: {
                Color c = *(const Color *) val;
                if (c.a == 255)
                        fprintf(f, "{ %d, %d, %d }", c.r, c.g, c.b);
                else
                        fprintf(f, "{ %d, %d, %d, %d }", c.r, c.g, c.b, c.a);
                break;
        }
        }
        fclose(f);
        return buf;
}

/* S from CONF at PATH, into its place in cfg. True if CONF has it. */
bool
setting_read(Conf conf, const Setting *s, const char *path)
{
        switch (s->type) {
        case SET_INT: return conf_int(conf, s->val, path);
        case SET_FLOAT: return conf_float(conf, s->val, path);
        case SET_BOOL: return conf_bool(conf, s->val, path);
        case SET_COLOR: return conf_color(conf, s->val, path);
        case SET_STR: {
                const char **cur = s->val, *str = NULL;
                if (!conf_str(conf, &str, path)) return false;
                /* Read already from the config, if it isn't the default */
                if (*cur != *(const char **) setting_in(s, &cfg_defaults)) free((char *) *cur);
                *cur = str;
                return true;
        }
        }
        return false;
}

const char *config_path;
long long config_mtime, settings_mtime;

/* Free S, a string of cfg read with conf_str, unless it's still KEEP or
 * the default (a literal) */
static void
str_drop(const char *s, const char *keep, const char *def)
{
        if (s && s != keep && s != def) free((char *) s);
}

/* Read the config (and settings.lua) again from the defaults, so removed
 * keys go back to them. Colors, sizes and fonts change at once. */
void
config_reload(void)
{
        Typeof(cfg) prev = cfg;

        cfg = cfg_defaults;
        if (!load_config(config_path, false)) {
                cfg = prev;
                set_status("Could not load %s, keeping the old config", config_path);
                return;
        }

        /* Some are only used at start: they keep what they had */
        for (int i = 0; i < countof(settings); i++) {
                const Setting *s = &settings[i];
                if (!s->start) continue;
                if (s->type == SET_STR)
                        str_drop(*(const char **) s->val, *(const char **) setting_in(s, &prev),
                                 *(const char **) setting_in(s, &cfg_defaults));
                memcpy(s->val, setting_in(s, &prev), setting_size(s));
        }

        bool fonts = strcmp(cfg.font.name, prev.font.name) || cfg.font.spacing != prev.font.spacing ||
                     cfg.bar.font_size != prev.bar.font_size || cfg.menu.font_size != prev.menu.font_size ||
                     cfg.output.font_size != prev.output.font_size ||
                     cfg.canvas.pin_font_size != prev.canvas.pin_font_size ||
                     cfg.canvas.name_font_size != prev.canvas.name_font_size || cfg.canvas.zoom_max != prev.canvas.zoom_max;
        if (fonts) {
                unload_fonts();
                load_font();
        }
        if (strcmp(cfg.window.title, prev.window.title)) tab_title();
        if (cfg.window.fps != prev.window.fps) SetTargetFPS(cfg.window.fps);
        if (cfg.save.autosave != prev.save.autosave) autosave = cfg.save.autosave;
        camera.zoom = Clamp(camera.zoom, cfg.canvas.zoom_min, cfg.canvas.zoom_max);

        /* Sizes depend on fonts, spacing and field width */
        Da_foreach(sp, squares)
        {
                (*sp)->side = 0;
        }
        /* The old strings, now that nothing uses them */
        for (int i = 0; i < countof(settings); i++) {
                const Setting *s = &settings[i];
                if (s->type != SET_STR || s->start) continue;
                str_drop(*(const char **) setting_in(s, &prev), *(const char **) s->val,
                         *(const char **) setting_in(s, &cfg_defaults));
        }
        set_status("Reloaded %s", config_path);
}

#define _POSIX_C_SOURCE 200809L /* strdup, strtok_r */
#define _XOPEN_SOURCE   700     /* realpath */
#define _DEFAULT_SOURCE         /* SOCK_NONBLOCK, struct timeval */
#define INCLUDE_CONF_IMPLEMENTATION
#include "conf.h"
#include "cum.h"
#include "flag.h"
#include "lineit.h"
#include "raylib.h"
#include "raymath.h"

#include <ctype.h>
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <fontconfig/fontconfig.h>
#include <libgen.h>
#include <limits.h>
#include <poll.h>
#include <signal.h>
#include <stdarg.h>
#include <stdint.h>
#include <strings.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <sys/un.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>
#include <zlib.h>

#define BT_IMPLEMENTATION
#include "bt.h"

#ifndef VERSION
#define VERSION "unknown"
#endif

#define CONFIG_DEFAULT "config.lua"

#define countof(a) ((int) (sizeof(a) / sizeof *(a)))

/* Names used in the config (colors.types.<name>) */
static const char *type_names[LI_TYPE_COUNT] = {
        [LI_NONE]   = "none",
        [LI_TICK]   = "tick",
        [LI_ANY]    = "any",
        [LI_BOOL]   = "bool",
        [LI_INT]    = "int",
        [LI_FLOAT]  = "float",
        [LI_STRING] = "string",
        [LI_PTR]    = "ptr",
};

/* Default values, overwritten by the config file */
static struct {
        struct {
                int width;
                int height;
                int fps;
                const char *title;
        } window;
        struct {
                float grid_step;
                int grid_major;
                float zoom_min;
                float zoom_max;
                float zoom_speed;
                float square_size; /* minimum side, it grows with the pins */
                float pin_spacing;
                int pin_font_size;
                int name_font_size;
                float pin_radius;
                float wire_thickness;
                float wire_curve_distance;
                float field_width; /* typed values of the inputs */
                float flash_time;  /* how long wires and squares glow after a tick */
        } canvas;
        struct {
                int height;
                int font_size;
        } bar;
        struct {
                int font_size;
                int padding;
        } menu;
        struct {
                const char *name; /* fontconfig name ("Iosevka:bold") or a file path */
                int spacing;
        } font;
        struct {
                const char *path;
                const char *source; /* where Make subgraph writes them */
        } modules;
        struct {
                const char *path; /* --file overrides it */
        } save;
        struct {
                int height; /* of the panel, pixels */
                int font_size;
                int lines; /* kept, older ones are dropped */
                bool clear_on_start;
        } output;
        struct {
                Color bg;
                Color grid;
                Color grid_major;
                Color bar;
                Color bar_border;
                Color bar_text;
                Color square;
                Color square_border;
                Color square_text;
                Color name;
                Color pin_hover;
                Color selection;
                Color flash;
                Color output;
                Color output_text;
                Color error;
                Color error_text;
                Color field;
                Color types[LI_TYPE_COUNT]; /* pins and wires */
                Color menu;
                Color menu_border;
                Color menu_hover;
                Color menu_text;
        } colors;
} cfg = {
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
        .modules = { .path = "build/modules", .source = "modules" },
        .save    = { .path = "canvas.lua" },
        .output  = { .height = 200, .font_size = 16, .lines = 2000, .clear_on_start = false },
        .colors  = {
        .bg            = { 24, 24, 27, 255 },
        .grid          = { 39, 39, 42, 255 },
        .grid_major    = { 63, 63, 70, 255 },
        .bar           = { 15, 15, 17, 255 },
        .bar_border    = { 63, 63, 70, 255 },
        .bar_text      = { 161, 161, 170, 255 },
        .square        = { 102, 191, 255, 255 },
        .square_border = { 15, 15, 17, 255 },
        .square_text   = { 15, 15, 17, 255 },
        .name          = { 161, 161, 170, 255 },
        .pin_hover     = { 250, 204, 21, 255 },
        .selection     = { 250, 204, 21, 255 },
        .flash         = { 253, 224, 71, 255 },
        .output        = { 20, 20, 23, 255 },
        .output_text   = { 228, 228, 231, 255 },
        .error         = { 239, 68, 68, 255 },
        .error_text    = { 255, 255, 255, 255 },
        .field         = { 244, 244, 245, 255 },
        .types         = {
        [LI_NONE]   = { 113, 113, 122, 255 },
        [LI_TICK]   = { 228, 228, 231, 255 },
        [LI_ANY]    = { 161, 161, 170, 255 },
        [LI_BOOL]   = { 248, 113, 113, 255 },
        [LI_INT]    = { 74, 222, 128, 255 },
        [LI_FLOAT]  = { 190, 242, 100, 255 },
        [LI_STRING] = { 244, 114, 182, 255 },
        [LI_PTR]    = { 192, 132, 252, 255 },
        },
        .menu        = { 39, 39, 42, 255 },
        .menu_border = { 63, 63, 70, 255 },
        .menu_hover  = { 63, 63, 70, 255 },
        .menu_text   = { 228, 228, 231, 255 },
        },
};

/* cfg as it was before reading the config, to read it again from scratch */
static Typeof(cfg) cfg_defaults;

/* Report a bad value. Undefined keys are fine: the default is kept. */
static void
conf_check(int err, const char *path)
{
        if (err == CONF_OK || err == CONF_UNDEF) return;
        fprintf(stderr, "config: invalid value for %s, using default\n", path);
}

static void
conf_int(Conf conf, int *val, const char *path)
{
        conf_check(Conf_get_int(conf, val, path), path);
}

static void
conf_float(Conf conf, float *val, const char *path)
{
        double d;
        int err = Conf_get_num(conf, &d, path);
        if (err == CONF_OK) *val = d;
        conf_check(err, path);
}

static void
conf_bool(Conf conf, bool *val, const char *path)
{
        int b;
        int err = Conf_get_bool(conf, &b, path);
        if (err == CONF_OK) *val = b;
        conf_check(err, path);
}

/* Strings are owned by Lua, so they are duplicated. */
static void
conf_str(Conf conf, const char **val, const char *path)
{
        const char *str;
        int err = Conf_get_str(conf, &str, path);
        if (err == CONF_OK) *val = strdup(str);
        conf_check(err, path);
}

/* Colors are written as { r, g, b } or { r, g, b, a } */
static void
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
        conf_check(err, path);
}

/* Returns false if PATH can't be read (missing, or not valid Lua) */
static bool
load_config(const char *path)
{
        Conf conf;

        if (Conf_open(&conf, path) != CONF_OK) {
                fprintf(stderr, "config: could not load %s, using defaults\n", path);
                return false;
        }

        conf_int(conf, &cfg.window.width, "window.width");
        conf_int(conf, &cfg.window.height, "window.height");
        conf_int(conf, &cfg.window.fps, "window.fps");
        conf_str(conf, &cfg.window.title, "window.title");

        conf_float(conf, &cfg.canvas.grid_step, "canvas.grid_step");
        conf_int(conf, &cfg.canvas.grid_major, "canvas.grid_major");
        conf_float(conf, &cfg.canvas.zoom_min, "canvas.zoom_min");
        conf_float(conf, &cfg.canvas.zoom_max, "canvas.zoom_max");
        conf_float(conf, &cfg.canvas.zoom_speed, "canvas.zoom_speed");
        conf_float(conf, &cfg.canvas.square_size, "canvas.square_size");
        conf_float(conf, &cfg.canvas.pin_spacing, "canvas.pin_spacing");
        conf_int(conf, &cfg.canvas.pin_font_size, "canvas.pin_font_size");
        conf_int(conf, &cfg.canvas.name_font_size, "canvas.name_font_size");
        conf_float(conf, &cfg.canvas.pin_radius, "canvas.pin_radius");
        conf_float(conf, &cfg.canvas.wire_thickness, "canvas.wire_thickness");
        conf_float(conf, &cfg.canvas.wire_curve_distance, "canvas.wire_curve_distance");
        conf_float(conf, &cfg.canvas.field_width, "canvas.field_width");
        conf_float(conf, &cfg.canvas.flash_time, "canvas.flash_time");

        conf_int(conf, &cfg.bar.height, "bar.height");
        conf_int(conf, &cfg.bar.font_size, "bar.font_size");

        conf_int(conf, &cfg.menu.font_size, "menu.font_size");
        conf_int(conf, &cfg.menu.padding, "menu.padding");

        conf_str(conf, &cfg.font.name, "font.name");
        conf_int(conf, &cfg.font.spacing, "font.spacing");

        conf_str(conf, &cfg.modules.path, "modules.path");
        conf_str(conf, &cfg.modules.source, "modules.source");
        conf_str(conf, &cfg.save.path, "save.path");

        conf_int(conf, &cfg.output.height, "output.height");
        conf_int(conf, &cfg.output.font_size, "output.font_size");
        conf_int(conf, &cfg.output.lines, "output.lines");
        conf_bool(conf, &cfg.output.clear_on_start, "output.clear_on_start");

        conf_color(conf, &cfg.colors.bg, "colors.bg");
        conf_color(conf, &cfg.colors.grid, "colors.grid");
        conf_color(conf, &cfg.colors.grid_major, "colors.grid_major");
        conf_color(conf, &cfg.colors.bar, "colors.bar");
        conf_color(conf, &cfg.colors.bar_border, "colors.bar_border");
        conf_color(conf, &cfg.colors.bar_text, "colors.bar_text");
        conf_color(conf, &cfg.colors.square, "colors.square");
        conf_color(conf, &cfg.colors.square_border, "colors.square_border");
        conf_color(conf, &cfg.colors.square_text, "colors.square_text");
        conf_color(conf, &cfg.colors.name, "colors.name");
        conf_color(conf, &cfg.colors.pin_hover, "colors.pin_hover");
        conf_color(conf, &cfg.colors.selection, "colors.selection");
        conf_color(conf, &cfg.colors.flash, "colors.flash");
        conf_color(conf, &cfg.colors.output, "colors.output");
        conf_color(conf, &cfg.colors.output_text, "colors.output_text");
        conf_color(conf, &cfg.colors.error, "colors.error");
        conf_color(conf, &cfg.colors.error_text, "colors.error_text");
        conf_color(conf, &cfg.colors.field, "colors.field");
        conf_color(conf, &cfg.colors.menu, "colors.menu");
        conf_color(conf, &cfg.colors.menu_border, "colors.menu_border");
        conf_color(conf, &cfg.colors.menu_hover, "colors.menu_hover");
        conf_color(conf, &cfg.colors.menu_text, "colors.menu_text");
        for (int i = 0; i < LI_TYPE_COUNT; i++) {
                char key[64];
                snprintf(key, sizeof key, "colors.types.%s", type_names[i]);
                conf_color(conf, &cfg.colors.types[i], key);
        }

        Conf_close(conf);

        if (cfg.canvas.grid_step <= 0) cfg.canvas.grid_step = 32.0f;
        if (cfg.canvas.grid_major <= 0) cfg.canvas.grid_major = 1;
        if (cfg.canvas.wire_curve_distance <= 0) cfg.canvas.wire_curve_distance = 1.0f;
        if (cfg.canvas.pin_spacing <= 0) cfg.canvas.pin_spacing = 24.0f;
        if (cfg.output.lines < 1) cfg.output.lines = 1;
        return true;
}

typedef struct Module Module;

typedef enum PinKind {
        PIN_WIRE,    /* only a wire */
        PIN_VALUE,   /* a wire, or a value typed in the square when unplugged */
        PIN_OPTIONS, /* no wire: one of a list, chosen in the square */
} PinKind;

typedef struct PinInfo {
        char *name;
        Li_Type type;
        PinKind kind;
        /* Outputs: the last value set. PIN_VALUE: what's typed.
         * PIN_OPTIONS: .i is the chosen one. */
        Li_Value value;
        char *str;      /* owns value.s when it's a typed string */
        char **options; /* PIN_OPTIONS */
        int option_count;
} PinInfo;

typedef Da(PinInfo) PinList;

/* A setting of a square, see li_param */
typedef struct Param {
        char *name;
        Li_Value value; /* for LI_STRING, .s is str */
        char *str;
} Param;

typedef Da(Param) ParamList;

/* What a module .so defines, see lineit.h */
typedef struct ModuleFns {
        void (*init)(Li_Node *n);
        void (*tick)(Li_Node *n, int input);
        void (*eval)(Li_Node *n);
        void (*frame)(Li_Node *n);
        void (*destroy)(Li_Node *n);
        void (*param)(Li_Node *n, int param);
        const char *(*save)(Li_Node *n);
        void (*load)(Li_Node *n, const char *state);
        void (*draw)(Li_Node *n);
        void (*start)(Li_Node *n);
} ModuleFns;

/* A square on the canvas, an instance of a module. Modules see it as an
 * opaque Li_Node. */
struct Li_Node {
        int id;
        Vector2 pos; /* top-left corner, world space */
        Module *module;
        void *handle; /* its own copy of the .so, see square_new */
        ModuleFns fn;
        PinList inputs;
        PinList outputs;
        ParamList params; /* changed from the settings panel */
        float side;       /* cached by square_side, 0 to recompute */
        float label_w;    /* longest input name, cached with side */
        float field_w;    /* widest input field, cached with side */
        bool selected;
        double flash;     /* last time a tick reached it */
        double fired[16]; /* last time each tick output fired, for the tooltip */
        void *data;       /* module state, see li_set_data */
        char text[64];    /* drawn inside, see li_text */
        bool evaluating;  /* guards module_eval against cycles */
        struct Graph *graph; /* a subgraph: its own canvas, see Subgraphs */
        struct Li_Node *owner; /* graph/input and graph/output in one: the subgraph */
        int port;              /* and which of its inputs or outputs they are */
};
typedef struct Li_Node Square;

/* Wire from an output of square `from` to an input of square `to` */
typedef struct Link {
        int from, from_pin; /* square id, output index */
        int to, to_pin;     /* square id, input index */
        double flash;       /* last time something went through it */
} Link;

/* A .so found in the modules dir. It's not kept open: each square opens
 * its own copy. */
struct Module {
        char *path;        /* the .so, or the .lua of a subgraph. NULL for built-in modules */
        const ModuleFns *builtin; /* callbacks in the editor itself instead of a .so */
        char *lib;                /* a package (.li): its .so for here, taken out (see Packages) */
        char *name;        /* path inside the modules dir, no extension: "math/add" */
        const char *label; /* last part of the name: "add" */
        long long mtime;   /* of the .so loaded, ns, to see when it's rebuilt */
        bool gone;         /* the .so was removed: not in the menu, its squares still work */
        bool seen;         /* found in the last scan */
};

typedef enum Part {
        PART_NONE,
        PART_INPUT,
        PART_OUTPUT,
        PART_FIELD, /* the typed value of an input, pin is its index */
        PART_BODY,
} Part;

/* What is under a point: a square, which part of it, and the pin index */
typedef struct Hit {
        Square *square;
        Part part;
        int pin;
} Hit;

/* What the left mouse button is currently dragging */
static struct {
        enum { DRAG_NONE,
               DRAG_SQUARE, /* moving the selection */
               DRAG_WIRE,
               DRAG_BOX } kind; /* selecting with a box */
        Square *square;         /* wire source */
        int pin;                /* wire source output */
        Vector2 start;          /* where it started, world space */
        Vector2 last;           /* mouse on the last frame, world space */
} drag;

static Font font;        /* bar and menus, at their size */
static Font canvas_font; /* squares: big, so it's sharp zoomed in */
static Camera2D camera = { .zoom = 1.0f };
typedef Da(Square *) SquareList;
typedef Da(Link) LinkList;
static SquareList squares; /* draw order: last is on top. Inside a subgraph, its own (see Subgraphs) */
static LinkList links;
static int next_id;
static Da(Module *) modules; /* sorted by name */
static Module *last_module;  /* the last one placed, first in the canvas menu */
static bool running;         /* module_frame is only called while running */
static const ModuleFns graph_fns;
static void square_open_tab(int id, void *arg);
static void menu_subgraph(int id, void *arg);
static void tabs_run_background(void);
static bool prompt_is_open(void);

typedef struct MenuItem MenuItem;

/* Same layout as Da(MenuItem), so the Da_* macros work on it */
typedef struct Menu {
        int capacity;
        int count;
        MenuItem *items;
} Menu;

/* Context menu entry. FN gets the id of the object the menu was opened on
 * (-1 on the empty canvas) and ARG. The world point clicked is in
 * menu.world. Entries with SUB open that menu instead. */
struct MenuItem {
        const char *label;
        const char *hint; /* drawn at the right, like a shortcut */
        void (*fn)(int id, void *arg);
        void *arg;
        Menu *sub;
};

#define MENU_DEPTH 8

typedef struct MenuLevel {
        Menu *menu;
        Vector2 pos; /* top-left corner, screen space */
        int scroll;  /* first entry shown, when they don't all fit */
} MenuLevel;

static struct {
        int depth; /* open levels (submenus), 0 when closed */
        MenuLevel level[MENU_DEPTH];
        int id;              /* object the menu belongs to */
        Vector2 world;       /* where it was opened, world space */
        Vector2 right_press; /* where the right button went down */
        bool searchable;     /* the canvas menu: typing searches the modules */
        float search_w;      /* its width, fixed while it's open so it doesn't move */
        int selected;        /* entry chosen with the arrow keys, -1 if none */
} menu;

/* What's typed in the search box of the canvas menu. Its type is defined
 * later, so it's kept apart. */
static struct TextInput *menu_search(void);

static Menu module_menu; /* one entry per module, folders are submenus */
static Menu search_menu; /* the modules that match the search */
static Menu canvas_menu; /* rebuilt each time it opens */

static void
init_raylib(void)
{
        SetConfigFlags(FLAG_WINDOW_RESIZABLE | FLAG_MSAA_4X_HINT | FLAG_VSYNC_HINT);
        SetTraceLogLevel(LOG_WARNING);
        InitWindow(cfg.window.width, cfg.window.height, cfg.window.title);
        SetExitKey(KEY_NULL);
        SetTargetFPS(cfg.window.fps);
}

/* Raylib only loads these. Others (.ttc collections) silently give the
 * default font. */
#define FONT_EXTENSIONS ".ttf;.otf"

/* Path of the best font file fontconfig has for NAME that raylib can load,
 * or NULL. Must be freed. */
static char *
font_find(const char *name)
{
        FcPattern *pattern;
        FcFontSet *set;
        FcChar8 *file;
        FcResult result;
        char *path = NULL;

        if (!FcInit()) return NULL;
        if ((pattern = FcNameParse((const FcChar8 *) name)) == NULL) return NULL;

        FcConfigSubstitute(NULL, pattern, FcMatchPattern);
        FcDefaultSubstitute(pattern);

        /* Best matches first */
        if ((set = FcFontSort(NULL, pattern, FcTrue, NULL, &result))) {
                for (int i = 0; !path && i < set->nfont; i++) {
                        if (FcPatternGetString(set->fonts[i], FC_FILE, 0, &file) == FcResultMatch &&
                            IsFileExtension((char *) file, FONT_EXTENSIONS))
                                path = strdup((char *) file);
                }
                FcFontSetDestroy(set);
        }
        FcPatternDestroy(pattern);
        return path;
}

/* PATH at SIZE, or raylib's default font if it fails */
static Font
font_load(const char *path, int size)
{
        Font f = path ? LoadFontEx(path, size, NULL, 0) : GetFontDefault();
        if (!IsFontValid(f) || f.texture.id == GetFontDefault().texture.id) {
                fprintf(stderr, "font: could not load %s, using default\n", path ? path : cfg.font.name);
                f = GetFontDefault();
        }
        return f;
}

/* Load cfg.font.name twice: at the UI size, and big for the canvas */
static void
load_font(void)
{
        char *path = FileExists(cfg.font.name) ? strdup(cfg.font.name) : font_find(cfg.font.name);

        /* Rasterized at the biggest size used, smaller ones are scaled down */
        font = font_load(path, Max(Max(cfg.bar.font_size, cfg.menu.font_size), cfg.output.font_size));
        SetTextureFilter(font.texture, TEXTURE_FILTER_BILINEAR);

        /* Canvas text is scaled by the zoom: rasterize it at the size it has
         * at max zoom, and use mipmaps so it's not jagged zoomed out */
        int size    = Max(cfg.canvas.pin_font_size, cfg.canvas.name_font_size) * cfg.canvas.zoom_max;
        canvas_font = font_load(path, Clamp(size, 16, 256));
        if (canvas_font.texture.id != GetFontDefault().texture.id) {
                GenTextureMipmaps(&canvas_font.texture);
                SetTextureFilter(canvas_font.texture, TEXTURE_FILTER_TRILINEAR);
        }
        free(path);
}

static void
unload_fonts(void)
{
        if (font.texture.id != GetFontDefault().texture.id) UnloadFont(font);
        if (canvas_font.texture.id != GetFontDefault().texture.id) UnloadFont(canvas_font);
        /* What font_find's FcInit loaded: it's done again when fonts load again */
        FcFini();
}

static void
draw_text(const char *text, float x, float y, float size, Color color)
{
        DrawTextEx(font, text, (Vector2) { x, y }, size, cfg.font.spacing, color);
}

static float
measure_text(const char *text, float size)
{
        return MeasureTextEx(font, text, size, cfg.font.spacing).x;
}

/* Same, for text on the canvas (world space) */
static void
draw_canvas_text(const char *text, float x, float y, float size, Color color)
{
        DrawTextEx(canvas_font, text, (Vector2) { x, y }, size, cfg.font.spacing, color);
}

static float
measure_canvas_text(const char *text, float size)
{
        return MeasureTextEx(canvas_font, text, size, cfg.font.spacing).x;
}

/* ---- Output panel: what print writes ---- */

typedef struct OutputLine {
        char *text;
        bool info; /* a message of the editor itself, not of a module */
} OutputLine;

static struct {
        Da(OutputLine) lines;
        bool open;
        int scroll; /* lines up from the newest one */
        int unseen; /* added while it was closed */
} output;

/* Add TEXT, one line per '\n' in it */
static void
output_add(const char *text, bool info)
{
        const char *p = text;
        do {
                const char *nl = strchr(p, '\n');
                int len        = nl ? (int) (nl - p) : (int) strlen(p);
                Da_append(&output.lines, (OutputLine) { .text = strndup(p, len), .info = info });
                if (!output.open) output.unseen++;
                if (output.scroll) output.scroll++; /* scrolled up: keep what's shown */
                p += len + (nl != NULL);
        } while (*p);

        /* Oldest first out. Remove in one go: the list can be long. */
        int extra = output.lines.count - cfg.output.lines;
        if (extra > 0) {
                for (int i = 0; i < extra; i++)
                        free(output.lines.items[i].text);
                memmove(output.lines.items, output.lines.items + extra, (output.lines.count - extra) * sizeof *output.lines.items);
                output.lines.count -= extra;
        }
}

static void
output_clear(void)
{
        Da_foreach(l, output.lines)
        {
                free(l->text);
        }
        output.lines.count = 0;
        output.scroll      = 0;
        output.unseen      = 0;
}

/* The panel, above the bar, while it's open */
static Rectangle
output_rect(void)
{
        float h = output.open ? Min(cfg.output.height, GetScreenHeight() - cfg.bar.height - 40) : 0;
        return (Rectangle) { 0, GetScreenHeight() - cfg.bar.height - h, GetScreenWidth(), h };
}

static float tabs_height(void);

/* Area of the screen where the canvas is drawn (everything but the bar and
 * the output panel). */
static Rectangle
canvas_rect(void)
{
        float top = tabs_height();
        return (Rectangle) { 0, top, GetScreenWidth(), output_rect().y - top };
}

static void
select_all(bool on)
{
        Da_foreach(sp, squares)
        {
                (*sp)->selected = on;
        }
}

static Vector2
mouse_world(void)
{
        return GetScreenToWorld2D(GetMousePosition(), camera);
}

static Square *
square_get(int id)
{
        Da_foreach(s, squares)
        {
                if ((*s)->id == id) return *s;
        }
        return NULL;
}

static int
square_index(Square *s)
{
        for (int i = 0; i < squares.count; i++) {
                if (squares.items[i] == s) return i;
        }
        return -1;
}

/* Height of the strip at the top where the text goes, above the pins */
static float
square_text_band(void)
{
        return cfg.canvas.pin_font_size + 8;
}

/* Space between a pin and its name */
static float
pin_label_gap(void)
{
        return cfg.canvas.pin_radius + 4;
}

static float
pins_label_width(PinList *l)
{
        float w = 0;
        Da_foreach(p, *l)
        {
                w = fmaxf(w, measure_canvas_text(p->name, cfg.canvas.pin_font_size));
        }
        return w;
}

/* Drawn at the right of option fields */
#define FIELD_ARROW "v"

/* Width of the field of input P, 0 if it has none */
static float
field_width(PinInfo *p)
{
        float fs = cfg.canvas.pin_font_size;
        float w  = cfg.canvas.field_width;

        if (p->kind == PIN_WIRE) return 0;
        if (p->kind == PIN_VALUE && p->type == LI_BOOL) return fs + 6; /* a checkbox */
        for (int i = 0; i < p->option_count; i++)
                w = fmaxf(w, measure_canvas_text(p->options[i], fs) + measure_canvas_text(FIELD_ARROW, fs) + 16);
        return w;
}

/* Always a square: as tall as the text and the pins need, and as wide as
 * the longest input name, its field and the longest output name side by
 * side */
static float
square_side(Square *s)
{
        if (s->side) return s->side;

        s->label_w = pins_label_width(&s->inputs);
        s->field_w = 0;
        Da_foreach(p, s->inputs)
        {
                s->field_w = fmaxf(s->field_w, field_width(p));
        }

        int pins     = Max(s->inputs.count, s->outputs.count);
        float height = square_text_band() + (pins + 1) * cfg.canvas.pin_spacing;
        float width  = s->label_w + (s->field_w ? s->field_w + 6 : 0) + pins_label_width(&s->outputs) +
                       2 * pin_label_gap() + 12;
        s->side      = fmaxf(fmaxf(height, width), cfg.canvas.square_size);
        return s->side;
}

static Rectangle
square_rect(Square *s)
{
        float side = square_side(s);
        return (Rectangle) { s->pos.x, s->pos.y, side, side };
}

static PinInfo *
pin_info(Square *s, Part part, int i)
{
        PinList *l = part == PART_INPUT ? &s->inputs : &s->outputs;
        return i >= 0 && i < l->count ? &l->items[i] : NULL;
}

/* Inputs on the left side, outputs on the right, centered vertically below
 * the text band */
static Vector2
pin_pos(Square *s, Part part, int i)
{
        float side = square_side(s);
        float band = square_text_band();
        int count  = part == PART_INPUT ? s->inputs.count : s->outputs.count;
        float y    = s->pos.y + band + (side - band) / 2 + (i - (count - 1) / 2.0f) * cfg.canvas.pin_spacing;
        float x    = part == PART_INPUT ? s->pos.x : s->pos.x + side;
        return (Vector2) { x, y };
}

static int link_to(int id, int pin);

/* Field where the value of input I is typed or chosen, after the names */
static Rectangle
field_rect(Square *s, int i)
{
        Vector2 p = pin_pos(s, PART_INPUT, i);
        float h   = cfg.canvas.pin_font_size + 6;
        square_side(s);
        return (Rectangle) { p.x + pin_label_gap() + s->label_w + 6, p.y - h / 2, field_width(&s->inputs.items[i]), h };
}

/* Options are always shown, typed values only while nothing is plugged in */
static bool
field_visible(Square *s, int i)
{
        PinInfo *p = &s->inputs.items[i];
        return p->kind == PIN_OPTIONS || (p->kind == PIN_VALUE && link_to(s->id, i) < 0);
}

/* Topmost square under world point P and which part of it was hit. */
static Hit
square_hit(Vector2 p)
{
        /* A bit bigger than the drawn circle so pins are easy to grab */
        float r = cfg.canvas.pin_radius * 1.5f;

        for (int i = squares.count - 1; i >= 0; i--) {
                Square *s = squares.items[i];
                for (int j = 0; j < s->outputs.count; j++) {
                        if (CheckCollisionPointCircle(p, pin_pos(s, PART_OUTPUT, j), r))
                                return (Hit) { s, PART_OUTPUT, j };
                }
                for (int j = 0; j < s->inputs.count; j++) {
                        if (s->inputs.items[j].kind != PIN_OPTIONS && CheckCollisionPointCircle(p, pin_pos(s, PART_INPUT, j), r))
                                return (Hit) { s, PART_INPUT, j };
                        if (field_visible(s, j) && CheckCollisionPointRec(p, field_rect(s, j)))
                                return (Hit) { s, PART_FIELD, j };
                }
                if (CheckCollisionPointRec(p, square_rect(s))) return (Hit) { s, PART_BODY, -1 };
        }
        return (Hit) { NULL, PART_NONE, -1 };
}

/* Index of the last link plugged into input PIN of square ID, or -1 */
static int
link_to(int id, int pin)
{
        for (int i = links.count - 1; i >= 0; i--) {
                if (links.items[i].to == id && links.items[i].to_pin == pin) return i;
        }
        return -1;
}

static bool
pin_connected(Square *s, Part part, int pin)
{
        Da_foreach(l, links)
        {
                if (part == PART_INPUT && l->to == s->id && l->to_pin == pin) return true;
                if (part == PART_OUTPUT && l->from == s->id && l->from_pin == pin) return true;
        }
        return false;
}

/* Ticks only go to ticks. Data goes to the same type, or from/to LI_ANY. */
static bool
types_compatible(Li_Type out, Li_Type in)
{
        if (out == LI_TICK || in == LI_TICK) return out == in;
        return out == in || out == LI_ANY || in == LI_ANY;
}

/* Any output can be wired to any input of another square. Wires between
 * incompatible types are kept, but carry nothing and show an error. */
static bool
link_allowed(Square *from, int from_pin, Square *to, int to_pin)
{
        PinInfo *in = pin_info(to, PART_INPUT, to_pin);
        return pin_info(from, PART_OUTPUT, from_pin) && in && in->kind != PIN_OPTIONS && from != to;
}

static bool
link_valid(Link k)
{
        Square *from = square_get(k.from);
        Square *to   = square_get(k.to);
        if (!from || !to) return false;
        return types_compatible(pin_info(from, PART_OUTPUT, k.from_pin)->type,
                                pin_info(to, PART_INPUT, k.to_pin)->type);
}

static void
link_add(Square *from, int from_pin, Square *to, int to_pin)
{
        int l;

        if (!link_allowed(from, from_pin, to, to_pin)) return;

        /* Data inputs take one wire, tick inputs can merge many */
        if (pin_info(to, PART_INPUT, to_pin)->type != LI_TICK) {
                while ((l = link_to(to->id, to_pin)) >= 0)
                        Da_remove(&links, l);
        }
        Da_foreach(k, links)
        {
                if (k->from == from->id && k->from_pin == from_pin && k->to == to->id && k->to_pin == to_pin)
                        return;
        }
        Da_append(&links, (Link) { .from = from->id, .from_pin = from_pin, .to = to->id, .to_pin = to_pin });
}

/* ---- Module API (lineit.h). Exported to the modules with -rdynamic ---- */

static int
pin_add(PinList *l, const char *name, Li_Type type)
{
        Da_append(l, (PinInfo) { .name = strdup(name ? name : ""), .type = type });
        return l->count - 1;
}

/* Set the typed value of P, copying strings */
static void
pin_store(PinInfo *p, Li_Value value)
{
        char *old = p->str;
        p->str    = value.type == LI_STRING ? strdup(value.s ? value.s : "") : NULL;
        p->value  = value;
        if (p->str) p->value.s = p->str;
        free(old);
}

int
li_input(Li_Node *n, const char *name, Li_Type type)
{
        n->side = 0;
        return pin_add(&n->inputs, name, type);
}

int
li_input_value(Li_Node *n, const char *name, Li_Type type, Li_Value value)
{
        int i      = li_input(n, name, type);
        PinInfo *p = &n->inputs.items[i];
        p->kind    = PIN_VALUE;
        pin_store(p, value);
        return i;
}

int
li_input_options(Li_Node *n, const char *name, const char *options, int chosen)
{
        int i      = li_input(n, name, LI_INT);
        PinInfo *p = &n->inputs.items[i];
        char *list = strdup(options ? options : "");
        char *save;

        p->kind = PIN_OPTIONS;
        for (char *o = strtok_r(list, "|", &save); o; o = strtok_r(NULL, "|", &save)) {
                p->options                    = realloc(p->options, (p->option_count + 1) * sizeof *p->options);
                p->options[p->option_count++] = strdup(o);
        }
        free(list);
        p->value = li_int(Clamp(chosen, 0, Max(p->option_count - 1, 0)));
        return i;
}

const char *
li_option(Li_Node *n, int input)
{
        PinInfo *p = pin_info(n, PART_INPUT, input);
        if (!p || p->kind != PIN_OPTIONS || p->value.i < 0 || p->value.i >= p->option_count) return "";
        return p->options[p->value.i];
}

int
li_output(Li_Node *n, const char *name, Li_Type type)
{
        n->side = 0;
        return pin_add(&n->outputs, name, type);
}

/* How deep in a tick chain we are, 0 outside of one */
static int tick_depth;

Li_Value
li_get(Li_Node *n, int input)
{
        PinInfo *in = pin_info(n, PART_INPUT, input);
        if (!in) return (Li_Value) { .type = LI_NONE };
        if (in->kind == PIN_OPTIONS) return in->value;

        /* Unplugged: what's typed in the square, if it has a field */
        int l = link_to(n->id, input);
        if (l < 0) return in->kind == PIN_VALUE ? in->value : (Li_Value) { .type = LI_NONE };

        Link k      = links.items[l];
        Square *src = square_get(k.from);
        if (!src || !link_valid(k)) return (Li_Value) { .type = LI_NONE };

        /* Data read while ticking glows too (not what's drawn every frame) */
        if (tick_depth > 0) links.items[l].flash = GetTime();

        /* Pure squares compute their outputs when someone reads them */
        if (src->fn.eval && !src->evaluating) {
                src->evaluating = true;
                src->fn.eval(src);
                src->evaluating = false;
        }
        PinInfo *out = pin_info(src, PART_OUTPUT, k.from_pin);
        return out ? out->value : (Li_Value) { .type = LI_NONE };
}

void
li_output_type(Li_Node *n, int output, Li_Type type)
{
        PinInfo *out = pin_info(n, PART_OUTPUT, output);
        if (out) out->type = type;
}

void
li_set(Li_Node *n, int output, Li_Value value)
{
        PinInfo *out = pin_info(n, PART_OUTPUT, output);
        if (out) out->value = value;
}

/* Tick chains are plain recursion, this stops loops in the graph */
#define TICK_DEPTH_MAX 256

void
li_tick(Li_Node *n, int output)
{
        static bool warned;
        double now = GetTime();

        if (output >= 0 && output < (int) countof(n->fired)) n->fired[output] = now;
        if (tick_depth >= TICK_DEPTH_MAX) {
                if (!warned) fprintf(stderr, "tick: more than %d squares deep, is there a loop?\n", TICK_DEPTH_MAX);
                warned = true;
                return;
        }

        tick_depth++;
        for (int i = 0; i < links.count; i++) {
                Link k = links.items[i];
                if (k.from != n->id || k.from_pin != output || !link_valid(k)) continue;
                Square *dst          = square_get(k.to);
                links.items[i].flash = now;
                if (dst) dst->flash = now;
                if (dst && dst->fn.tick) dst->fn.tick(dst, k.to_pin);
        }
        tick_depth--;
}

void *
li_data(Li_Node *n)
{
        return n->data;
}

void
li_set_data(Li_Node *n, void *data)
{
        n->data = data;
}

void
li_text(Li_Node *n, const char *fmt, ...)
{
        va_list ap;
        va_start(ap, fmt);
        vsnprintf(n->text, sizeof n->text, fmt, ap);
        va_end(ap);
}

void
li_print(Li_Node *n, const char *fmt, ...)
{
        char buf[1024];
        va_list ap;
        (void) n;
        va_start(ap, fmt);
        vsnprintf(buf, sizeof buf, fmt, ap);
        va_end(ap);
        output_add(buf, false);
        printf("%s\n", buf);
}

double
li_time(void)
{
        return GetTime();
}

int
li_param(Li_Node *n, const char *name, Li_Value value)
{
        if (value.type != LI_INT && value.type != LI_FLOAT && value.type != LI_BOOL && value.type != LI_STRING) {
                fprintf(stderr, "%s: param %s must be int, float, bool or string\n", n->module->name, name);
                return -1;
        }

        Param p = { .name = strdup(name ? name : ""), .value = value };
        if (value.type == LI_STRING) {
                p.str     = strdup(value.s ? value.s : "");
                p.value.s = p.str;
        }
        Da_append(&n->params, p);
        return n->params.count - 1;
}

Li_Value
li_get_param(Li_Node *n, int param)
{
        if (param < 0 || param >= n->params.count) return (Li_Value) { .type = LI_NONE };
        return n->params.items[param].value;
}

/* Variables, by name. Modules can't share globals (each square has its own
 * copy), so they live here. Like in C, they are local to the canvas they
 * are in: a subgraph square has its own (see Subgraphs), the canvas of a
 * tab its own. Globals (li_global_*) are shared by all of them in the tab. */
typedef struct Var {
        char *name;
        Li_Value value; /* for LI_STRING, .s is str */
        char *str;
} Var;

typedef Da(Var) VarList;
static VarList vars;    /* of the canvas running: the tab's, or a subgraph's */
static VarList globals; /* of the tab */

static Var *
var_find(VarList *l, const char *name)
{
        Da_foreach(v, *l)
        {
                if (strcmp(v->name, name) == 0) return v;
        }
        return NULL;
}

static void
var_set(VarList *l, const char *name, Li_Value value)
{
        Var *v = var_find(l, name ? name : "");
        if (!v) {
                Da_append(l, (Var) { .name = strdup(name ? name : "") });
                v = &l->items[l->count - 1];
        }

        /* Strings are copied: the module's one may not live long */
        char *old = v->str;
        v->str    = value.type == LI_STRING ? strdup(value.s ? value.s : "") : NULL;
        v->value  = value;
        if (v->str) v->value.s = v->str;
        free(old);
}

static Li_Value
var_get(VarList *l, const char *name)
{
        Var *v = var_find(l, name ? name : "");
        return v ? v->value : (Li_Value) { .type = LI_NONE };
}

void
li_var_set(const char *name, Li_Value value)
{
        var_set(&vars, name, value);
}

Li_Value
li_var_get(const char *name)
{
        return var_get(&vars, name);
}

void
li_global_set(const char *name, Li_Value value)
{
        var_set(&globals, name, value);
}

Li_Value
li_global_get(const char *name)
{
        return var_get(&globals, name);
}

static void
var_list_clear(VarList *l)
{
        Da_foreach(v, *l)
        {
                free(v->name);
                free(v->str);
        }
        Da_destroy(l);
}

/* The variables of the canvas running, and the globals of the tab */
static void
vars_clear(void)
{
        var_list_clear(&vars);
        var_list_clear(&globals);
}

/* ---- Lists (Da) and trees (bt.h), shared by every square, by name ---- */

/* A value that owns its string */
typedef struct Stored {
        Li_Value value; /* for LI_STRING, .s is str */
        char *str;
} Stored;

static void
stored_set(Stored *st, Li_Value value)
{
        char *old = st->str;
        st->str   = value.type == LI_STRING ? strdup(value.s ? value.s : "") : NULL;
        st->value = value;
        if (st->str) st->value.s = st->str;
        free(old);
}

static void
stored_free(Stored *st)
{
        free(st->str);
        *st = (Stored) { 0 };
}

typedef struct List {
        Da(Stored) items;
        Stored popped; /* what li_list_pop returned, alive until the next pop */
} List;

/* String keys to Stored *, in order */
typedef struct Tree {
        BT map;
        long count;
} Tree;

/* Roots on the heap, not here: nodes point to their parent, and tabs move
 * these pointers around (see Tabs) */
static BT *lists; /* name -> List * */
static BT *trees; /* name -> Tree * */

/* *ROOT, made if it isn't yet */
static BT *
bt_root(BT **root)
{
        if (!*root) *root = calloc(1, sizeof **root);
        return *root;
}

static List *
list_find(const char *name, bool create)
{
        List *l = bt_get(bt_root(&lists), name ? name : "");
        if (!l && create) {
                l = calloc(1, sizeof *l);
                bt_add(bt_root(&lists), name ? name : "", l);
        }
        return l;
}

/* Negative indices count from the end. -1 if out of range. */
static long
list_index(List *l, long i)
{
        if (!l) return -1;
        if (i < 0) i += l->items.count;
        return i >= 0 && i < l->items.count ? i : -1;
}

long
li_list_len(const char *list)
{
        List *l = list_find(list, false);
        return l ? l->items.count : 0;
}

void
li_list_push(const char *list, Li_Value value)
{
        List *l = list_find(list, true);
        Da_append(&l->items, (Stored) { 0 });
        stored_set(&l->items.items[l->items.count - 1], value);
}

Li_Value
li_list_pop(const char *list)
{
        List *l = list_find(list, false);
        if (!l || !l->items.count) return (Li_Value) { .type = LI_NONE };

        /* The popped item keeps its string until the next pop */
        stored_free(&l->popped);
        l->popped = l->items.items[--l->items.count];
        return l->popped.value;
}

Li_Value
li_list_get(const char *list, long index)
{
        List *l = list_find(list, false);
        long i  = list_index(l, index);
        return i < 0 ? (Li_Value) { .type = LI_NONE } : l->items.items[i].value;
}

bool
li_list_set(const char *list, long index, Li_Value value)
{
        List *l = list_find(list, false);
        long i  = list_index(l, index);
        if (i < 0) return false;
        stored_set(&l->items.items[i], value);
        return true;
}

bool
li_list_remove(const char *list, long index)
{
        List *l = list_find(list, false);
        long i  = list_index(l, index);
        if (i < 0) return false;
        stored_free(&l->items.items[i]);
        Da_remove(&l->items, i);
        return true;
}

void
li_list_clear(const char *list)
{
        List *l = list_find(list, false);
        if (!l) return;
        Da_foreach(st, l->items)
        {
                stored_free(st);
        }
        l->items.count = 0;
}

static Tree *
tree_find(const char *name, bool create)
{
        Tree *t = bt_get(bt_root(&trees), name ? name : "");
        if (!t && create) {
                t = calloc(1, sizeof *t);
                bt_add(bt_root(&trees), name ? name : "", t);
        }
        return t;
}

void
li_tree_set(const char *tree, const char *key, Li_Value value)
{
        Tree *t    = tree_find(tree, true);
        Stored *st = bt_get(&t->map, key ? key : "");
        if (!st) {
                st = calloc(1, sizeof *st);
                bt_add(&t->map, key ? key : "", st);
                t->count++;
        }
        stored_set(st, value);
}

Li_Value
li_tree_get(const char *tree, const char *key)
{
        Tree *t    = tree_find(tree, false);
        Stored *st = t ? bt_get(&t->map, key ? key : "") : NULL;
        return st ? st->value : (Li_Value) { .type = LI_NONE };
}

bool
li_tree_has(const char *tree, const char *key)
{
        Tree *t = tree_find(tree, false);
        return t && bt_get(&t->map, key ? key : "");
}

void
li_tree_del(const char *tree, const char *key)
{
        Tree *t    = tree_find(tree, false);
        Stored *st = t ? bt_get(&t->map, key ? key : "") : NULL;
        if (!st) return;
        stored_free(st);
        free(st);
        bt_del(&t->map, key ? key : "");
        t->count--;
}

long
li_tree_len(const char *tree)
{
        Tree *t = tree_find(tree, false);
        return t ? t->count : 0;
}

void
li_tree_keys(const char *tree, const char *list)
{
        Tree *t = tree_find(tree, false);
        BT *it;

        li_list_clear(list);
        if (!t) return;
        for_bt_each(it, &t->map)
        {
                li_list_push(list, li_string(it->key));
        }
}

static void
containers_clear(void)
{
        BT *it, *e;

        for_bt_each(it, bt_root(&lists))
        {
                List *l = it->value;
                Da_foreach(st, l->items)
                {
                        stored_free(st);
                }
                Da_destroy(&l->items);
                stored_free(&l->popped);
                free(l);
        }
        bt_destroy(lists);
        free(lists);
        lists = NULL;

        for_bt_each(it, bt_root(&trees))
        {
                Tree *t = it->value;
                for_bt_each(e, &t->map)
                {
                        stored_free(e->value);
                        free(e->value);
                }
                bt_destroy(&t->map);
                free(t);
        }
        bt_destroy(trees);
        free(trees);
        trees = NULL;
}

int
li_format(Li_Value v, char *buf, int size)
{
        switch (v.type) {
        case LI_BOOL: return snprintf(buf, size, "%s", v.b ? "true" : "false");
        case LI_INT: return snprintf(buf, size, "%ld", v.i);
        case LI_FLOAT: return snprintf(buf, size, "%g", v.f);
        case LI_STRING: return snprintf(buf, size, "%s", v.s ? v.s : "");
        case LI_PTR: return snprintf(buf, size, "%p", v.p);
        case LI_TICK: return snprintf(buf, size, "tick");
        default: return snprintf(buf, size, "-");
        }
}

/* ---- Typed values: params and input fields ---- */

/* V as text. EXACT keeps every digit of floats, for saving. */
static void
value_text(Li_Value v, char *buf, int size, bool exact)
{
        if (v.type == LI_FLOAT && exact) {
                /* The shortest that reads back the same: 0.1, not 0.10000000000000001 */
                for (int digits = 6; digits <= 17; digits++) {
                        snprintf(buf, size, "%.*g", digits, v.f);
                        if (strtod(buf, NULL) == v.f) break;
                }
        } else if (v.type == LI_NONE) {
                *buf = '\0';
        } else {
                li_format(v, buf, size);
        }
}

/* TEXT as TYPE in OUT (strings are not copied). False if it isn't one. */
static bool
value_parse_as(Li_Type type, const char *text, Li_Value *out)
{
        char *end;

        errno = 0;
        switch (type) {
        case LI_INT: {
                long v = strtol(text, &end, 0);
                if (end == text || *end || errno) return false;
                *out = li_int(v);
                return true;
        }
        case LI_FLOAT: {
                double v = strtod(text, &end);
                if (end == text || *end || errno) return false;
                *out = li_float(v);
                return true;
        }
        case LI_BOOL:
                if (!strcasecmp(text, "true") || !strcmp(text, "1") || !strcasecmp(text, "yes"))
                        *out = li_bool(true);
                else if (!strcasecmp(text, "false") || !strcmp(text, "0") || !strcasecmp(text, "no"))
                        *out = li_bool(false);
                else
                        return false;
                return true;
        case LI_STRING: *out = li_string(text); return true;
        case LI_NONE: *out = (Li_Value) { .type = LI_NONE }; return true;
        case LI_ANY:
                /* Whatever it looks like: 2 is an int, 2.5 a float, true a
                 * bool, nothing is no value, the rest a string */
                if (!*text) return value_parse_as(LI_NONE, text, out);
                if (value_parse_as(LI_INT, text, out) || value_parse_as(LI_FLOAT, text, out)) return true;
                if (!strcmp(text, "true") || !strcmp(text, "false")) return value_parse_as(LI_BOOL, text, out);
                return value_parse_as(LI_STRING, text, out);
        default: return false;
        }
}

/* ---- Params ---- */

static void
param_text(Param *p, char *buf, int size, bool exact)
{
        value_text(p->value, buf, size, exact);
}

/* Parse TEXT as the type of param I of S and tell the module. Returns false
 * (and changes nothing) if it isn't valid. */
static bool
param_set_text(Square *s, int i, const char *text)
{
        Param *p = &s->params.items[i];
        Li_Value v;

        if (!value_parse_as(p->value.type, text, &v)) return false;
        if (v.type == LI_STRING) {
                free(p->str);
                p->str = strdup(v.s);
                v.s    = p->str;
        }
        p->value = v;

        if (s->fn.param) s->fn.param(s, i);
        return true;
}

/* ---- Input fields ---- */

/* Text shown in the field of input I */
static void
input_text(Square *s, int i, char *buf, int size, bool exact)
{
        PinInfo *p = &s->inputs.items[i];
        if (p->kind == PIN_OPTIONS)
                snprintf(buf, size, "%s", li_option(s, i));
        else
                value_text(p->value, buf, size, exact);
}

/* Set input I from TEXT: an option name, or a value of its type (any type
 * for LI_ANY inputs, TYPE if not LI_NONE). False if it isn't valid. */
static bool
input_set_text(Square *s, int i, const char *text, Li_Type type)
{
        PinInfo *p = &s->inputs.items[i];
        Li_Value v;

        if (p->kind == PIN_OPTIONS) {
                for (int k = 0; k < p->option_count; k++) {
                        if (strcmp(p->options[k], text) == 0) {
                                p->value = li_int(k);
                                return true;
                        }
                }
                return false;
        }
        if (p->kind != PIN_VALUE) return false;
        if (type == LI_NONE) type = p->type;
        /* Typed strings are fine for string inputs, but an empty int is not */
        if (!value_parse_as(type, text, &v) || (v.type == LI_NONE && p->type != LI_ANY)) return false;
        pin_store(p, v);
        return true;
}

static int
input_find(Square *s, const char *name)
{
        for (int i = 0; i < s->inputs.count; i++) {
                if (s->inputs.items[i].kind != PIN_WIRE && strcmp(s->inputs.items[i].name, name) == 0) return i;
        }
        return -1;
}

/* ---- Typing text ---- */

typedef struct TextInput {
        char buf[256];
        int len;
} TextInput;

typedef enum TextResult {
        TEXT_TYPING,
        TEXT_DONE,   /* Enter */
        TEXT_CANCEL, /* Esc */
        TEXT_NEXT,   /* Tab */
} TextResult;

static void
text_input_set(TextInput *t, const char *text)
{
        t->len = snprintf(t->buf, sizeof t->buf, "%s", text);
        if (t->len >= (int) sizeof t->buf) t->len = sizeof t->buf - 1;
}

/* Keys typed this frame. CHANGED is set when the text changes. */
static TextResult
text_input_update(TextInput *t, bool *changed)
{
        int c;

        /* The font only has ASCII. Ctrl+key is a shortcut, not a letter (but
         * AltGr can come as Ctrl+Alt, and that one types). */
        bool ctrl = (IsKeyDown(KEY_LEFT_CONTROL) || IsKeyDown(KEY_RIGHT_CONTROL)) &&
                    !IsKeyDown(KEY_LEFT_ALT) && !IsKeyDown(KEY_RIGHT_ALT);
        while ((c = GetCharPressed())) {
                if (!ctrl && c >= ' ' && c < 127 && t->len < (int) sizeof t->buf - 1) {
                        t->buf[t->len++] = c;
                        t->buf[t->len]   = '\0';
                        *changed         = true;
                }
        }
        if ((IsKeyPressed(KEY_BACKSPACE) || IsKeyPressedRepeat(KEY_BACKSPACE)) && t->len > 0) {
                t->buf[--t->len] = '\0';
                *changed         = true;
        }

        if (IsKeyPressed(KEY_ENTER) || IsKeyPressed(KEY_KP_ENTER)) return TEXT_DONE;
        if (IsKeyPressed(KEY_TAB)) return TEXT_NEXT;
        if (IsKeyPressed(KEY_ESCAPE)) return TEXT_CANCEL;
        return TEXT_TYPING;
}

static TextInput menu_search_text;

static TextInput *
menu_search(void)
{
        return &menu_search_text;
}

/* Input field being typed in the square */
static struct {
        Square *square; /* NULL if none */
        int input;
        TextInput text;
        bool error; /* what is typed is not valid */
} field_edit;

static void
field_edit_start(Square *s, int i)
{
        char buf[256];
        input_text(s, i, buf, sizeof buf, false);
        text_input_set(&field_edit.text, buf);
        field_edit.square = s;
        field_edit.input  = i;
        field_edit.error  = false;
}

/* Apply what is typed. On error it stays open. */
static bool
field_edit_apply(void)
{
        if (!field_edit.square) return true;
        if (!input_set_text(field_edit.square, field_edit.input, field_edit.text.buf, LI_NONE)) {
                field_edit.error = true;
                return false;
        }
        field_edit.square = NULL;
        return true;
}

/* Returns true if the field consumed the input this frame. */
static bool
update_field(void)
{
        bool changed = false;

        if (!field_edit.square) return false;

        switch (text_input_update(&field_edit.text, &changed)) {
        case TEXT_DONE:
        case TEXT_NEXT: field_edit_apply(); break;
        case TEXT_CANCEL: field_edit.square = NULL; return true;
        case TEXT_TYPING: break;
        }
        if (changed) field_edit.error = false;

        if (IsMouseButtonPressed(MOUSE_BUTTON_LEFT) || IsMouseButtonPressed(MOUSE_BUTTON_RIGHT)) {
                Vector2 world = GetScreenToWorld2D(GetMousePosition(), camera);
                if (field_edit.square && CheckCollisionPointRec(world, field_rect(field_edit.square, field_edit.input)))
                        return true;
                /* Clicking away applies what's valid, the click goes on */
                if (!field_edit_apply()) field_edit.square = NULL;
        }
        return false;
}

/* The list of an options input, opened by clicking its field */
static Menu options_menu;
static int options_input;

static void
option_choose(int id, void *arg)
{
        Square *s  = square_get(id);
        PinInfo *p = s ? pin_info(s, PART_INPUT, options_input) : NULL;
        if (p && p->kind == PIN_OPTIONS) p->value = li_int((int) (intptr_t) arg);
}

static int
param_find(Square *s, const char *name)
{
        for (int i = 0; i < s->params.count; i++) {
                if (strcmp(s->params.items[i].name, name) == 0) return i;
        }
        return -1;
}

/* Settings panel of a square, next to it */
static struct {
        Square *square; /* NULL when closed */
        int field;      /* param being typed, -1 if none */
        TextInput text;
        bool error; /* what was typed is not valid for the param */
} panel = { .field = -1 };

static void
panel_open(Square *s)
{
        panel.square = s;
        panel.field  = -1;
        panel.error  = false;
}

static void
panel_close(void)
{
        panel.square = NULL;
        panel.field  = -1;
}

/* ---- Squares ---- */

/* dlopen PATH and look up the callbacks. NULL if it fails or has no
 * module_init. */
static void *
module_open(const char *path, ModuleFns *fn)
{
        void *handle = dlopen(path, RTLD_NOW | RTLD_LOCAL);
        if (!handle) {
                fprintf(stderr, "modules: %s\n", dlerror());
                return NULL;
        }

        fn->init    = (void (*)(Li_Node *)) dlsym(handle, "module_init");
        fn->tick    = (void (*)(Li_Node *, int)) dlsym(handle, "module_tick");
        fn->eval    = (void (*)(Li_Node *)) dlsym(handle, "module_eval");
        fn->frame   = (void (*)(Li_Node *)) dlsym(handle, "module_frame");
        fn->destroy = (void (*)(Li_Node *)) dlsym(handle, "module_destroy");
        fn->param   = (void (*)(Li_Node *, int)) dlsym(handle, "module_param");
        fn->save    = (const char *(*)(Li_Node *) ) dlsym(handle, "module_save");
        fn->load    = (void (*)(Li_Node *, const char *)) dlsym(handle, "module_load");
        fn->draw    = (void (*)(Li_Node *)) dlsym(handle, "module_draw");
        fn->start   = (void (*)(Li_Node *)) dlsym(handle, "module_start");
        if (!fn->init) {
                fprintf(stderr, "modules: %s has no module_init\n", path);
                dlclose(handle);
                return NULL;
        }
        return handle;
}

/* Where the copies of the modules go, created on first use */
static char tmp_dir[PATH_MAX];

static bool
copy_file(const char *from, const char *to)
{
        char buf[1 << 16];
        size_t n;
        bool ok   = false;
        FILE *in  = fopen(from, "rb");
        FILE *out = in ? fopen(to, "wb") : NULL;

        if (in && out) {
                ok = true;
                while (ok && (n = fread(buf, 1, sizeof buf, in)) > 0)
                        ok = fwrite(buf, 1, n, out) == n;
                ok = ok && !ferror(in);
        }
        if (out && fclose(out) != 0) ok = false;
        if (in) fclose(in);
        return ok;
}

/* Open a private copy of M for square S. dlopen on the same file again only
 * bumps a reference count, so every square would share the module's globals.
 * A copy is a different file, so it gets its own. */
static bool
tmp_dir_make(void)
{
        if (*tmp_dir) return true;
        const char *tmp = getenv("TMPDIR");
        snprintf(tmp_dir, sizeof tmp_dir, "%s/lineit-XXXXXX", tmp && *tmp ? tmp : "/tmp");
        if (!mkdtemp(tmp_dir)) {
                perror("lineit: mkdtemp");
                *tmp_dir = '\0';
                return false;
        }
        return true;
}

static bool
square_open(Square *s, Module *m)
{
        char path[PATH_MAX];

        if (m->builtin) {
                s->fn = *m->builtin;
                return true;
        }
        if (!tmp_dir_make()) return false;

        snprintf(path, sizeof path, "%s/%d-%s.so", tmp_dir, s->id, m->label);
        if (!copy_file(m->lib ? m->lib : m->path, path)) {
                fprintf(stderr, "modules: could not copy %s to %s\n", m->lib ? m->lib : m->path, path);
                remove(path);
                return false;
        }
        s->handle = module_open(path, &s->fn);
        /* Once open it's mapped in memory, the file is not needed */
        remove(path);
        return s->handle != NULL;
}

/* A new square of M with id ID (a new one if ID < 0), centered at CENTER */
static Square *
square_new_id(Module *m, Vector2 center, int id)
{
        Square *s = calloc(1, sizeof *s);
        s->id     = id >= 0 ? id : next_id++;
        s->module = m;
        if (s->id >= next_id) next_id = s->id + 1;
        if (!square_open(s, m)) {
                free(s);
                return NULL;
        }
        s->fn.init(s);
        /* Let the module apply the defaults the same way it applies changes */
        for (int i = 0; s->fn.param && i < s->params.count; i++)
                s->fn.param(s, i);

        s->pos = Vector2SubtractValue(center, square_side(s) / 2);
        Da_append(&squares, s);
        last_module = m;
        return s;
}

static Square *
square_new(Module *m, Vector2 center)
{
        return square_new_id(m, center, -1);
}

static void
pins_free(PinList *l)
{
        Da_foreach(p, *l)
        {
                free(p->name);
                free(p->str);
                for (int i = 0; i < p->option_count; i++)
                        free(p->options[i]);
                free(p->options);
        }
        Da_destroy(l);
}

static void
square_free(Square *s)
{
        if (panel.square == s) panel_close();
        if (field_edit.square == s) field_edit.square = NULL;
        if (s->fn.destroy) s->fn.destroy(s);
        pins_free(&s->inputs);
        pins_free(&s->outputs);
        Da_foreach(p, s->params)
        {
                free(p->name);
                free(p->str);
        }
        Da_destroy(&s->params);
        if (s->handle) dlclose(s->handle);
        free(s);
}

static void
square_disconnect_inputs(int id, void *arg)
{
        (void) arg;
        for (int i = links.count - 1; i >= 0; i--) {
                if (links.items[i].to == id) Da_remove(&links, i);
        }
}

static void
square_disconnect_outputs(int id, void *arg)
{
        (void) arg;
        for (int i = links.count - 1; i >= 0; i--) {
                if (links.items[i].from == id) Da_remove(&links, i);
        }
}

static void
square_disconnect_all(int id, void *arg)
{
        square_disconnect_inputs(id, arg);
        square_disconnect_outputs(id, arg);
}

static void
square_configure(int id, void *arg)
{
        (void) arg;
        Square *s = square_get(id);
        if (s) panel_open(s);
}

static void
square_delete(int id, void *arg)
{
        Square *s = square_get(id);
        if (!s) return;
        square_disconnect_all(id, arg);
        Da_remove(&squares, square_index(s));
        square_free(s);
}

static void
canvas_add_module(int id, void *arg)
{
        (void) id;
        square_new(arg, menu.world);
}

static void
canvas_reset_view(int id, void *arg)
{
        (void) id, (void) arg;
        camera = (Camera2D) { .zoom = 1.0f };
}

static void
canvas_clear(int id, void *arg)
{
        (void) id, (void) arg;
        Da_foreach(s, squares)
        {
                square_free(*s);
        }
        squares.count = 0;
        links.count   = 0;
        vars_clear();
        containers_clear();
}

static void each_selected(void (*fn)(int id, void *arg));
static void selection_duplicate(void);

/* The square menu acts on the selection (the right clicked square is
 * always in it) */
static void
menu_duplicate(int id, void *arg)
{
        (void) id, (void) arg;
        selection_duplicate();
}

static void
menu_disconnect_inputs(int id, void *arg)
{
        (void) id, (void) arg;
        each_selected(square_disconnect_inputs);
}

static void
menu_disconnect_outputs(int id, void *arg)
{
        (void) id, (void) arg;
        each_selected(square_disconnect_outputs);
}

static void
menu_disconnect_all(int id, void *arg)
{
        (void) id, (void) arg;
        each_selected(square_disconnect_all);
}

static void
menu_delete(int id, void *arg)
{
        (void) id, (void) arg;
        each_selected(square_delete);
}

/* Options shown on right click over a square. They act on every selected
 * square. Add new entries here. */
static const MenuItem square_items[] = {
        { .label = "Duplicate", .fn = menu_duplicate },
        { .label = "Disconnect inputs", .fn = menu_disconnect_inputs },
        { .label = "Disconnect outputs", .fn = menu_disconnect_outputs },
        { .label = "Disconnect all", .fn = menu_disconnect_all },
        { .label = "Delete", .fn = menu_delete },
};
static Menu square_menu; /* rebuilt each time it opens */

static void
square_menu_build(Square *s)
{
        square_menu.count = 0;
        if (s->params.count) Da_append(&square_menu, (MenuItem) { .label = "Configure", .fn = square_configure });
        if (s->module->builtin == &graph_fns) Da_append(&square_menu, (MenuItem) { .label = "Open in a new tab", .fn = square_open_tab });
        Da_append(&square_menu, (MenuItem) { .label = "Make subgraph...", .hint = "Ctrl+G", .fn = menu_subgraph });
        for (int i = 0; i < countof(square_items); i++)
                Da_append(&square_menu, square_items[i]);
}

/* Options shown on right click over the empty canvas. Add new entries here. */
static void
canvas_menu_build(void)
{
        static char add_last[128];

        canvas_menu.count = 0;
        if (last_module) {
                snprintf(add_last, sizeof add_last, "Add %s", last_module->label);
                Da_append(&canvas_menu, (MenuItem) { .label = add_last, .fn = canvas_add_module, .arg = last_module });
        }
        if (module_menu.count) Da_append(&canvas_menu, (MenuItem) { .label = "Add module", .sub = &module_menu });
        Da_append(&canvas_menu, (MenuItem) { .label = "Reset view", .fn = canvas_reset_view });
        Da_append(&canvas_menu, (MenuItem) { .label = "Clear all", .fn = canvas_clear });
}

/* ---- Modules ---- */

/* Entry LABEL of M that opens a submenu, created if needed */
static Menu *
menu_submenu(Menu *m, const char *label)
{
        Da_foreach(it, *m)
        {
                if (it->sub && strcmp(it->label, label) == 0) return it->sub;
        }
        Menu *sub = calloc(1, sizeof *sub);
        Da_append(m, (MenuItem) { .label = strdup(label), .sub = sub });
        return sub;
}

/* Free the submenus and labels menu_submenu created */
static void
menu_free(Menu *m)
{
        Da_foreach(it, *m)
        {
                if (!it->sub) continue;
                menu_free(it->sub);
                free(it->sub);
                free((char *) it->label);
        }
        Da_destroy(m);
}

static int
module_cmp(const void *a, const void *b)
{
        return strcmp((*(Module **) a)->name, (*(Module **) b)->name);
}

/* Modification time of PATH in ns, -1 if it isn't there */
static long long
file_mtime(const char *path)
{
        struct stat st;
        if (stat(path, &st) != 0) return -1;
        return st.st_mtim.tv_sec * 1000000000LL + st.st_mtim.tv_nsec;
}

static long long
now_ns(void)
{
        struct timespec t;
        clock_gettime(CLOCK_REALTIME, &t);
        return t.tv_sec * 1000000000LL + t.tv_nsec;
}

/* ---- Packages: modules in a .li ---- */

/* A .li is a zip with a module built for one or more platforms, its source
 * and its license (tools/lineit-pack makes them, see its --howto):
 *
 *   manifest.lua   name, version, license, api (LI_API_VERSION) and
 *                  platforms: platform -> library inside, like
 *                  ["linux-x86_64"] = "lib/linux-x86_64.so"
 *   lib/...        the libraries
 *   LICENSE, src/  for people, not read here
 *
 * The library for this platform is taken out to tmp_dir once, and squares
 * copy it from there like any other .so. A package without this platform
 * is skipped. */

#if defined(__linux__)
#define LI_OS "linux"
#elif defined(__APPLE__)
#define LI_OS "macos"
#elif defined(_WIN32)
#define LI_OS "windows"
#else
#define LI_OS "unknown"
#endif
#if defined(__x86_64__) || defined(_M_X64)
#define LI_ARCH "x86_64"
#elif defined(__aarch64__) || defined(_M_ARM64)
#define LI_ARCH "arm64"
#else
#define LI_ARCH "unknown"
#endif
#define LI_PLATFORM LI_OS "-" LI_ARCH

static unsigned
le16(const unsigned char *p)
{
        return p[0] | p[1] << 8;
}

static unsigned long
le32(const unsigned char *p)
{
        return p[0] | p[1] << 8 | (unsigned long) p[2] << 16 | (unsigned long) p[3] << 24;
}

/* Member NAME of the zip ZIP (SIZE bytes), in a new buffer of *LEN bytes.
 * NULL if it isn't there or is broken. Stored and deflated members, no
 * zip64: modules are small. */
static unsigned char *
zip_member(const unsigned char *zip, size_t size, const char *name, size_t *len)
{
        size_t eocd = 0;
        bool found  = false;

        /* The end record is last, maybe after a comment of up to 64K */
        for (size_t i = size >= 22 ? size - 22 : 0; size >= 22 && !found; i--) {
                if (le32(zip + i) == 0x06054b50) eocd = i, found = true;
                if (i == 0 || size - i > 22 + 0xffff) break;
        }
        if (!found) return NULL;

        size_t at  = le32(zip + eocd + 16);
        unsigned n = le16(zip + eocd + 10);
        for (unsigned e = 0; e < n; e++) {
                if (at + 46 > size || le32(zip + at) != 0x02014b50) return NULL;
                unsigned method  = le16(zip + at + 10);
                unsigned long crc = le32(zip + at + 16);
                size_t csize     = le32(zip + at + 20);
                size_t usize     = le32(zip + at + 24);
                unsigned nlen    = le16(zip + at + 28);
                size_t local     = le32(zip + at + 42);
                bool match       = at + 46 + nlen <= size && nlen == strlen(name) && !memcmp(zip + at + 46, name, nlen);
                at += 46 + nlen + le16(zip + at + 30) + le16(zip + at + 32);
                if (!match) continue;

                if (local + 30 > size || le32(zip + local) != 0x04034b50) return NULL;
                size_t data = local + 30 + le16(zip + local + 26) + le16(zip + local + 28);
                if (data + csize > size) return NULL;

                unsigned char *out = malloc(usize ? usize : 1);
                bool ok            = false;
                if (method == 0 && csize == usize) {
                        memcpy(out, zip + data, usize);
                        ok = true;
                } else if (method == 8) {
                        z_stream z = { .next_in = (unsigned char *) zip + data, .avail_in = csize, .next_out = out, .avail_out = usize };
                        ok         = inflateInit2(&z, -MAX_WBITS) == Z_OK && inflate(&z, Z_FINISH) == Z_STREAM_END && z.total_out == usize;
                        inflateEnd(&z);
                }
                if (!ok || crc32(0, out, usize) != crc) {
                        free(out);
                        return NULL;
                }
                *len = usize;
                return out;
        }
        return NULL;
}

/* Take the library for this platform out of the package at PATH. Returns
 * where it went (to free and remove), or NULL with the reason in WHY. */
static char *
package_lib(const char *path, char *why, int why_size)
{
        static int n;
        int size              = 0;
        unsigned char *zip    = LoadFileData(path, &size);
        size_t len            = 0;
        unsigned char *member = NULL;
        char tmp[PATH_MAX], member_name[PATH_MAX] = "";
        char *lib = NULL;
        const char *str;
        Conf conf;
        int api = -1;

        if (!zip || !tmp_dir_make()) {
                snprintf(why, why_size, "can't read it");
                goto done;
        }
        if (!(member = zip_member(zip, size, "manifest.lua", &len))) {
                snprintf(why, why_size, "not a package: no manifest.lua");
                goto done;
        }
        /* conf.h only reads files */
        snprintf(tmp, sizeof tmp, "%s/pkg-%d-manifest.lua", tmp_dir, n);
        bool saved = SaveFileData(tmp, member, len);
        free(member), member = NULL;
        if (!saved || Conf_open(&conf, tmp) != CONF_OK) {
                remove(tmp);
                snprintf(why, why_size, "broken manifest.lua");
                goto done;
        }
        remove(tmp);
        Conf_get_int(conf, &api, "api");
        if (Conf_get_str(conf, &str, "platforms.%s", LI_PLATFORM) == CONF_OK) snprintf(member_name, sizeof member_name, "%s", str);
        Conf_close(conf);

        if (api != LI_API_VERSION) {
                snprintf(why, why_size, "made for module API %d, this lineit has %d", api, LI_API_VERSION);
                goto done;
        }
        if (!*member_name) {
                snprintf(why, why_size, "not built for " LI_PLATFORM);
                goto done;
        }
        if (!(member = zip_member(zip, size, member_name, &len))) {
                snprintf(why, why_size, "%s missing or broken in it", member_name);
                goto done;
        }
        snprintf(tmp, sizeof tmp, "%s/pkg-%d.so", tmp_dir, n++);
        if (!SaveFileData(tmp, member, len)) {
                remove(tmp);
                snprintf(why, why_size, "can't write %s", tmp);
                goto done;
        }
        lib = strdup(tmp);
done:
        free(member);
        UnloadFileData(zip);
        return lib;
}

/* Whether the .so at PATH loads and has module_init. From a copy, like the
 * squares, so dlopen doesn't hand back one it has already loaded. */
static bool
module_check(const char *path)
{
        static int n;
        char copy[PATH_MAX];
        ModuleFns fn;

        if (!tmp_dir_make()) return false;
        snprintf(copy, sizeof copy, "%s/check-%d.so", tmp_dir, n++);
        bool ok = copy_file(path, copy);
        void *h = ok ? module_open(copy, &fn) : NULL;
        remove(copy);
        if (h) dlclose(h);
        return h != NULL;
}

/* module_menu with the folders of the modules that are there */
static void
module_menu_build(void)
{
        menu_free(&module_menu);
        Da_foreach(mp, modules)
        {
                Module *mod = *mp;
                Menu *m     = &module_menu;
                char *name  = strdup(mod->name);
                char *save, *next;
                char *part = strtok_r(name, "/", &save);

                if (mod->gone) {
                        free(name);
                        continue;
                }
                while ((next = strtok_r(NULL, "/", &save))) {
                        m    = menu_submenu(m, part);
                        part = next;
                }
                Da_append(m, (MenuItem) { .label = mod->label, .fn = canvas_add_module, .arg = mod });
                free(name);
        }
}

static void set_status(const char *fmt, ...);
static Module *module_find(const char *name);
static void module_reload_squares(Module *m);
static const ModuleFns graph_fns;
static void graph_builtins(void);
static bool square_uses(Square *s, Module *m);

/* Where load_modules found them */
static const char *modules_dir;

/* A .so is only used once it has been left alone this long: make may be
 * writing it */
#define MODULE_SETTLE_NS 300000000LL

/* Look at every .so under modules_dir. The first time, load them all. Later
 * (RELOAD), add new ones, swap rebuilt ones in their squares, and take
 * removed ones out of the menu. */
static void
modules_scan(bool reload)
{
        bool menu_changed = !reload;
        long long now     = now_ns();

        if (!DirectoryExists(modules_dir)) {
                if (!reload) fprintf(stderr, "modules: %s not found\n", modules_dir);
                return;
        }

        Da_foreach(mp, modules)

        {
                (*mp)->seen = !(*mp)->path; /* built-in, not in modules_dir */
        }
        FilePathList files = LoadDirectoryFilesEx(modules_dir, ".so;.lua;.li", true);
        for (unsigned i = 0; i < files.count; i++) {
                const char *path = files.paths[i];
                long long mtime  = file_mtime(path);
                bool settled     = !reload || now - mtime > MODULE_SETTLE_NS;

                /* "build/modules/math/add.so" -> "math/add" */
                const char *rel = path + strlen(modules_dir);
                while (*rel == '/')
                        rel++;
                char *name          = strdup(rel);
                *strrchr(name, '.') = '\0';
                Module *m           = module_find(name);
                bool graph          = IsFileExtension(path, ".lua");
                bool package        = IsFileExtension(path, ".li");
                char *lib           = NULL; /* of the package */
                char why[256]       = "doesn't load";

                if (m) {
                        m->seen = true;
                        if (m->gone) {
                                m->gone      = false;
                                menu_changed = true;
                        }
                        free(name);
                        if (mtime == m->mtime || !settled) continue;
                        m->mtime = mtime; /* a broken build is tried again when it changes */
                        if (package && !(lib = package_lib(path, why, sizeof why))) {
                                set_status("Module %s: %s, keeping the old one", m->name, why);
                                continue;
                        }
                        if (!graph && !module_check(lib ? lib : path)) {
                                set_status("Module %s doesn't load, keeping the old one", m->name);
                                if (lib) remove(lib);
                                free(lib);
                                continue;
                        }
                        if (lib) {
                                if (m->lib) remove(m->lib);
                                free(m->lib);
                                m->lib = lib;
                        }
                        module_reload_squares(m);
                        set_status("Reloaded %s", m->name);
                        continue;
                }

                if (!settled) {
                        free(name);
                        continue;
                }
                if (package) lib = package_lib(path, why, sizeof why);
                if ((package && !lib) || (!graph && !module_check(lib ? lib : path))) {
                        fprintf(stderr, "modules: %s skipped: %s\n", path, why);
                        if (reload) set_status("Module %s: %s", name, why);
                        if (lib) remove(lib);
                        free(lib);
                        free(name);
                        continue;
                }
                m          = calloc(1, sizeof *m);
                m->lib     = lib;
                m->path    = strdup(path);
                m->builtin = graph ? &graph_fns : NULL;
                m->name    = name;
                m->label = strrchr(m->name, '/') ? strrchr(m->name, '/') + 1 : m->name;
                m->mtime = mtime;
                m->seen  = true;
                Da_append(&modules, m);
                menu_changed = true;
                if (reload) set_status("Added module %s", m->name);
        }
        UnloadDirectoryFiles(files);

        /* Removed: their squares keep their copies, so they still work */
        Da_foreach(mp, modules)
        {
                if (!(*mp)->seen && !(*mp)->gone) {
                        (*mp)->gone  = true;
                        menu_changed = true;
                        set_status("Removed module %s", (*mp)->name);
                }
        }

        if (!menu_changed) return;
        qsort(modules.items, modules.count, sizeof *modules.items, module_cmp);
        module_menu_build();
        if (!last_module || last_module->gone) {
                last_module = NULL;
                Da_foreach(mp, modules)
                {
                        if (!last_module && !(*mp)->gone) last_module = *mp;
                }
        }
}

/* Load every module under DIR (.so, .lua subgraphs and .li packages), and build module_menu with the same folders */
static void
load_modules(const char *dir)
{
        modules_dir = dir;
        graph_builtins();
        modules_scan(false);
}

static void
unload_modules(void)
{
        menu_free(&module_menu);
        Da_destroy(&canvas_menu);
        Da_destroy(&square_menu);
        Da_destroy(&options_menu);
        Da_destroy(&search_menu);
        Da_foreach(mp, modules)
        {
                if ((*mp)->lib) remove((*mp)->lib);
                free((*mp)->lib);
                free((*mp)->path);
                free((*mp)->name);
                free(*mp);
        }
        Da_destroy(&modules);
        last_module = NULL;
        /* The copies are removed as soon as they are open */
        if (*tmp_dir) rmdir(tmp_dir);
}

/* Let squares start tick chains (only while running), and update what
 * they show (always) */
static void
run_frame(void)
{
        for (int i = 0; running && i < squares.count; i++) {
                Square *s = squares.items[i];
                if (s->fn.frame) s->fn.frame(s);
        }
        for (int i = 0; i < squares.count; i++) {
                Square *s = squares.items[i];
                if (s->fn.draw) s->fn.draw(s);
        }
        tabs_run_background();
}

/* ---- Menu ---- */

static void
menu_open(Menu *m, int id, Vector2 pos)
{
        menu.depth      = 1;
        menu.level[0]   = (MenuLevel) { .menu = m, .pos = pos };
        menu.id         = id;
        menu.world      = GetScreenToWorld2D(pos, camera);
        menu.searchable = false;
        menu.selected   = -1;
}

static float
menu_item_height(void)
{
        return cfg.menu.font_size + 2 * cfg.menu.padding;
}

/* Drawn at the right of the entries that open a submenu */
#define MENU_ARROW ">"

static Rectangle
menu_rect(int lvl)
{
        Menu *m      = menu.level[lvl].menu;
        Vector2 pos  = menu.level[lvl].pos;
        float w      = 0;
        bool has_sub = false;

        Da_foreach(it, *m)
        {
                float hint = it->hint ? measure_text(it->hint, cfg.menu.font_size) + 4 * cfg.menu.padding : 0;
                w          = fmaxf(w, measure_text(it->label, cfg.menu.font_size) + hint);
                has_sub    = has_sub || it->sub;
        }
        w += 4 * cfg.menu.padding;
        if (has_sub) w += measure_text(MENU_ARROW, cfg.menu.font_size) + 2 * cfg.menu.padding;

        float h = menu_item_height();

        /* The search menu stays where menu_search_open put it, only as many
         * entries as fit below it are shown, and the rest scroll */
        if (lvl == 0 && menu.searchable) {
                int rows = Clamp((int) ((GetScreenHeight() - pos.y) / h), 1, Max(m->count, 1));
                return (Rectangle) { pos.x, pos.y, menu.search_w, rows * h };
        }

        /* Taller than the window: as many as fit, and the rest scroll */
        int rows    = Clamp((int) (GetScreenHeight() / h), 1, Max(m->count, 1));
        Rectangle r = { pos.x, pos.y, w, rows * h };

        /* Keep it inside the window. Submenus flip to the parent's left. */
        if (r.x + r.width > GetScreenWidth())
                r.x = lvl > 0 ? menu_rect(lvl - 1).x - r.width : GetScreenWidth() - r.width;
        r.y = fminf(r.y, GetScreenHeight() - r.height);
        r.x = fmaxf(r.x, 0);
        r.y = fmaxf(r.y, 0);
        return r;
}

/* Entries of level LVL shown at once */
static int
menu_rows(int lvl)
{
        return (int) roundf(menu_rect(lvl).height / menu_item_height());
}

/* Keep the scroll of LVL in range */
static void
menu_scroll_clamp(int lvl)
{
        MenuLevel *l = &menu.level[lvl];
        l->scroll    = Clamp(l->scroll, 0, Max(l->menu->count - menu_rows(lvl), 0));
}

/* Scroll LVL so entry I is shown */
static void
menu_scroll_to(int lvl, int i)
{
        MenuLevel *l = &menu.level[lvl];
        int rows     = menu_rows(lvl);
        if (i < l->scroll) l->scroll = i;
        if (i >= l->scroll + rows) l->scroll = i - rows + 1;
        menu_scroll_clamp(lvl);
}

/* The canvas menu with its search box, opened at POS. Its size and place
 * are fixed now, wide enough for any module name and with the search box at
 * the top, so typing doesn't move it. */
static void
menu_search_open(Menu *m, Vector2 pos)
{
        float h = menu_item_height();
        float w = 260;

        menu_open(m, -1, pos);
        menu.searchable = true;
        Da_foreach(it, *m)
        {
                w = fmaxf(w, measure_text(it->label, cfg.menu.font_size) + (it->sub ? measure_text(MENU_ARROW, cfg.menu.font_size) + 2 * cfg.menu.padding : 0));
        }
        Da_foreach(mp, modules)
        {
                w = fmaxf(w, measure_text((*mp)->name, cfg.menu.font_size));
        }
        menu.search_w = w + 4 * cfg.menu.padding;

        /* Below the search box, and up if the menu doesn't fit under it,
         * with room for some results too */
        int rows = Clamp((int) ((GetScreenHeight() - h) / h), 1, Max(m->count, 8));
        pos.y += h;
        pos.y = fmaxf(fminf(pos.y, GetScreenHeight() - rows * h), h);
        pos.x = fmaxf(fminf(pos.x, GetScreenWidth() - menu.search_w), 0);
        menu.level[0].pos = pos;
}

/* The search box, above the canvas menu */
static Rectangle
menu_search_rect(void)
{
        Rectangle r = menu_rect(0);
        return (Rectangle) { r.x, r.y - menu_item_height(), r.width, menu_item_height() };
}

/* Position of NEEDLE in HAY ignoring case, -1 if it's not there */
static int
find_icase(const char *hay, const char *needle)
{
        size_t n = strlen(needle);
        for (int i = 0; hay[i]; i++) {
                if (strncasecmp(hay + i, needle, n) == 0) return i;
        }
        return n == 0 ? 0 : -1;
}

/* NEEDLE as a subsequence of HAY, ignoring case: its letters in order, maybe
 * with others between ("l/" is in "list/push" and "logic/and"). Returns the
 * length of the shortest part of HAY that has it, -1 if none does. */
static int
fuzzy_span(const char *hay, const char *needle)
{
        int best = -1;
        if (!*needle) return 0;
        for (int start = 0; hay[start]; start++) {
                if (tolower((unsigned char) hay[start]) != tolower((unsigned char) *needle)) continue;
                const char *q = needle + 1;
                int i         = start + 1;
                for (; *q && hay[i]; i++)
                        if (tolower((unsigned char) hay[i]) == tolower((unsigned char) *q)) q++;
                if (*q) break; /* later starts can't have it either */
                if (best < 0 || i - start < best) best = i - start;
        }
        return best;
}

typedef struct SearchHit {
        int score; /* lower is better */
        Module *module;
} SearchHit;

static int
search_hit_cmp(const void *a, const void *b)
{
        const SearchHit *x = a, *y = b;
        if (x->score != y->score) return x->score - y->score;
        return strcmp(x->module->name, y->module->name);
}

/* search_menu = the modules matching what's typed: that name first, then
 * names starting with it, names with it, paths with it ("math" finds
 * math/add), and last paths with its letters in that order ("l/" finds
 * everything in folders with an l), tighter matches first */
static void
search_menu_build(void)
{
        const char *q      = menu_search()->buf;
        Da(SearchHit) hits = { 0 };

        Da_foreach(mp, modules)
        {
                if ((*mp)->gone) continue;
                int at    = find_icase((*mp)->label, q);
                int span  = fuzzy_span((*mp)->name, q);
                int score = !strcasecmp((*mp)->label, q) ? 0 : /* exactly it */
                            at == 0                         ? 1 :
                            at > 0                          ? 2 :
                            find_icase((*mp)->name, q) >= 0 ? 3 :
                            span >= 0                       ? 4 + span :
                                                              -1;
                if (score >= 0) Da_append(&hits, (SearchHit) { score, *mp });
        }
        if (hits.count) qsort(hits.items, hits.count, sizeof *hits.items, search_hit_cmp);

        search_menu.count = 0;
        Da_foreach(h, hits)
        {
                Da_append(&search_menu, (MenuItem) { .label = h->module->name, .fn = canvas_add_module, .arg = h->module });
        }
        if (!search_menu.count) Da_append(&search_menu, (MenuItem) { .label = "No modules match" });
        Da_destroy(&hits);
}

/* Item under screen point P, deepest menu first, or -1. Its level goes to LVL. */
static int
menu_hit(Vector2 p, int *lvl)
{
        for (int k = menu.depth - 1; k >= 0; k--) {
                Rectangle r = menu_rect(k);
                if (!CheckCollisionPointRec(p, r)) continue;
                *lvl = k;
                return Min(menu.level[k].scroll + (int) ((p.y - r.y) / menu_item_height()), menu.level[k].menu->count - 1);
        }
        return -1;
}

/* Returns true if the menu consumed the input this frame. */
static bool
update_menu(void)
{
        Vector2 mouse = GetMousePosition();

        if (IsMouseButtonPressed(MOUSE_BUTTON_RIGHT)) {
                menu.right_press = mouse;
                menu.depth       = 0;
        }

        /* A right click that did not move (not a pan) opens the menu */
        if (IsMouseButtonReleased(MOUSE_BUTTON_RIGHT) &&
            Vector2Distance(mouse, menu.right_press) < 4.0f &&
            CheckCollisionPointRec(mouse, canvas_rect())) {
                Hit h = square_hit(GetScreenToWorld2D(mouse, camera));
                if (h.square) {
                        /* Right click on a square that isn't selected selects only it */
                        if (!h.square->selected) {
                                select_all(false);
                                h.square->selected = true;
                        }
                        square_menu_build(h.square);
                        menu_open(&square_menu, h.square->id, mouse);
                } else {
                        canvas_menu_build();
                        /* The search box goes where it was clicked */
                        menu_search_open(&canvas_menu, mouse);
                        text_input_set(menu_search(), "");
                }
        }

        if (!menu.depth) return false;

        if (menu.searchable) {
                bool changed  = false;
                TextResult tr = text_input_update(menu_search(), &changed);
                if (changed) {
                        /* Typing shows the modules that match instead */
                        bool any = menu_search()->len > 0;
                        if (any) search_menu_build();
                        menu.level[0].menu   = any ? &search_menu : &canvas_menu;
                        menu.level[0].scroll = 0;
                        menu.depth           = 1;
                        menu.selected        = any ? 0 : -1;
                }

                /* Arrows, or C-n and C-p */
                Menu *m   = menu.level[0].menu;
                bool ctrl = IsKeyDown(KEY_LEFT_CONTROL) || IsKeyDown(KEY_RIGHT_CONTROL);
                bool down = IsKeyPressed(KEY_DOWN) || IsKeyPressedRepeat(KEY_DOWN) ||
                            (ctrl && (IsKeyPressed(KEY_N) || IsKeyPressedRepeat(KEY_N)));
                bool up   = IsKeyPressed(KEY_UP) || IsKeyPressedRepeat(KEY_UP) ||
                          (ctrl && (IsKeyPressed(KEY_P) || IsKeyPressedRepeat(KEY_P)));
                if (down) menu.selected = (menu.selected + 1) % m->count;
                if (up) menu.selected = (menu.selected <= 0 ? m->count : menu.selected) - 1;
                if (down || up) {
                        menu.depth = 1;
                        menu_scroll_to(0, menu.selected);
                }

                /* Esc closes it, and so does Enter with nothing typed */
                if (tr == TEXT_CANCEL || (tr == TEXT_DONE && !menu_search()->len)) {
                        menu.depth = 0;
                        return true;
                }
                if (tr == TEXT_DONE && menu.selected >= 0) {
                        MenuItem *it = &m->items[menu.selected];
                        if (it->fn) {
                                menu.depth = 0;
                                it->fn(menu.id, it->arg);
                        }
                        return true;
                }
                if (IsMouseButtonPressed(MOUSE_BUTTON_LEFT) && CheckCollisionPointRec(mouse, menu_search_rect()))
                        return true;
        } else if (IsKeyPressed(KEY_ESCAPE)) {
                menu.depth = 0;
                return true;
        }

        /* The wheel scrolls the menu under the mouse */
        float wheel = GetMouseWheelMove();
        for (int k = menu.depth - 1; wheel != 0 && k >= 0; k--) {
                if (!CheckCollisionPointRec(mouse, menu_rect(k))) continue;
                menu.level[k].scroll -= (int) (wheel * 3);
                menu_scroll_clamp(k);
                break;
        }

        int lvl      = 0;
        int i        = menu_hit(mouse, &lvl);
        MenuItem *it = i >= 0 ? &menu.level[lvl].menu->items[i] : NULL;

        /* Hovering an entry opens its submenu and closes deeper ones */
        if (it) {
                menu.depth = lvl + 1;
                if (it->sub && menu.depth < MENU_DEPTH) {
                        Rectangle r     = menu_rect(lvl);
                        MenuLevel *next = &menu.level[menu.depth++];
                        /* Still open: keep where it's scrolled to */
                        int scroll = next->menu == it->sub ? next->scroll : 0;
                        *next      = (MenuLevel) {
                                it->sub, { r.x + r.width, r.y + (i - menu.level[lvl].scroll) * menu_item_height() }, scroll
                        };
                        menu_scroll_clamp(menu.depth - 1);
                }
        }

        if (IsMouseButtonPressed(MOUSE_BUTTON_LEFT)) {
                if (it && it->sub) return true; /* it's open already */
                menu.depth = 0;
                if (it && it->fn) it->fn(menu.id, it->arg);
                return true; /* clicking outside just closes it */
        }
        return true;
}

static void
draw_menu(void)
{
        float h = menu_item_height();
        int hover_lvl;
        int hover = menu_hit(GetMousePosition(), &hover_lvl);

        for (int k = 0; k < menu.depth; k++) {
                Menu *m     = menu.level[k].menu;
                Rectangle r = menu_rect(k);

                int scroll = menu.level[k].scroll;
                int rows   = menu_rows(k);

                DrawRectangleRec(r, cfg.colors.menu);
                for (int i = scroll; i < m->count && i < scroll + rows; i++) {
                        MenuItem *it   = &m->items[i];
                        Rectangle item = { r.x, r.y + (i - scroll) * h, r.width, h };
                        float ty       = item.y + cfg.menu.padding;

                        /* Highlight the hovered entry, the one chosen with the
                         * arrows and the ones whose submenu is open */
                        bool open = it->sub && k + 1 < menu.depth && menu.level[k + 1].menu == it->sub;
                        bool keys = k == 0 && menu.searchable && i == menu.selected;
                        if ((k == hover_lvl && i == hover) || open || keys) DrawRectangleRec(item, cfg.colors.menu_hover);

                        draw_text(it->label, item.x + 2 * cfg.menu.padding, ty, cfg.menu.font_size, cfg.colors.menu_text);
                        if (it->hint) {
                                float hw = measure_text(it->hint, cfg.menu.font_size);
                                draw_text(it->hint, item.x + item.width - hw - 2 * cfg.menu.padding, ty,
                                          cfg.menu.font_size, cfg.colors.name);
                        }
                        if (it->sub) {
                                float aw = measure_text(MENU_ARROW, cfg.menu.font_size);
                                draw_text(MENU_ARROW, item.x + item.width - aw - 2 * cfg.menu.padding, ty,
                                          cfg.menu.font_size, cfg.colors.menu_text);
                        }
                }
                /* Some don't fit: a scrollbar on the right, as long as the
                 * part shown */
                if (m->count > rows) {
                        float th = fmaxf(r.height * rows / m->count, 8);
                        float ty = r.y + (r.height - th) * scroll / (m->count - rows);
                        DrawRectangleRounded((Rectangle) { r.x + r.width - 5, ty + 2, 3, th - 4 }, 1.0f, 4, cfg.colors.name);
                }
                DrawRectangleLinesEx(r, 1.0f, cfg.colors.menu_border);
        }

        if (menu.depth && menu.searchable) {
                Rectangle b     = menu_search_rect();
                TextInput *t    = menu_search();
                const char *txt = t->len ? TextFormat("%s%s", t->buf, fmod(GetTime(), 1.0) < 0.5 ? "_" : "") : "Type to search modules";
                DrawRectangleRec(b, cfg.colors.field);
                DrawRectangleLinesEx(b, 1.0f, cfg.colors.menu_border);
                draw_text(txt, b.x + 2 * cfg.menu.padding, b.y + cfg.menu.padding, cfg.menu.font_size,
                          t->len ? cfg.colors.square_text : cfg.colors.name);
        }
}

/* The selection box, world space */
static Rectangle
drag_box(void)
{
        Vector2 a = drag.start, b = mouse_world();
        return (Rectangle) { fminf(a.x, b.x), fminf(a.y, b.y), fabsf(a.x - b.x), fabsf(a.y - b.y) };
}

/* Select what touches B (world space). ADD keeps what's selected. */
static void
select_in_box(Rectangle b, bool add)
{
        Da_foreach(sp, squares)
        {
                bool in         = b.width > 0 && b.height > 0 && CheckCollisionRecs(square_rect(*sp), b);
                (*sp)->selected = in || (add && (*sp)->selected);
        }
}

static void
update_mouse(void)
{
        Vector2 mouse = GetMousePosition();
        Vector2 world = GetScreenToWorld2D(mouse, camera);

        if (IsMouseButtonPressed(MOUSE_BUTTON_LEFT) && CheckCollisionPointRec(mouse, canvas_rect())) {
                Hit h = square_hit(world);
                int l;

                switch (h.part) {
                case PART_OUTPUT:
                        drag.kind   = DRAG_WIRE;
                        drag.square = h.square;
                        drag.pin    = h.pin;
                        break;

                case PART_INPUT: {
                        /* Pick up the wire plugged here, keeping its source */
                        if ((l = link_to(h.square->id, h.pin)) < 0) break;
                        Link k = links.items[l];
                        Da_remove(&links, l);
                        if ((drag.square = square_get(k.from))) {
                                drag.kind = DRAG_WIRE;
                                drag.pin  = k.from_pin;
                        }
                        break;
                }

                case PART_FIELD: {
                        PinInfo *p = &h.square->inputs.items[h.pin];
                        if (p->kind == PIN_OPTIONS) {
                                /* Its list, below the field */
                                Rectangle f        = field_rect(h.square, h.pin);
                                options_input      = h.pin;
                                options_menu.count = 0;
                                for (int k = 0; k < p->option_count; k++)
                                        Da_append(&options_menu, (MenuItem) { .label = p->options[k], .fn = option_choose, .arg = (void *) (intptr_t) k });
                                menu_open(&options_menu, h.square->id,
                                          GetWorldToScreen2D((Vector2) { f.x, f.y + f.height }, camera));
                        } else if (p->type == LI_BOOL) {
                                pin_store(p, li_bool(!(p->value.type == LI_BOOL && p->value.b)));
                        } else {
                                field_edit_start(h.square, h.pin);
                        }
                        break;
                }

                case PART_BODY: {
                        bool shift = IsKeyDown(KEY_LEFT_SHIFT) || IsKeyDown(KEY_RIGHT_SHIFT);

                        /* Double click opens the settings */
                        static Square *last;
                        static double last_time;
                        double now = GetTime();
                        if (h.square->params.count && h.square == last && now - last_time < 0.3) {
                                panel_open(h.square);
                                last = NULL;
                                break;
                        }
                        last      = h.square;
                        last_time = now;

                        /* Shift adds or removes it. Without it, clicking a
                         * selected square keeps the selection, to move it. */
                        if (shift) {
                                h.square->selected = !h.square->selected;
                                if (!h.square->selected) break;
                        } else if (!h.square->selected) {
                                select_all(false);
                                h.square->selected = true;
                        }

                        /* Move to the top of the draw order */
                        Da_remove(&squares, square_index(h.square));
                        Da_append(&squares, h.square);
                        drag.kind = DRAG_SQUARE;
                        drag.last = world;
                        break;
                }

                case PART_NONE:
                        drag.kind  = DRAG_BOX;
                        drag.start = world;
                        break;
                }
        }

        if (drag.kind == DRAG_SQUARE) {
                Vector2 d = Vector2Subtract(world, drag.last);
                Da_foreach(sp, squares)
                {
                        if ((*sp)->selected) (*sp)->pos = Vector2Add((*sp)->pos, d);
                }
                drag.last = world;
        }

        if (IsMouseButtonReleased(MOUSE_BUTTON_LEFT)) {
                if (drag.kind == DRAG_WIRE) {
                        /* Dropped outside an input: the wire is discarded */
                        Hit h = square_hit(world);
                        if (h.part == PART_INPUT) link_add(drag.square, drag.pin, h.square, h.pin);
                }
                if (drag.kind == DRAG_BOX) {
                        /* Everything the box touches. A click (no box) just
                         * clears the selection. Shift adds to it. */
                        select_in_box(drag_box(), IsKeyDown(KEY_LEFT_SHIFT) || IsKeyDown(KEY_RIGHT_SHIFT));
                }
                drag.kind = DRAG_NONE;
        }
}

static void
update_camera(void)
{
        Vector2 mouse = GetMousePosition();

        if (IsMouseButtonDown(MOUSE_BUTTON_RIGHT)) {
                Vector2 delta = Vector2Scale(GetMouseDelta(), -1.0f / camera.zoom);
                camera.target = Vector2Add(camera.target, delta);
        }

        /* Over an open menu the wheel scrolls it instead */
        bool over_menu = false;
        for (int k = 0; k < menu.depth; k++)
                over_menu = over_menu || CheckCollisionPointRec(mouse, menu_rect(k));
        if (menu.depth && menu.searchable) over_menu = over_menu || CheckCollisionPointRec(mouse, menu_search_rect());

        float wheel = GetMouseWheelMove();
        if (wheel != 0 && !over_menu && CheckCollisionPointRec(mouse, canvas_rect())) {
                /* Zoom around the cursor: keep the world point under it fixed */
                camera.target = GetScreenToWorld2D(mouse, camera);
                camera.offset = mouse;
                float scale   = cfg.canvas.zoom_speed * wheel;
                camera.zoom   = Clamp(expf(logf(camera.zoom) + scale),
                                      cfg.canvas.zoom_min, cfg.canvas.zoom_max);
        }
}

static void
draw_grid(void)
{
        Rectangle r = canvas_rect();
        Vector2 tl  = GetScreenToWorld2D((Vector2) { r.x, r.y }, camera);
        Vector2 br  = GetScreenToWorld2D((Vector2) { r.x + r.width, r.y + r.height }, camera);

        float step  = cfg.canvas.grid_step;
        int major   = cfg.canvas.grid_major;
        float x0    = floorf(tl.x / step) * step;
        float y0    = floorf(tl.y / step) * step;
        float thick = 1.0f / camera.zoom;

        for (float x = x0; x <= br.x; x += step) {
                Color c = ((int) roundf(x / step) % major == 0) ? cfg.colors.grid_major : cfg.colors.grid;
                DrawLineEx((Vector2) { x, tl.y }, (Vector2) { x, br.y }, thick, c);
        }
        for (float y = y0; y <= br.y; y += step) {
                Color c = ((int) roundf(y / step) % major == 0) ? cfg.colors.grid_major : cfg.colors.grid;
                DrawLineEx((Vector2) { tl.x, y }, (Vector2) { br.x, y }, thick, c);
        }
}

/* Bezier control points of the wire from A to B */
static void
wire_controls(Vector2 a, Vector2 b, Vector2 *c1, Vector2 *c2)
{
        float dx = fmaxf(fabsf(b.x - a.x), fabsf(b.y - a.y)) * 0.5;
        *c1      = (Vector2) { a.x + dx, a.y };
        *c2      = (Vector2) { b.x - dx, b.y };
}

static void
draw_wire_ex(Vector2 a, Vector2 b, Color color, float thick)
{
        Vector2 c1, c2;
        wire_controls(a, b, &c1, &c2);
        DrawSplineSegmentBezierCubic(a, c1, c2, b, thick, color);
}

static void
draw_wire(Vector2 a, Vector2 b, Color color)
{
        draw_wire_ex(a, b, color, cfg.canvas.wire_thickness);
}

/* 1 right after T, fading to 0 after flash_time */
static float
glow(double t)
{
        if (t <= 0 || cfg.canvas.flash_time <= 0) return 0;
        float f = 1 - (GetTime() - t) / cfg.canvas.flash_time;
        return Clamp(f, 0, 1);
}

/* Distance from P to the segment AB */
static float
segment_distance(Vector2 p, Vector2 a, Vector2 b)
{
        Vector2 ab = Vector2Subtract(b, a);
        float len  = Vector2LengthSqr(ab);
        float t    = len > 0 ? Clamp(Vector2DotProduct(Vector2Subtract(p, a), ab) / len, 0, 1) : 0;
        return Vector2Distance(p, Vector2Add(a, Vector2Scale(ab, t)));
}

/* Index of the link whose wire passes under world point P, -1 if none */
static int
link_hit(Vector2 p)
{
        float near = cfg.canvas.wire_thickness / 2 + 5 / camera.zoom;

        for (int i = links.count - 1; i >= 0; i--) {
                Square *a = square_get(links.items[i].from), *b = square_get(links.items[i].to);
                if (!a || !b) continue;
                Vector2 p1 = pin_pos(a, PART_OUTPUT, links.items[i].from_pin);
                Vector2 p2 = pin_pos(b, PART_INPUT, links.items[i].to_pin);
                Vector2 c1, c2, prev = p1;
                wire_controls(p1, p2, &c1, &c2);
                for (int k = 1; k <= 24; k++) {
                        Vector2 q = GetSplinePointBezierCubic(p1, c1, c2, p2, k / 24.0f);
                        if (segment_distance(p, prev, q) < near) return i;
                        prev = q;
                }
        }
        return -1;
}

/* "int -> tick" above the middle of a wire with incompatible types */
static void
draw_wire_error(Vector2 a, Vector2 b, Li_Type out, Li_Type in)
{
        Vector2 c1, c2;
        wire_controls(a, b, &c1, &c2);
        Vector2 mid = GetSplinePointBezierCubic(a, c1, c2, b, 0.5f);

        const char *msg = TextFormat("%s -> %s", type_names[out], type_names[in]);
        float fs        = cfg.canvas.pin_font_size;
        float pad       = 4;
        float tw        = measure_canvas_text(msg, fs);
        Rectangle r     = { mid.x - tw / 2 - pad, mid.y - fs - 2 * pad - cfg.canvas.wire_thickness - 4,
                            tw + 2 * pad, fs + 2 * pad };

        DrawRectangleRec(r, cfg.colors.error);
        draw_canvas_text(msg, r.x + pad, r.y + pad, fs, cfg.colors.error_text);
}

/* TINT overrides the type color (hover), NULL for none */
static void
draw_pin(Vector2 p, Li_Type type, bool connected, const Color *tint)
{
        float r = cfg.canvas.pin_radius;
        Color c = tint ? *tint : cfg.colors.types[type];

        DrawCircleV(p, r, cfg.colors.square_border);
        if (connected)
                DrawCircleV(p, r * 0.7f, c);
        else
                DrawRing(p, r * 0.45f, r * 0.7f, 0, 360, 24, c);
}

/* IN cut with ".." at the end to fit in MAX */
static void
fit_canvas_text(const char *in, char *out, int size, float max, float fs)
{
        int len = snprintf(out, size, "%s", in);
        if (len >= size) len = size - 1;
        while (len > 2 && measure_canvas_text(out, fs) > max) {
                len--;
                out[len]     = '\0';
                out[len - 1] = '.';
                out[len - 2] = '.';
        }
}

/* The typed value or chosen option of input I */
static void
draw_field(Square *s, int i)
{
        PinInfo *p   = &s->inputs.items[i];
        Rectangle f  = field_rect(s, i);
        float fs     = cfg.canvas.pin_font_size;
        bool typing  = field_edit.square == s && field_edit.input == i;
        Color border = typing && field_edit.error ? cfg.colors.error :
                       typing                     ? cfg.colors.pin_hover :
                                                    cfg.colors.square_border;
        char buf[256], text[256];

        DrawRectangleRec(f, cfg.colors.field);
        DrawRectangleLinesEx(f, typing ? 2.0f : 1.0f, border);

        if (p->kind == PIN_VALUE && p->type == LI_BOOL) {
                if (p->value.type == LI_BOOL && p->value.b)
                        DrawRectangleRec((Rectangle) { f.x + 4, f.y + 4, f.width - 8, f.height - 8 }, cfg.colors.square_text);
                return;
        }

        float arrow = p->kind == PIN_OPTIONS ? measure_canvas_text(FIELD_ARROW, fs) + 6 : 0;
        if (typing)
                snprintf(buf, sizeof buf, "%s%s", field_edit.text.buf, fmod(GetTime(), 1.0) < 0.5 ? "_" : "");
        else
                input_text(s, i, buf, sizeof buf, false);
        fit_canvas_text(buf, text, sizeof text, f.width - 8 - arrow, fs);
        draw_canvas_text(text, f.x + 4, f.y + (f.height - fs) / 2, fs, cfg.colors.square_text);
        if (arrow) draw_canvas_text(FIELD_ARROW, f.x + f.width - arrow, f.y + (f.height - fs) / 2, fs, cfg.colors.square_text);
}

/* HOVER_TINT is the color of the hovered pin, if it's one of S */
static void
draw_square(Square *s, Hit hover, Color hover_tint)
{
        Rectangle r = square_rect(s);
        float fs    = cfg.canvas.pin_font_size;
        float gap   = pin_label_gap();

        DrawRectangleRec(r, cfg.colors.square);
        /* Glows when a tick reaches it */
        float g = glow(s->flash);
        if (g > 0) DrawRectangleLinesEx((Rectangle) { r.x - 5, r.y - 5, r.width + 10, r.height + 10 }, 4.0f, Fade(cfg.colors.flash, g));
        if (s->selected)
                DrawRectangleLinesEx((Rectangle) { r.x - 2, r.y - 2, r.width + 4, r.height + 4 }, 3.0f, cfg.colors.selection);
        else
                DrawRectangleLinesEx(r, 2.0f, cfg.colors.square_border);

        for (int i = 0; i < s->inputs.count; i++) {
                PinInfo *in = &s->inputs.items[i];
                Vector2 p   = pin_pos(s, PART_INPUT, i);
                bool hov    = hover.square == s && hover.part == PART_INPUT && hover.pin == i;
                /* Options can't be wired: no pin */
                if (in->kind != PIN_OPTIONS) draw_pin(p, in->type, pin_connected(s, PART_INPUT, i), hov ? &hover_tint : NULL);
                draw_canvas_text(in->name, p.x + gap, p.y - fs / 2, fs, cfg.colors.square_text);
                if (field_visible(s, i)) draw_field(s, i);
        }
        for (int i = 0; i < s->outputs.count; i++) {
                PinInfo *out = &s->outputs.items[i];
                Vector2 p    = pin_pos(s, PART_OUTPUT, i);
                bool hov     = hover.square == s && hover.part == PART_OUTPUT && hover.pin == i;
                draw_pin(p, out->type, pin_connected(s, PART_OUTPUT, i), hov ? &hover_tint : NULL);
                draw_canvas_text(out->name, p.x - gap - measure_canvas_text(out->name, fs), p.y - fs / 2, fs,
                                 cfg.colors.square_text);
        }

        /* Text in the band at the top. Cut to fit, so a long value doesn't
         * cover the pins or resize the square. */
        if (*s->text) {
                char text[sizeof s->text];
                fit_canvas_text(s->text, text, sizeof text, r.width - 8, fs);
                float tw = measure_canvas_text(text, fs);
                draw_canvas_text(text, r.x + (r.width - tw) / 2, r.y + (square_text_band() - fs) / 2 + 2, fs,
                                 cfg.colors.square_text);
        }

        /* Module name below */
        float nfs = cfg.canvas.name_font_size;
        float nw  = measure_canvas_text(s->module->label, nfs);
        draw_canvas_text(s->module->label, r.x + (r.width - nw) / 2, r.y + r.height + 4, nfs, cfg.colors.name);
}

static void
draw_canvas(void)
{
        Rectangle r = canvas_rect();
        BeginScissorMode(r.x, r.y, r.width, r.height);
        BeginMode2D(camera);

        Vector2 world = GetScreenToWorld2D(GetMousePosition(), camera);
        Hit hover     = square_hit(world);
        Color tint    = cfg.colors.pin_hover;

        /* While wiring, inputs of another type light up as an error */
        if (drag.kind == DRAG_WIRE && hover.part == PART_INPUT) {
                if (!link_allowed(drag.square, drag.pin, hover.square, hover.pin))
                        hover.part = PART_NONE;
                else if (!types_compatible(pin_info(drag.square, PART_OUTPUT, drag.pin)->type,
                                           pin_info(hover.square, PART_INPUT, hover.pin)->type))
                        tint = cfg.colors.error;
        }

        draw_grid();

        /* Wires go below the squares. Their type is the output type. They
         * glow when something goes through them, and get thicker hovered. */
        int hovered = hover.part == PART_NONE && drag.kind == DRAG_NONE ? link_hit(world) : -1;
        for (int i = 0; i < links.count; i++) {
                Link *l   = &links.items[i];
                Square *a = square_get(l->from);
                Square *b = square_get(l->to);
                if (!a || !b) continue;
                Color c     = cfg.colors.types[pin_info(a, PART_OUTPUT, l->from_pin)->type];
                float g     = glow(l->flash);
                float thick = cfg.canvas.wire_thickness * (1 + g + (i == hovered ? 0.6f : 0));
                draw_wire_ex(pin_pos(a, PART_OUTPUT, l->from_pin), pin_pos(b, PART_INPUT, l->to_pin),
                             ColorLerp(c, cfg.colors.flash, g), thick);
        }

        Da_foreach(s, squares)
        {
                draw_square(*s, hover, tint);
        }

        if (drag.kind == DRAG_BOX) {
                Rectangle b = drag_box();
                DrawRectangleRec(b, Fade(cfg.colors.selection, 0.12f));
                DrawRectangleLinesEx(b, 1.0f / camera.zoom, cfg.colors.selection);
        }

        /* Errors go above everything */
        Da_foreach(l, links)
        {
                if (link_valid(*l)) continue;
                Square *a = square_get(l->from);
                Square *b = square_get(l->to);
                if (!a || !b) continue;
                draw_wire_error(pin_pos(a, PART_OUTPUT, l->from_pin), pin_pos(b, PART_INPUT, l->to_pin),
                                pin_info(a, PART_OUTPUT, l->from_pin)->type, pin_info(b, PART_INPUT, l->to_pin)->type);
        }

        if (drag.kind == DRAG_WIRE) {
                Color c = cfg.colors.types[pin_info(drag.square, PART_OUTPUT, drag.pin)->type];
                draw_wire(pin_pos(drag.square, PART_OUTPUT, drag.pin), world, c);
        }

        EndMode2D();
        EndScissorMode();
}

/* ---- Settings panel ---- */

#define PANEL_WIDTH 260

static float
panel_row_height(void)
{
        return cfg.menu.font_size + 2 * cfg.menu.padding;
}

/* Screen space, at the right of the square, inside the canvas. First row is
 * the title, then one per param. */
static Rectangle
panel_rect(void)
{
        Rectangle sr = square_rect(panel.square);
        Vector2 p    = GetWorldToScreen2D((Vector2) { sr.x + sr.width, sr.y }, camera);
        Rectangle c  = canvas_rect();
        Rectangle r  = { p.x + 16, p.y, PANEL_WIDTH, (panel.square->params.count + 1) * panel_row_height() };

        r.x = Clamp(r.x, 0, fmaxf(0, c.width - r.width));
        r.y = Clamp(r.y, 0, fmaxf(0, c.height - r.height));
        return r;
}

static Rectangle
panel_field_rect(int i)
{
        Rectangle r = panel_rect();
        float h     = panel_row_height();
        float pad   = cfg.menu.padding;
        return (Rectangle) { r.x + r.width * 0.45f, r.y + (i + 1) * h + pad / 2, r.width * 0.55f - pad, h - pad };
}

static Rectangle
panel_close_rect(void)
{
        Rectangle r = panel_rect();
        float h     = panel_row_height();
        return (Rectangle) { r.x + r.width - h, r.y, h, h };
}

/* Apply what is typed. On error the field stays open. */
static bool
panel_apply(void)
{
        if (panel.field < 0) return true;
        if (!param_set_text(panel.square, panel.field, panel.text.buf)) {
                panel.error = true;
                return false;
        }
        panel.field = -1;
        panel.error = false;
        return true;
}

static void
panel_edit(int i)
{
        char buf[256];
        param_text(&panel.square->params.items[i], buf, sizeof buf, false);
        text_input_set(&panel.text, buf);
        panel.field = i;
        panel.error = false;
}

/* Returns true if the panel consumed the input this frame. */
static bool
update_panel(void)
{
        bool changed = false;

        if (!panel.square) return false;

        Vector2 mouse = GetMousePosition();
        bool inside   = CheckCollisionPointRec(mouse, panel_rect());
        int count     = panel.square->params.count;

        if (panel.field >= 0) {
                switch (text_input_update(&panel.text, &changed)) {
                case TEXT_DONE: panel_apply(); break;
                case TEXT_NEXT: {
                        int next = (panel.field + 1) % count;
                        if (panel_apply()) panel_edit(next);
                        break;
                }
                case TEXT_CANCEL: panel.field = -1; return true; /* keep the old value */
                case TEXT_TYPING: break;
                }
                if (changed) panel.error = false;
        } else if (IsKeyPressed(KEY_ESCAPE)) {
                panel_close();
                return true;
        }

        if (IsMouseButtonPressed(MOUSE_BUTTON_LEFT)) {
                if (!inside) {
                        /* Clicking away applies what's valid and closes it */
                        panel_apply();
                        panel_close();
                        return false;
                }
                if (CheckCollisionPointRec(mouse, panel_close_rect())) {
                        panel_apply();
                        panel_close();
                        return true;
                }
                for (int i = 0; i < count; i++) {
                        if (!CheckCollisionPointRec(mouse, panel_field_rect(i))) continue;
                        if (panel.field != i && !panel_apply()) break;
                        Param *p = &panel.square->params.items[i];
                        if (p->value.type == LI_BOOL)
                                param_set_text(panel.square, i, p->value.b ? "false" : "true");
                        else if (panel.field != i)
                                panel_edit(i);
                        break;
                }
                return true;
        }

        if (IsMouseButtonPressed(MOUSE_BUTTON_RIGHT) && !inside) panel_close();
        return false;
}

static void
draw_panel(void)
{
        if (!panel.square) return;

        Rectangle r = panel_rect();
        float h     = panel_row_height();
        float fs    = cfg.menu.font_size;
        float pad   = cfg.menu.padding;

        DrawRectangleRec(r, cfg.colors.menu);
        DrawRectangleLinesEx(r, 1.0f, cfg.colors.menu_border);
        draw_text(panel.square->module->name, r.x + pad, r.y + pad, fs, cfg.colors.menu_text);

        Rectangle x = panel_close_rect();
        if (CheckCollisionPointRec(GetMousePosition(), x)) DrawRectangleRec(x, cfg.colors.menu_hover);
        draw_text("x", x.x + (x.width - measure_text("x", fs)) / 2, r.y + pad, fs, cfg.colors.menu_text);

        for (int i = 0; i < panel.square->params.count; i++) {
                Param *p    = &panel.square->params.items[i];
                Rectangle f = panel_field_rect(i);
                bool typing = panel.field == i;
                char buf[256];

                draw_text(p->name, r.x + pad, r.y + (i + 1) * h + pad, fs, cfg.colors.menu_text);

                if (typing)
                        snprintf(buf, sizeof buf, "%s%s", panel.text.buf, fmod(GetTime(), 1.0) < 0.5 ? "_" : "");
                else
                        param_text(p, buf, sizeof buf, false);

                Color border = panel.error && typing ? cfg.colors.error :
                               typing                ? cfg.colors.pin_hover :
                                                       cfg.colors.menu_border;
                DrawRectangleRec(f, cfg.colors.field);
                DrawRectangleLinesEx(f, typing ? 2.0f : 1.0f, border);

                /* Long values are cut at the field */
                BeginScissorMode(f.x, f.y, f.width, f.height);
                draw_text(buf, f.x + 4, f.y + (f.height - fs) / 2, fs, cfg.colors.square_text);
                EndScissorMode();
        }
}

/* ---- Save and load ---- */

/* File used by Save and Load */
static const char *canvas_path;

/* Message shown in the bar for a few seconds */
static char status[256];
static double status_until;

static void
set_status(const char *fmt, ...)
{
        va_list ap;
        va_start(ap, fmt);
        vsnprintf(status, sizeof status, fmt, ap);
        va_end(ap);
        status_until = GetTime() + 3.0;
        fprintf(stderr, "%s\n", status);
        output_add(status, true);
}

/* Write S as a Lua string literal */
static void
lua_quote(FILE *f, const char *s)
{
        fputc('"', f);
        for (; *s; s++) {
                if (*s == '"' || *s == '\\')
                        fprintf(f, "\\%c", *s);
                else if ((unsigned char) *s < ' ')
                        fprintf(f, "\\%03d", (unsigned char) *s);
                else
                        fputc(*s, f);
        }
        fputc('"', f);
}

/* What canvas_write writes */
enum {
        WRITE_CAMERA   = 1 << 0,
        WRITE_STATE    = 1 << 1, /* what module_save returns */
        WRITE_SELECTED = 1 << 2, /* only selected squares and the links between them */
};

/* The canvas as Lua: camera, squares (draw order) and links. Squares keep
 * their params and typed inputs. */
static void
canvas_write(FILE *f, int flags)
{
        if (flags & WRITE_CAMERA)
                fprintf(f, "camera = { x = %.9g, y = %.9g, offset_x = %.9g, offset_y = %.9g, zoom = %.9g }\n\n",
                        camera.target.x, camera.target.y, camera.offset.x, camera.offset.y, camera.zoom);

        fprintf(f, "squares = {\n");
        Da_foreach(sp, squares)
        {
                Square *sq = *sp;
                if ((flags & WRITE_SELECTED) && !sq->selected) continue;
                fprintf(f, "        { id = %d, module = ", sq->id);
                lua_quote(f, sq->module->name);
                fprintf(f, ", x = %.9g, y = %.9g", sq->pos.x, sq->pos.y);
                if (sq->params.count) {
                        fprintf(f, ", params = {");
                        Da_foreach(p, sq->params)
                        {
                                char buf[256];
                                param_text(p, buf, sizeof buf, true);
                                fprintf(f, " { name = ");
                                lua_quote(f, p->name);
                                fprintf(f, ", value = ");
                                lua_quote(f, buf);
                                fprintf(f, " },");
                        }
                        fprintf(f, " }");
                }
                bool any_field = false;
                for (int k = 0; k < sq->inputs.count; k++)
                        any_field = any_field || sq->inputs.items[k].kind != PIN_WIRE;
                if (any_field) {
                        fprintf(f, ", inputs = {");
                        for (int k = 0; k < sq->inputs.count; k++) {
                                PinInfo *in = &sq->inputs.items[k];
                                char buf[256];
                                if (in->kind == PIN_WIRE) continue;
                                input_text(sq, k, buf, sizeof buf, true);
                                fprintf(f, " { name = ");
                                lua_quote(f, in->name);
                                /* The type too: "2" typed in an any input may be a string */
                                if (in->kind == PIN_VALUE) fprintf(f, ", type = \"%s\"", type_names[in->value.type]);
                                fprintf(f, ", value = ");
                                lua_quote(f, buf);
                                fprintf(f, " },");
                        }
                        fprintf(f, " }");
                }
                const char *state = (flags & WRITE_STATE) && sq->fn.save ? sq->fn.save(sq) : NULL;
                if (state) {
                        fprintf(f, ", state = ");
                        lua_quote(f, state);
                }
                fprintf(f, " },\n");
        }
        fprintf(f, "}\n\n");

        fprintf(f, "links = {\n");
        Da_foreach(l, links)
        {
                if (flags & WRITE_SELECTED) {
                        Square *a = square_get(l->from), *b = square_get(l->to);
                        if (!a || !b || !a->selected || !b->selected) continue;
                }
                fprintf(f, "        { from = %d, from_pin = %d, to = %d, to_pin = %d },\n",
                        l->from, l->from_pin, l->to, l->to_pin);
        }
        fprintf(f, "}\n");
}

static bool
canvas_save(const char *path)
{
        FILE *f = fopen(path, "w");
        if (!f) return false;
        fprintf(f, "-- lineit canvas\n");
        canvas_write(f, WRITE_CAMERA | WRITE_STATE);
        return fclose(f) == 0;
}

/* canvas_write to a string. Must be freed. */
static char *
canvas_text(int flags)
{
        char *buf   = NULL;
        size_t size = 0;
        FILE *f     = open_memstream(&buf, &size);
        if (!f) return strdup("");
        canvas_write(f, flags);
        fclose(f);
        return buf;
}

static Module *
module_find(const char *name)
{
        Da_foreach(mp, modules)
        {
                if (strcmp((*mp)->name, name) == 0) return *mp;
        }
        return NULL;
}

typedef enum ReadMode {
        READ_LOAD,    /* replace the canvas, camera included */
        READ_RESTORE, /* make the canvas match (undo): squares that are still there keep their state */
        READ_PASTE,   /* add, with new ids, at the mouse, selected */
        READ_GRAPH,   /* add, into a subgraph being made (see Subgraphs) */
} ReadMode;

/* Where READ_PASTE puts the top-left corner of what it pastes */
static Vector2 paste_at;

static void
canvas_read(Conf conf, ReadMode mode)
{
        typedef struct {
                int saved_id;
                Square *square;
        } Saved;
        Da(Saved) map = { 0 };
        int n         = 0;

        if (mode != READ_GRAPH) {
                panel_close();
                field_edit.square = NULL;
                drag.kind         = DRAG_NONE;
        }

        if (mode == READ_LOAD) {
                canvas_clear(-1, NULL);
                double cx = 0, cy = 0, ox = 0, oy = 0, zoom = 1;
                Conf_get_num(conf, &cx, "camera.x");
                Conf_get_num(conf, &cy, "camera.y");
                Conf_get_num(conf, &ox, "camera.offset_x");
                Conf_get_num(conf, &oy, "camera.offset_y");
                Conf_get_num(conf, &zoom, "camera.zoom");
                camera = (Camera2D) { .target = { cx, cy }, .offset = { ox, oy }, .zoom = Clamp(zoom, cfg.canvas.zoom_min, cfg.canvas.zoom_max) };
        }
        if (mode == READ_PASTE) {
                Da_foreach(sp, squares)
                {
                        (*sp)->selected = false;
                }
        }

        Conf_get_len(conf, &n, "squares");
        for (int i = 1; i <= n; i++) {
                const char *str;
                int id   = -1;
                int np   = 0;
                double x = 0, y = 0;
                Module *m;
                Square *sq = NULL;

                /* Strings from Conf die on the next call, use them right away */
                if (Conf_get_str(conf, &str, "squares.%d.module", i) != CONF_OK) continue;
                if (!(m = module_find(str))) {
                        fprintf(stderr, "load: module %s not found, square skipped\n", str);
                        continue;
                }
                char *state = Conf_get_str(conf, &str, "squares.%d.state", i) == CONF_OK ? strdup(str) : NULL;
                Conf_get_int(conf, &id, "squares.%d.id", i);
                Conf_get_num(conf, &x, "squares.%d.x", i);
                Conf_get_num(conf, &y, "squares.%d.y", i);

                if (mode == READ_RESTORE && id >= 0 && (sq = square_get(id))) {
                        /* Already there: keep it (and its state) unless it's
                         * another module now, or a repeated id */
                        bool seen = false;
                        Da_foreach(e, map)
                        {
                                seen = seen || e->square == sq;
                        }
                        if (sq->module != m || seen) {
                                if (!seen) square_delete(id, NULL);
                                sq = NULL;
                                id = seen ? -1 : id;
                        }
                }
                if (!sq) {
                        /* Keep the saved id if it's free, so undo snapshots match */
                        int new_id = mode != READ_PASTE && id >= 0 && !square_get(id) ? id : -1;
                        sq         = square_new_id(m, (Vector2) { 0, 0 }, new_id);
                }
                if (!sq) {
                        free(state);
                        continue;
                }
                sq->pos = (Vector2) { x, y };

                Conf_get_len(conf, &np, "squares.%d.params", i);
                for (int j = 1; j <= np; j++) {
                        char name[256];
                        int k;
                        if (Conf_get_str(conf, &str, "squares.%d.params.%d.name", i, j) != CONF_OK) continue;
                        snprintf(name, sizeof name, "%s", str);
                        if (Conf_get_str(conf, &str, "squares.%d.params.%d.value", i, j) != CONF_OK) continue;
                        if ((k = param_find(sq, name)) < 0) continue;
                        char now[256];
                        param_text(&sq->params.items[k], now, sizeof now, true);
                        if (strcmp(now, str) != 0 && !param_set_text(sq, k, str))
                                fprintf(stderr, "load: invalid value for %s.%s, using default\n", m->name, name);
                }

                int ni = 0;
                Conf_get_len(conf, &ni, "squares.%d.inputs", i);
                for (int j = 1; j <= ni; j++) {
                        char name[256];
                        Li_Type type = LI_NONE;
                        int k;
                        if (Conf_get_str(conf, &str, "squares.%d.inputs.%d.name", i, j) != CONF_OK) continue;
                        snprintf(name, sizeof name, "%s", str);
                        if (Conf_get_str(conf, &str, "squares.%d.inputs.%d.type", i, j) == CONF_OK) {
                                for (int t = 0; t < LI_TYPE_COUNT; t++)
                                        if (!strcmp(str, type_names[t])) type = t;
                        }
                        if (Conf_get_str(conf, &str, "squares.%d.inputs.%d.value", i, j) != CONF_OK) continue;
                        if ((k = input_find(sq, name)) < 0) continue;
                        /* An empty value of an any input is "none" */
                        if (type == LI_NONE && sq->inputs.items[k].kind == PIN_VALUE && !*str)
                                pin_store(&sq->inputs.items[k], (Li_Value) { .type = LI_NONE });
                        else if (!input_set_text(sq, k, str, type))
                                fprintf(stderr, "load: invalid value for %s input %s, using default\n", m->name, name);
                }

                /* After the params, so state can override what module_param set */
                if (state && sq->fn.load) sq->fn.load(sq, state);
                free(state);
                if (mode == READ_PASTE) sq->selected = true;
                Da_append(&map, (Saved) { id, sq });
        }

        if (mode == READ_RESTORE) {
                /* Remove what isn't in it, and use its draw order */
                for (int i = squares.count - 1; i >= 0; i--) {
                        bool keep = false;
                        Da_foreach(e, map)
                        {
                                keep = keep || e->square == squares.items[i];
                        }
                        if (!keep) square_delete(squares.items[i]->id, NULL);
                }
                squares.count = 0;
                Da_foreach(e, map)
                {
                        Da_append(&squares, e->square);
                }
                links.count = 0;
        }

        if (mode == READ_PASTE && map.count) {
                /* The top-left corner of what's pasted goes to the mouse */
                Vector2 lo = map.items[0].square->pos;
                Da_foreach(e, map)
                {
                        lo = Vector2Min(lo, e->square->pos);
                }
                Vector2 off = Vector2Subtract(paste_at, lo);
                Da_foreach(e, map)
                {
                        e->square->pos = Vector2Add(e->square->pos, off);
                }
        }

        n = 0;
        Conf_get_len(conf, &n, "links");
        for (int i = 1; i <= n; i++) {
                int from = -1, from_pin = -1, to = -1, to_pin = -1;
                Square *a = NULL, *b = NULL;

                Conf_get_int(conf, &from, "links.%d.from", i);
                Conf_get_int(conf, &from_pin, "links.%d.from_pin", i);
                Conf_get_int(conf, &to, "links.%d.to", i);
                Conf_get_int(conf, &to_pin, "links.%d.to_pin", i);
                Da_foreach(e, map)
                {
                        if (e->saved_id == from) a = e->square;
                        if (e->saved_id == to) b = e->square;
                }
                /* link_add checks the pins still exist */
                if (a && b) link_add(a, from_pin, b, to_pin);
        }

        Da_destroy(&map);
}

/* Replace the canvas with the one in PATH. Squares of missing modules are
 * skipped with their wires. Returns false if the file can't be read. */
static bool
canvas_read_file(const char *path, ReadMode mode)
{
        Conf conf;
        if (Conf_open(&conf, path) != CONF_OK) return false;
        canvas_read(conf, mode);
        Conf_close(conf);
        return true;
}

/* Same, from what canvas_text returned (conf.h only reads files) */
static bool
canvas_read_text(const char *text, ReadMode mode)
{
        char path[PATH_MAX];
        FILE *f;

        if (!tmp_dir_make()) return false;
        snprintf(path, sizeof path, "%s/canvas.lua", tmp_dir);
        if (!(f = fopen(path, "w"))) return false;
        fputs(text, f);
        fclose(f);
        bool ok = canvas_read_file(path, mode);
        remove(path);
        return ok;
}

/* ---- Subgraphs ---- */

/* A module can also be a canvas: a .lua under the modules dir. Each square
 * of it (a subgraph) gets its own copy of that canvas, hidden, with its own
 * squares and wires. Its graph/input squares are the inputs of the
 * subgraph, and its graph/output squares the outputs, named by their name
 * param, in the order they are from top to bottom (left to right on the
 * same row).
 *
 * The inner squares are not in `squares` and `links`: while a call goes into
 * a subgraph, graph_swap puts its lists there, and the next graph_swap puts
 * them back. So everything works the same inside, li_get and li_tick
 * included. Calls that go back out (a graph/input reading an input of its
 * subgraph, a graph/output ticking one of its outputs) swap too, which
 * brings back the lists outside for that while. Calls nest, so the swaps
 * always come in pairs.
 *
 * Its variables, lists and trees are swapped the same way: they are its
 * locals, like a C function's, and the ones outside can't be seen from
 * inside (nor the other way around). Each square of the subgraph has its
 * own, and keeps them between ticks. Data goes in and out through its
 * pins, or through globals (li_global_*). */

typedef struct Graph {
        SquareList squares; /* while swapped in: the ones outside */
        LinkList links;
        VarList vars; /* its locals: variables, lists and trees */
        BT *lists, *trees;
        Da(Square *) inputs; /* graph/input squares inside, by port */
        Da(Square *) outputs;
} Graph;

static void
graph_swap(Graph *g)
{
        SquareList s = squares;
        LinkList l   = links;
        VarList v    = vars;
        BT *ls = lists, *ts = trees;
        squares    = g->squares;
        links      = g->links;
        vars       = g->vars;
        lists      = g->lists;
        trees      = g->trees;
        g->squares = s;
        g->links   = l;
        g->vars    = v;
        g->lists   = ls;
        g->trees   = ts;
}

/* Port squares: pick a type, and an input or output of that type */
static const Li_Type port_types[] = { LI_TICK, LI_ANY, LI_BOOL, LI_INT, LI_FLOAT, LI_STRING };
#define PORT_OPTIONS "tick|any|bool|int|float|string"
enum { PORT_IN_TYPE }; /* inputs of graph/input */
enum { PORT_OUT_TYPE,
       PORT_OUT_VALUE }; /* of graph/output */

static Li_Type
port_type(Square *s)
{
        Li_Value v = li_get(s, 0);
        return v.i >= 0 && v.i < countof(port_types) ? port_types[v.i] : LI_ANY;
}

static const char *
port_name(Square *s)
{
        return li_get_param(s, 0).s;
}

static void
port_in_init(Li_Node *n)
{
        li_input_options(n, "type", PORT_OPTIONS, 1);
        li_output(n, "value", LI_ANY);
        li_param(n, "name", li_string("in"));
}

static void
port_out_init(Li_Node *n)
{
        li_input_options(n, "type", PORT_OPTIONS, 1);
        li_input(n, "value", LI_ANY);
        li_param(n, "name", li_string("out"));
}

/* The type chosen goes to the pin */
static void
port_in_draw(Li_Node *n)
{
        li_output_type(n, 0, port_type(n));
        li_text(n, "%s", port_name(n));
}

static void
port_out_draw(Li_Node *n)
{
        PinInfo *p = pin_info(n, PART_INPUT, PORT_OUT_VALUE);
        if (p) p->type = port_type(n);
        li_text(n, "%s", port_name(n));
}

/* The input of the subgraph. Outside of one there is nothing to read. */
static void
port_in_eval(Li_Node *n)
{
        Li_Value v = { .type = LI_NONE };
        if (n->owner) {
                graph_swap(n->owner->graph);
                v = li_get(n->owner, n->port);
                graph_swap(n->owner->graph);
        }
        li_set(n, 0, v);
}

static void
port_out_tick(Li_Node *n, int input)
{
        if (input != PORT_OUT_VALUE || !n->owner) return;
        graph_swap(n->owner->graph);
        li_tick(n->owner, n->port);
        graph_swap(n->owner->graph);
}

static const ModuleFns port_in_fns  = { .init = port_in_init, .eval = port_in_eval, .draw = port_in_draw };
static const ModuleFns port_out_fns = { .init = port_out_init, .tick = port_out_tick, .draw = port_out_draw };

static int
port_cmp(const void *a, const void *b)
{
        Vector2 p = (*(Square **) a)->pos, q = (*(Square **) b)->pos;
        if (p.y != q.y) return p.y < q.y ? -1 : 1;
        return (p.x > q.x) - (p.x < q.x);
}

/* A typed field to start with, by type */
static Li_Value
port_default(Li_Type type)
{
        switch (type) {
        case LI_BOOL: return li_bool(false);
        case LI_INT: return li_int(0);
        case LI_FLOAT: return li_float(0);
        case LI_STRING: return li_string("");
        default: return (Li_Value) { .type = LI_NONE };
        }
}

/* Subgraphs being made, outermost first: one can't be inside itself */
static Module *graph_making[16];
static int graph_depth;

static void
graph_init(Li_Node *n)
{
        Graph *g     = calloc(1, sizeof *g);
        Module *last = last_module;
        n->graph     = g;

        for (int i = 0; i < graph_depth; i++) {
                if (graph_making[i] == n->module) {
                        li_text(n, "inside itself");
                        return;
                }
        }
        if (graph_depth == countof(graph_making)) {
                li_text(n, "too deep");
                return;
        }

        graph_making[graph_depth++] = n->module;
        graph_swap(g);
        if (!canvas_read_file(n->module->path, READ_GRAPH)) li_text(n, "can't read it");
        Da_foreach(sp, squares)
        {
                Square *s = *sp;
                if (s->fn.draw == port_in_draw) Da_append(&g->inputs, s);
                if (s->fn.draw == port_out_draw) Da_append(&g->outputs, s);
                if (s->fn.draw == port_in_draw || s->fn.draw == port_out_draw) s->fn.draw(s);
        }
        graph_swap(g);
        graph_depth--;
        last_module = last; /* not the ones inside */

        if (g->inputs.count) qsort(g->inputs.items, g->inputs.count, sizeof *g->inputs.items, port_cmp);
        if (g->outputs.count) qsort(g->outputs.items, g->outputs.count, sizeof *g->outputs.items, port_cmp);
        for (int k = 0; k < g->inputs.count; k++) {
                Square *p    = g->inputs.items[k];
                Li_Type type = port_type(p);
                p->owner     = n;
                p->port      = k;
                if (type == LI_TICK)
                        li_input(n, port_name(p), type);
                else
                        li_input_value(n, port_name(p), type, port_default(type));
        }
        for (int k = 0; k < g->outputs.count; k++) {
                Square *p = g->outputs.items[k];
                p->owner  = n;
                p->port   = k;
                li_output(n, port_name(p), port_type(p));
        }
}

static void
graph_tick(Li_Node *n, int input)
{
        Graph *g = n->graph;
        if (input >= g->inputs.count) return;
        graph_swap(g);
        li_tick(g->inputs.items[input], 0);
        graph_swap(g);
}

static void
graph_eval(Li_Node *n)
{
        Graph *g = n->graph;
        for (int k = 0; k < g->outputs.count; k++) {
                Square *p = g->outputs.items[k];
                if (port_type(p) == LI_TICK) continue;
                graph_swap(g);
                Li_Value v = li_get(p, PORT_OUT_VALUE);
                graph_swap(g);
                li_set(n, k, v);
        }
}

/* The squares inside get the same calls as the ones outside */
#define GRAPH_EACH(name, cb)                                          \
        static void name(Li_Node *n)                                  \
        {                                                             \
                graph_swap(n->graph);                                 \
                for (int i = 0; i < squares.count; i++) {             \
                        Square *s = squares.items[i];                 \
                        if (s->fn.cb) s->fn.cb(s);                    \
                }                                                     \
                graph_swap(n->graph);                                 \
        }
GRAPH_EACH(graph_frame, frame)
GRAPH_EACH(graph_start, start)
GRAPH_EACH(graph_draw, draw)
#undef GRAPH_EACH

static void
graph_destroy(Li_Node *n)
{
        Graph *g = n->graph;
        graph_swap(g);
        Da_foreach(sp, squares)
        {
                square_free(*sp);
        }
        Da_destroy(&squares);
        Da_destroy(&links);
        var_list_clear(&vars);
        containers_clear();
        graph_swap(g);
        Da_destroy(&g->inputs);
        Da_destroy(&g->outputs);
        free(g);
        n->graph = NULL;
}

static const ModuleFns graph_fns = {
        .init    = graph_init,
        .tick    = graph_tick,
        .eval    = graph_eval,
        .frame   = graph_frame,
        .start   = graph_start,
        .draw    = graph_draw,
        .destroy = graph_destroy,
};

/* graph/input and graph/output, in the menu with the modules */
static void
graph_builtins(void)
{
        static const struct {
                const char *name;
                const ModuleFns *fn;
        } builtins[] = { { "graph/input", &port_in_fns }, { "graph/output", &port_out_fns } };

        for (int i = 0; i < countof(builtins); i++) {
                if (module_find(builtins[i].name)) continue;
                Module *m  = calloc(1, sizeof *m);
                m->name    = strdup(builtins[i].name);
                m->label   = strrchr(m->name, '/') + 1;
                m->builtin = builtins[i].fn;
                m->seen    = true;
                Da_append(&modules, m);
        }
}

/* Whether S is of M, or is a subgraph with one of M inside */
static bool
square_uses(Square *s, Module *m)
{
        if (s->module == m) return true;
        if (!s->graph) return false;
        Da_foreach(sp, s->graph->squares)
        {
                if (square_uses(*sp, m)) return true;
        }
        return false;
}

/* M was rebuilt: the squares of the canvas shown get new copies of it. Everything else in them
 * stays: position, params, typed inputs, wires, state, selection. */
static void
module_reload_squares_here(Module *m)
{
        Da(int) selected = { 0 };
        bool any         = false;

        Da_foreach(sp, squares)
        {
                any = any || square_uses(*sp, m);
                if ((*sp)->selected) Da_append(&selected, (*sp)->id);
        }
        if (!any) {
                Da_destroy(&selected);
                return;
        }

        char *text = canvas_text(WRITE_STATE);
        /* Close the old copies: restoring makes new ones from the new build */
        for (int i = squares.count - 1; i >= 0; i--) {
                if (square_uses(squares.items[i], m)) square_delete(squares.items[i]->id, NULL);
        }
        canvas_read_text(text, READ_RESTORE);
        free(text);

        Da_foreach(id, selected)
        {
                Square *s = square_get(*id);
                if (s) s->selected = true;
        }
        Da_destroy(&selected);
}

/* ---- Undo ---- */

#define UNDO_MAX 200

/* Snapshots (canvas_text without camera or state), oldest first */
static struct {
        Da(char *) undo;
        Da(char *) redo;
        char *current;
} history;

static void
history_free_list(void *list)
{
        Da(char *) *l = list;
        Da_foreach(t, *l)
        {
                free(*t);
        }
        l->count = 0;
}

/* Forget everything: a new canvas */
static void
history_reset(void)
{
        history_free_list(&history.undo);
        history_free_list(&history.redo);
        free(history.current);
        history.current = canvas_text(0);
}

static bool
editing_text(void)
{
        return panel.field >= 0 || field_edit.square || (menu.depth && menu.searchable) || prompt_is_open();
}

/* Called every frame: if the canvas changed since the last snapshot (and
 * nothing is half done: a drag, typing), the last one goes to undo */
static void
history_check(void)
{
        if (drag.kind != DRAG_NONE || editing_text()) return;

        char *now = canvas_text(0);
        if (history.current && strcmp(now, history.current) == 0) {
                free(now);
                return;
        }
        if (history.current) {
                if (history.undo.count >= UNDO_MAX) {
                        free(history.undo.items[0]);
                        Da_remove(&history.undo, 0);
                }
                Da_append(&history.undo, history.current);
        }
        history_free_list(&history.redo);
        history.current = now;
}

static void
history_step(bool undo)
{
        history_check();
        void *from    = undo ? (void *) &history.undo : (void *) &history.redo;
        void *to      = undo ? (void *) &history.redo : (void *) &history.undo;
        Da(char *) *f = from;
        Da(char *) *t = to;

        if (!f->count) return;
        Da_append(t, history.current);
        history.current = f->items[--f->count];
        canvas_read_text(history.current, READ_RESTORE);
}

/* ---- Selection ---- */

/* Run FN on every selected square. FN may remove squares. */
static void
each_selected(void (*fn)(int id, void *arg))
{
        Da(int) ids = { 0 };
        Da_foreach(sp, squares)
        {
                if ((*sp)->selected) Da_append(&ids, (*sp)->id);
        }
        Da_foreach(id, ids)
        {
                fn(*id, NULL);
        }
        Da_destroy(&ids);
}

static int
selected_count(void)
{
        int n = 0;
        Da_foreach(sp, squares)
        {
                n += (*sp)->selected;
        }
        return n;
}

/* Top-left corner of the selection */
static Vector2
selection_corner(void)
{
        Vector2 lo = { INFINITY, INFINITY };
        Da_foreach(sp, squares)
        {
                if ((*sp)->selected) lo = Vector2Min(lo, (*sp)->pos);
        }
        return lo;
}

/* Copies only live here: pasting runs the text as Lua, so the system
 * clipboard (anything copied from anywhere) is never read */
static char *clipboard;

static void
selection_copy(void)
{
        if (!selected_count()) return;
        free(clipboard);
        clipboard = canvas_text(WRITE_SELECTED);
}

static void
selection_paste(Vector2 at)
{
        if (!clipboard) return;
        paste_at = at;
        canvas_read_text(clipboard, READ_PASTE);
}

/* Copy and paste next to it, wires between them included */
static void
selection_duplicate(void)
{
        if (!selected_count()) return;
        char *text = canvas_text(WRITE_SELECTED);
        paste_at   = Vector2AddValue(selection_corner(), 2 * cfg.canvas.grid_step);
        canvas_read_text(text, READ_PASTE);
        free(text);
}

static void
history_destroy(void)
{
        history_free_list(&history.undo);
        history_free_list(&history.redo);
        Da_destroy(&history.undo);
        Da_destroy(&history.redo);
        free(history.current);
        free(clipboard);
}

static void file_save_as(void);
static void tab_mark_saved(void);

static void
bar_save(void)
{
        if (!canvas_path) {
                file_save_as(); /* never saved: ask where */
        } else if (canvas_save(canvas_path)) {
                tab_mark_saved();
                set_status("Saved %s", canvas_path);
        } else {
                set_status("Could not save %s", canvas_path);
        }
}

static void
bar_load(void)
{
        if (canvas_path && canvas_read_file(canvas_path, READ_LOAD)) {
                history_reset();
                tab_mark_saved();
                set_status("Loaded %s", canvas_path);
        } else {
                set_status("Could not load %s", canvas_path);
        }
}

static void
bar_start_stop(void)
{
        running = !running;
        if (!running) return;
        if (cfg.output.clear_on_start) output_clear();

        /* Before the first frame runs, so entry squares go first */
        for (int i = 0; i < squares.count; i++) {
                Square *s = squares.items[i];
                if (s->fn.start) s->fn.start(s);
        }
}

static void
bar_output(void)
{
        output.open   = !output.open;
        output.unseen = 0;
}

/* A * when there are lines not seen yet */
static const char *
output_label(void)
{
        return output.unseen ? "Output*" : "Output";
}

static const char *
start_stop_label(void)
{
        return running ? "Stop" : "Start";
}

static void file_menu_open(void);
enum { BAR_FILE = 2 };

/* Buttons at the right of the bar, from right to left. LABEL is used when
 * GET_LABEL is NULL. WIDEST is the longest label it can show. */
static const struct {
        const char *label;
        const char *(*get_label)(void);
        const char *widest;
        void (*fn)(void);
} bar_buttons[] = {
        { .get_label = start_stop_label, .widest = "Start", .fn = bar_start_stop },
        { .get_label = output_label, .widest = "Output*", .fn = bar_output },
        [BAR_FILE] = { .label = "File", .widest = "File", .fn = file_menu_open },
};

static Rectangle
bar_button_rect(int i)
{
        float fs  = cfg.bar.font_size;
        float pad = 4;
        float h   = cfg.bar.height - 2 * pad;
        float x   = GetScreenWidth();
        float w   = 0;

        for (int k = 0; k <= i; k++) {
                w = measure_text(bar_buttons[k].widest, fs) + 4 * pad;
                x -= w + pad;
        }
        return (Rectangle) { x, GetScreenHeight() - cfg.bar.height + pad, w, h };
}

/* Returns true if the bar consumed the input this frame. */
static bool
update_bar(void)
{
        if (!IsMouseButtonPressed(MOUSE_BUTTON_LEFT)) return false;
        for (int i = 0; i < countof(bar_buttons); i++) {
                if (CheckCollisionPointRec(GetMousePosition(), bar_button_rect(i))) {
                        menu.depth = 0; /* before: File opens one */
                        bar_buttons[i].fn();
                        return true;
                }
        }
        return false;
}

/* ---- Hot reload ---- */

static const char *config_path;
static long long config_mtime;

/* Free S, a string of cfg read with conf_str, unless it's still KEEP or
 * the default (a literal) */
static void
str_drop(const char *s, const char *keep, const char *def)
{
        if (s && s != keep && s != def) free((char *) s);
}

/* Read the config again from the defaults, so removed keys go back to
 * them. Colors, sizes and fonts change at once. */
static void
config_reload(void)
{
        Typeof(cfg) prev = cfg;

        cfg = cfg_defaults;
        if (!load_config(config_path)) {
                cfg = prev;
                set_status("Could not load %s, keeping the old config", config_path);
                return;
        }

        /* These are only used at start */
        str_drop(cfg.modules.path, prev.modules.path, cfg_defaults.modules.path);
        str_drop(cfg.save.path, prev.save.path, cfg_defaults.save.path);
        cfg.window.width  = prev.window.width;
        cfg.window.height = prev.window.height;
        cfg.modules.path  = prev.modules.path;
        cfg.save.path     = prev.save.path;

        bool fonts = strcmp(cfg.font.name, prev.font.name) || cfg.font.spacing != prev.font.spacing ||
                     cfg.bar.font_size != prev.bar.font_size || cfg.menu.font_size != prev.menu.font_size ||
                     cfg.output.font_size != prev.output.font_size ||
                     cfg.canvas.pin_font_size != prev.canvas.pin_font_size ||
                     cfg.canvas.name_font_size != prev.canvas.name_font_size || cfg.canvas.zoom_max != prev.canvas.zoom_max;
        if (fonts) {
                unload_fonts();
                load_font();
        }
        if (strcmp(cfg.window.title, prev.window.title)) SetWindowTitle(cfg.window.title);
        if (cfg.window.fps != prev.window.fps) SetTargetFPS(cfg.window.fps);
        camera.zoom = Clamp(camera.zoom, cfg.canvas.zoom_min, cfg.canvas.zoom_max);

        /* Sizes depend on fonts, spacing and field width */
        Da_foreach(sp, squares)
        {
                (*sp)->side = 0;
        }
        /* The old strings, now that nothing uses them */
        str_drop(prev.window.title, cfg.window.title, cfg_defaults.window.title);
        str_drop(prev.font.name, cfg.font.name, cfg_defaults.font.name);
        str_drop(prev.modules.source, cfg.modules.source, cfg_defaults.modules.source);
        set_status("Reloaded %s", config_path);
}

/* Every half second: the config file, and the modules */
/* ---- Tabs ---- */

/* Each tab is a canvas file and everything that goes with it: squares,
 * wires, camera, variables, lists, trees, undo and whether it's running.
 * The tab shown keeps all that in the globals, so the rest of the editor
 * doesn't know about tabs. The others keep theirs in their Tab, and
 * tab_swap trades them, like graph_swap. Tabs that are running keep running
 * when they aren't shown. */

typedef struct Tab {
        /* Traded with the globals by tab_swap */
        SquareList squares;
        LinkList links;
        Camera2D camera;
        VarList vars, globals;
        BT *lists, *trees;
        Typeof(history) history;
        bool running;
        const char *path; /* canvas_path: NULL until it's saved somewhere */

        char *saved; /* canvas_text(0) when it was last loaded or saved, to see changes */
} Tab;

static Da(Tab *) tabs;
static int tab_cur;      /* the one shown */
static bool handed_over; /* a rebuilt editor took over: quit (see Reloading) */

#define SWAP(a, b)                    \
        do {                          \
                Typeof(a) swap_ = a;  \
                a               = b;  \
                b               = swap_; \
        } while (0)

static void
tab_swap(Tab *t)
{
        SWAP(squares, t->squares);
        SWAP(links, t->links);
        SWAP(camera, t->camera);
        SWAP(vars, t->vars);
        SWAP(globals, t->globals);
        SWAP(lists, t->lists);
        SWAP(trees, t->trees);
        SWAP(history, t->history);
        SWAP(running, t->running);
        SWAP(canvas_path, t->path);
}

/* Bring tab I to the globals if it isn't there, or take it back: call it
 * twice around what has to be done on it */
static void
tab_visit(int i)
{
        if (i != tab_cur) tab_swap(tabs.items[i]);
}

static const char *
tab_name(int i)
{
        const char *path = i == tab_cur ? canvas_path : tabs.items[i]->path;
        return path ? GetFileName(path) : "untitled";
}

/* Changed since it was loaded or saved */
static bool
tab_dirty(int i)
{
        Tab *t          = tabs.items[i];
        const char *now = i == tab_cur ? history.current : t->history.current;
        return t->saved && now && strcmp(t->saved, now) != 0;
}

/* What the tab shown has now is what's saved */
static void
tab_mark_saved(void)
{
        if (!tabs.count) return;
        Tab *t = tabs.items[tab_cur];
        free(history.current);
        history.current = canvas_text(0);
        free(t->saved);
        t->saved = strdup(history.current);
}

static void
tab_title(void)
{
        SetWindowTitle(TextFormat("%s - %s", tab_name(tab_cur), cfg.window.title));
}

static void
tab_switch(int i)
{
        if (i == tab_cur || i < 0 || i >= tabs.count) return;
        panel_close();
        field_edit.square = NULL;
        drag.kind         = DRAG_NONE;
        menu.depth        = 0;
        tab_swap(tabs.items[tab_cur]);
        tab_swap(tabs.items[i]);
        tab_cur = i;
        /* Sizes may be old: the config or the fonts may have changed */
        Da_foreach(sp, squares)
        {
                (*sp)->side = 0;
        }
        tab_title();
}

/* A new empty tab, shown. Its path is PATH (copied), and what's there is
 * loaded if there is something. */
static void
tab_new(const char *path)
{
        Tab *t    = calloc(1, sizeof *t);
        t->camera = (Camera2D) { .zoom = 1.0f };
        Da_append(&tabs, t);
        /* The first one is the globals already */
        if (tabs.count > 1) tab_switch(tabs.count - 1);
        canvas_path = path ? strdup(path) : NULL;
        if (path && FileExists(path) && !canvas_read_file(path, READ_LOAD)) set_status("Could not load %s", path);
        history_reset();
        tab_mark_saved();
        tab_title();
}

static void ask_close_tab(int i);

/* Close tab I. With unsaved changes it asks first, unless FORCE. */
static void
tab_close(int i, bool force)
{
        Tab *t = tabs.items[i];
        if (!force && tab_dirty(i)) {
                ask_close_tab(i);
                return;
        }

        tab_visit(i);
        running = false;
        canvas_clear(-1, NULL);
        Da_destroy(&squares);
        Da_destroy(&links);
        history_free_list(&history.undo);
        history_free_list(&history.redo);
        Da_destroy(&history.undo);
        Da_destroy(&history.redo);
        free(history.current);
        history.current = NULL;
        free((char *) canvas_path);
        canvas_path = NULL;
        camera      = (Camera2D) { .zoom = 1.0f };
        tab_visit(i);
        /* Now the globals (if it was shown) or T have empty ones */

        free(t->saved);
        free(t);
        Da_remove(&tabs, i);
        if (i < tab_cur) {
                tab_cur--;
        } else if (i == tab_cur) {
                if (!tabs.count) {
                        tab_cur = 0;
                        tab_new(NULL); /* always one */
                        return;
                }
                /* The globals are empty: bring the next one, or the last */
                tab_cur = Min(i, tabs.count - 1);
                tab_swap(tabs.items[tab_cur]);
                Da_foreach(sp, squares)
                {
                        (*sp)->side = 0;
                }
        }
        tab_title();
}

/* Show PATH: its tab if it's open, or a new one. An untitled empty tab
 * shown goes away, it was only there to have one. */
static void
tab_open(const char *path)
{
        char want[PATH_MAX], have[PATH_MAX];
        bool known = realpath(path, want) != NULL;

        for (int i = 0; known && i < tabs.count; i++) {
                const char *p = i == tab_cur ? canvas_path : tabs.items[i]->path;
                if (p && realpath(p, have) && !strcmp(want, have)) {
                        tab_switch(i);
                        return;
                }
        }

        Tab *blank = tabs.count && !canvas_path && !squares.count && !tab_dirty(tab_cur) ? tabs.items[tab_cur] : NULL;
        tab_new(known ? want : path);
        for (int i = 0; blank && i < tabs.count; i++) {
                if (tabs.items[i] == blank) tab_close(i, true);
        }
        set_status(FileExists(path) ? "Opened %s" : "New file %s", path);
}

/* Running tabs that aren't shown go on running */
static void
tabs_run_background(void)
{
        for (int t = 0; t < tabs.count; t++) {
                if (t == tab_cur || !tabs.items[t]->running) continue;
                tab_visit(t);
                for (int i = 0; i < squares.count; i++) {
                        Square *s = squares.items[i];
                        if (s->fn.frame) s->fn.frame(s);
                }
                tab_visit(t);
        }
}

/* M was rebuilt: in every tab */
static void
module_reload_squares(Module *m)
{
        if (!tabs.count) {
                module_reload_squares_here(m);
                return;
        }
        for (int t = 0; t < tabs.count; t++) {
                tab_visit(t);
                module_reload_squares_here(m);
                tab_visit(t);
        }
}

/* "Open in a new tab" of a subgraph: the canvas it's made of. Under
 * build/modules it's a link to the one in modules/, realpath finds that. */
static void
square_open_tab(int id, void *arg)
{
        (void) arg;
        char path[PATH_MAX];
        Square *s = square_get(id);
        if (!s || !s->module->path) return;
        tab_open(realpath(s->module->path, path) ? path : s->module->path);
}

/* ---- Tab strip, above the canvas ---- */

static float
tabs_height(void)
{
        return cfg.bar.height;
}

#define TAB_CLOSE "x"
#define TAB_NEW   "+"

static float
tab_pad(void)
{
        return cfg.bar.font_size / 2.0f;
}

/* Tab I, or the + after the last one for I == tabs.count. They shrink to
 * fit when there are many. */
static Rectangle
tab_rect(int i)
{
        float fs    = cfg.bar.font_size;
        float pad   = tab_pad();
        float close = measure_text(TAB_CLOSE, fs) + 2 * pad;
        float plus  = measure_text(TAB_NEW, fs) + 4 * pad;
        float total = plus, x = 0;
        float w[tabs.count + 1];

        for (int k = 0; k < tabs.count; k++) {
                w[k] = measure_text(TextFormat("%s*", tab_name(k)), fs) + 3 * pad + close;
                total += w[k];
        }
        float scale = total > GetScreenWidth() ? (GetScreenWidth() - plus) / (total - plus) : 1;
        for (int k = 0; k < i; k++)
                x += w[k] * scale;
        return (Rectangle) { x, 0, i < tabs.count ? w[i] * scale : plus, tabs_height() };
}

static Rectangle
tab_close_rect(int i)
{
        Rectangle r = tab_rect(i);
        float cw    = measure_text(TAB_CLOSE, cfg.bar.font_size) + 2 * tab_pad();
        return (Rectangle) { r.x + r.width - cw - tab_pad() / 2, r.y, cw, r.height };
}

/* Click a tab to show it, its x or the middle button to close it, + for a
 * new one */
static bool
update_tabs(void)
{
        Vector2 mouse = GetMousePosition();
        bool left     = IsMouseButtonPressed(MOUSE_BUTTON_LEFT);
        bool middle   = IsMouseButtonPressed(MOUSE_BUTTON_MIDDLE);

        if (mouse.y >= tabs_height() || (!left && !middle)) return false;
        for (int i = 0; i < tabs.count; i++) {
                if (!CheckCollisionPointRec(mouse, tab_rect(i))) continue;
                if (middle || CheckCollisionPointRec(mouse, tab_close_rect(i)))
                        tab_close(i, false);
                else
                        tab_switch(i);
                return true;
        }
        if (left && CheckCollisionPointRec(mouse, tab_rect(tabs.count))) tab_new(NULL);
        menu.depth = 0;
        return true;
}

static void
draw_tabs(void)
{
        float fs      = cfg.bar.font_size;
        float pad     = tab_pad();
        Vector2 mouse = GetMousePosition();

        DrawRectangle(0, 0, GetScreenWidth(), tabs_height(), cfg.colors.bar);
        DrawLine(0, tabs_height(), GetScreenWidth(), tabs_height(), cfg.colors.bar_border);
        for (int i = 0; i < tabs.count; i++) {
                Rectangle r = tab_rect(i);
                Rectangle c = tab_close_rect(i);
                float ty    = r.y + (r.height - fs) / 2;

                /* The one shown joins the canvas below it */
                if (i == tab_cur)
                        DrawRectangleRec((Rectangle) { r.x, r.y, r.width, r.height + 1 }, cfg.colors.bg);
                else if (CheckCollisionPointRec(mouse, r))
                        DrawRectangleRec(r, cfg.colors.menu);
                DrawLine(r.x + r.width, r.y, r.x + r.width, r.y + r.height, cfg.colors.bar_border);

                BeginScissorMode(r.x, r.y, c.x - r.x, r.height);
                draw_text(TextFormat("%s%s", tab_name(i), tab_dirty(i) ? "*" : ""), r.x + pad, ty, fs,
                          i == tab_cur ? cfg.colors.menu_text : cfg.colors.bar_text);
                EndScissorMode();
                if (CheckCollisionPointRec(mouse, c)) DrawRectangleRec(c, cfg.colors.menu_hover);
                draw_text(TAB_CLOSE, c.x + pad, ty, fs, cfg.colors.name);
        }
        Rectangle p = tab_rect(tabs.count);
        if (CheckCollisionPointRec(mouse, p)) DrawRectangleRec(p, cfg.colors.menu);
        draw_text(TAB_NEW, p.x + 2 * pad, p.y + (p.height - fs) / 2, fs, cfg.colors.bar_text);
}

/* ---- Prompt: a path typed for Open and Save as ---- */

static struct {
        bool open;
        const char *label;
        TextInput input;
        void (*done)(const char *text);
        char matches[1024]; /* what Tab found, shown below */
} prompt;

static void
prompt_open(const char *label, const char *text, void (*done)(const char *))
{
        prompt.open       = true;
        prompt.label      = label;
        prompt.done       = done;
        prompt.matches[0] = '\0';
        text_input_set(&prompt.input, text);
        menu.depth = 0;
}

static int
str_cmp(const void *a, const void *b)
{
        return strcmp(*(char **) a, *(char **) b);
}

/* Tab: complete what's typed with the folders and .lua files it can be,
 * as far as they all agree, and list them */
static void
prompt_complete(void)
{
        TextInput *t      = &prompt.input;
        char *slash       = strrchr(t->buf, '/');
        int dir_len       = slash ? (int) (slash - t->buf) + 1 : 0;
        char *start       = t->buf + dir_len;
        char dir[PATH_MAX];
        Da(char *) found = { 0 };

        snprintf(dir, sizeof dir, "%.*s", dir_len, t->buf);
        if (!*dir) strcpy(dir, ".");
        FilePathList files = LoadDirectoryFiles(dir);
        for (unsigned i = 0; i < files.count; i++) {
                const char *name = GetFileName(files.paths[i]);
                bool is_dir      = DirectoryExists(files.paths[i]);
                if (strncmp(name, start, strlen(start)) || (*name == '.' && *start != '.')) continue;
                if (!is_dir && !IsFileExtension(name, ".lua")) continue;
                Da_append(&found, strdup(TextFormat("%s%s", name, is_dir ? "/" : "")));
        }
        UnloadDirectoryFiles(files);

        prompt.matches[0] = '\0';
        if (found.count) {
                qsort(found.items, found.count, sizeof *found.items, str_cmp);
                /* As far as they all agree */
                int same = strlen(found.items[0]);
                for (int i = 1; i < found.count; i++) {
                        int k = 0;
                        while (k < same && found.items[i][k] == found.items[0][k])
                                k++;
                        same = k;
                }
                snprintf(start, sizeof t->buf - dir_len, "%.*s", same, found.items[0]);
                t->len = strlen(t->buf);
                for (int i = 0; found.count > 1 && i < found.count; i++) {
                        size_t used = strlen(prompt.matches);
                        snprintf(prompt.matches + used, sizeof prompt.matches - used, "%s%s", i ? "  " : "", found.items[i]);
                }
        } else {
                snprintf(prompt.matches, sizeof prompt.matches, "nothing there");
        }
        Da_foreach(f, found)
        {
                free(*f);
        }
        Da_destroy(&found);
}

static bool
update_prompt(void)
{
        bool changed = false;
        if (!prompt.open) return false;
        TextResult tr = text_input_update(&prompt.input, &changed);
        if (changed) prompt.matches[0] = '\0';
        if (tr == TEXT_NEXT) prompt_complete();
        if (tr == TEXT_CANCEL) prompt.open = false;
        if (tr == TEXT_DONE && prompt.input.len) {
                prompt.open = false;
                prompt.done(prompt.input.buf);
        }
        return true; /* nothing else while it's open */
}

static Rectangle
prompt_rect(void)
{
        float w = fminf(640, GetScreenWidth() - 32);
        return (Rectangle) { (GetScreenWidth() - w) / 2, tabs_height() + 16, w, menu_item_height() };
}

static void
draw_prompt(void)
{
        if (!prompt.open) return;
        Rectangle r    = prompt_rect();
        float fs       = cfg.menu.font_size;
        float pad      = cfg.menu.padding;
        float lw       = measure_text(prompt.label, fs) + 2 * pad;
        const char *tx = TextFormat("%s%s", prompt.input.buf, fmod(GetTime(), 1.0) < 0.5 ? "_" : "");

        DrawRectangleRec(r, cfg.colors.field);
        DrawRectangleRec((Rectangle) { r.x, r.y, lw + pad, r.height }, cfg.colors.menu);
        DrawRectangleLinesEx(r, 1.0f, cfg.colors.menu_border);
        draw_text(prompt.label, r.x + pad, r.y + pad, fs, cfg.colors.menu_text);
        /* The end of it if it's long */
        float tw = measure_text(tx, fs), room = r.width - lw - 3 * pad;
        BeginScissorMode(r.x + lw + pad, r.y, r.width - lw - pad, r.height);
        draw_text(tx, r.x + lw + 2 * pad - fmaxf(0, tw - room), r.y + pad, fs, cfg.colors.square_text);
        EndScissorMode();

        const char *hint = *prompt.matches ? prompt.matches : "Tab completes, Enter accepts, Esc cancels";
        Rectangle h      = { r.x, r.y + r.height, r.width, menu_item_height() };
        DrawRectangleRec(h, cfg.colors.menu);
        DrawRectangleLinesEx(h, 1.0f, cfg.colors.menu_border);
        BeginScissorMode(h.x, h.y, h.width, h.height);
        draw_text(hint, h.x + pad, h.y + pad, fs, cfg.colors.name);
        EndScissorMode();
}

/* ---- File menu ---- */

static void
file_open_done(const char *path)
{
        tab_open(path);
}

static void
file_save_as_done(const char *path)
{
        free((char *) canvas_path);
        canvas_path = strdup(path);
        bar_save();
        tab_title();
}

/* Where Open starts: the folder of the file shown */
static const char *
file_dir(void)
{
        static char dir[PATH_MAX];
        if (!canvas_path || !strchr(canvas_path, '/')) return "";
        snprintf(dir, sizeof dir, "%s/", GetDirectoryPath(canvas_path));
        return dir;
}

static void
file_new(void)
{
        tab_new(NULL);
}

static void
file_open(void)
{
        prompt_open("Open", file_dir(), file_open_done);
}

static void
file_save_as(void)
{
        prompt_open("Save as", canvas_path ? canvas_path : file_dir(), file_save_as_done);
}

static void
file_close(void)
{
        tab_close(tab_cur, false);
}

static void
file_next_tab(int step)
{
        tab_switch((tab_cur + step + tabs.count) % tabs.count);
}

#define FILE_ITEM(fn) \
        static void fn##_item(int id, void *arg) { (void) id, (void) arg, fn(); }
FILE_ITEM(file_new)
FILE_ITEM(file_open)
FILE_ITEM(bar_save)
FILE_ITEM(file_save_as)
FILE_ITEM(bar_load)
FILE_ITEM(file_close)
#undef FILE_ITEM

static Menu file_menu;

/* The bar button: the menu, above it */
static void
file_menu_open(void)
{
        static const MenuItem items[] = {
                { .label = "New tab", .hint = "Ctrl+T", .fn = file_new_item },
                { .label = "Open...", .hint = "Ctrl+O", .fn = file_open_item },
                { .label = "Save", .hint = "Ctrl+S", .fn = bar_save_item },
                { .label = "Save as...", .hint = "Ctrl+Shift+S", .fn = file_save_as_item },
                { .label = "Revert to saved", .fn = bar_load_item },
                { .label = "Close tab", .hint = "Ctrl+W", .fn = file_close_item },
        };
        file_menu.count = 0;
        for (int i = 0; i < countof(items); i++)
                Da_append(&file_menu, items[i]);
        Rectangle b = bar_button_rect(BAR_FILE);
        menu_open(&file_menu, -1, (Vector2) { b.x, b.y - countof(items) * menu_item_height() });
}

/* ---- Make subgraph: the selection becomes a module ---- */

/* Like "extract function": the selected squares and the wires between them
 * go to a new subgraph (a .lua under modules.source, linked into the
 * modules dir so it shows up now), and one square of it takes their place.
 * Each output outside that feeds the selection becomes a graph/input, and
 * each output inside that is used outside a graph/output, of the type of
 * that output, so the wires around can be plugged to the new square as
 * they were. One output feeding several squares inside is one input. */

typedef struct Port {
        int sq, pin; /* the output: outside for inputs, inside for outputs */
        char name[64];
        Li_Type type;
        float y;      /* to keep them in the order they were, top to bottom */
        Square *node; /* its graph/input or graph/output in the subgraph */
} Port;

typedef Da(Port) PortList;

/* The port of output SQ.PIN, added if new */
static Port *
port_for(PortList *l, int sq, int pin)
{
        Da_foreach(p, *l)
        {
                if (p->sq == sq && p->pin == pin) return p;
        }
        Square *s  = square_get(sq);
        PinInfo *o = pin_info(s, PART_OUTPUT, pin);
        Port p     = { .sq = sq, .pin = pin, .type = o ? o->type : LI_ANY, .y = 1e9 };

        /* Named after the output, and unique among the ports of this side */
        const char *base = o && *o->name ? o->name : "value";
        snprintf(p.name, sizeof p.name, "%s", base);
        for (int k = 2;; k++) {
                bool used = false;
                Da_foreach(q, *l)
                {
                        used = used || !strcmp(q->name, p.name);
                }
                if (!used) break;
                snprintf(p.name, sizeof p.name, "%s_%d", base, k);
        }
        Da_append(l, p);
        return &l->items[l->count - 1];
}

/* Index of the pin of L named NAME, -1 if none is */
static int
pin_named(PinList *l, const char *name)
{
        for (int i = 0; i < l->count; i++)
                if (!strcmp(l->items[i].name, name)) return i;
        return -1;
}

/* A graph/input or graph/output square for P at X, in the canvas there is */
static Square *
port_node(const char *module, Port *p, float x)
{
        Module *m = module_find(module);
        Square *s = m ? square_new(m, (Vector2) { x, p->y }) : NULL;
        if (!s) return NULL;
        param_set_text(s, 0, p->name);
        int option = 1; /* any: what the type isn't one of these */
        for (int i = 0; i < countof(port_types); i++)
                if (port_types[i] == p->type) option = i;
        s->inputs.items[0].value = li_int(option);
        s->fn.draw(s); /* the pin takes the type */
        return s;
}

static int
port_y_cmp(const void *a, const void *b)
{
        float x = ((Port *) a)->y, y = ((Port *) b)->y;
        return (x > y) - (x < y);
}

/* Spread the ports of L at least a square apart, in order */
static void
ports_spread(PortList *l)
{
        if (l->count) qsort(l->items, l->count, sizeof *l->items, port_y_cmp);
        for (int i = 1; i < l->count; i++)
                l->items[i].y = fmaxf(l->items[i].y, l->items[i - 1].y + cfg.canvas.square_size * 1.5f);
}

/* Write the subgraph of the selection to PATH. INS and OUTS are its ports. */
static bool
subgraph_write(const char *path, PortList *ins, PortList *outs, Rectangle box)
{
        char *text = canvas_text(WRITE_SELECTED | WRITE_STATE);
        Graph g    = { 0 };
        Camera2D outer = camera;
        bool ok;

        /* Made in a canvas of its own, like the ones inside subgraphs */
        graph_swap(&g);
        canvas_read_text(text, READ_GRAPH);
        free(text);
        float gap = cfg.canvas.square_size * 2.5f;
        Da_foreach(p, *ins)
        {
                p->node = port_node("graph/input", p, box.x - gap);
        }
        Da_foreach(p, *outs)
        {
                p->node = port_node("graph/output", p, box.x + box.width + gap);
        }
        /* The wires inside to and from them: the ids are the same here */
        LinkList outside = g.links; /* graph_swap put the canvas's there */
        Da_foreach(k, outside)
        {
                Square *a = square_get(k->from), *b = square_get(k->to);
                Da_foreach(p, *ins)
                {
                        if (p->node && !a && b && k->from == p->sq && k->from_pin == p->pin) link_add(p->node, 0, b, k->to_pin);
                }
                Da_foreach(p, *outs)
                {
                        if (p->node && a && !b && k->from == p->sq && k->from_pin == p->pin)
                                link_add(a, k->from_pin, p->node, PORT_OUT_VALUE);
                }
        }
        /* Ports may be linked once each, but several outside squares can use one output */
        for (int i = 0; i < links.count; i++)
                for (int j = links.count - 1; j > i; j--)
                        if (!memcmp(&links.items[i], &links.items[j], offsetof(Link, flash))) Da_remove(&links, j);

        camera = (Camera2D) { .target = { box.x + box.width / 2, box.y + box.height / 2 }, .offset = { cfg.window.width / 2.0f, cfg.window.height / 2.0f }, .zoom = 1 };
        FILE *f = fopen(path, "w");
        ok      = f != NULL;
        if (f) {
                fprintf(f, "-- lineit canvas, a subgraph made from a selection\n");
                canvas_write(f, WRITE_CAMERA | WRITE_STATE);
                ok = fclose(f) == 0;
        }
        camera = outer;

        Da_foreach(sp, squares)
        {
                square_free(*sp);
        }
        Da_destroy(&squares);
        Da_destroy(&links);
        var_list_clear(&vars);
        containers_clear();
        graph_swap(&g);
        return ok;
}

/* mkdir -p of the folder of PATH */
static void
mkdir_parents(const char *path)
{
        char dir[PATH_MAX];
        snprintf(dir, sizeof dir, "%s", path);
        for (char *p = dir + 1; *p; p++) {
                if (*p != '/') continue;
                *p = '\0';
                mkdir(dir, 0755);
                *p = '/';
        }
}

static void
subgraph_make(const char *typed)
{
        char path[PATH_MAX], name[PATH_MAX], link[PATH_MAX], abs[PATH_MAX];
        const char *src = cfg.modules.source;
        size_t slen     = strlen(src);
        PortList ins = { 0 }, outs = { 0 };

        snprintf(path, sizeof path, "%s%s", typed, IsFileExtension(typed, ".lua") ? "" : ".lua");
        if (strncmp(path, src, slen) || path[slen] != '/') {
                set_status("Subgraphs go in %s/ (modules.source in the config)", src);
                return;
        }
        if (FileExists(path)) {
                set_status("%s already exists", path);
                return;
        }
        /* "modules/my/thing.lua" -> "my/thing" */
        snprintf(name, sizeof name, "%s", path + slen + 1);
        *strrchr(name, '.') = '\0';
        if (module_find(name)) {
                set_status("There is a module %s already", name);
                return;
        }

        /* Where it is, and the wires that cross it */
        Vector2 lo = { 1e9, 1e9 }, hi = { -1e9, -1e9 };
        Da_foreach(sp, squares)
        {
                if (!(*sp)->selected) continue;
                Rectangle r = square_rect(*sp);
                lo          = Vector2Min(lo, (Vector2) { r.x, r.y });
                hi          = Vector2Max(hi, (Vector2) { r.x + r.width, r.y + r.height });
        }
        Rectangle box = { lo.x, lo.y, hi.x - lo.x, hi.y - lo.y };

        Da_foreach(k, links)
        {
                Square *a = square_get(k->from), *b = square_get(k->to);
                if (!a || !b || a->selected == b->selected) continue;
                Port *p = b->selected ? port_for(&ins, k->from, k->from_pin) : port_for(&outs, k->from, k->from_pin);
                /* In the order of what they plug into inside */
                Square *in = b->selected ? b : a;
                p->y       = fminf(p->y, in->pos.y);
        }
        ports_spread(&ins);
        ports_spread(&outs);

        mkdir_parents(path);
        if (!subgraph_write(path, &ins, &outs, box)) {
                set_status("Could not write %s", path);
                goto done;
        }
        /* Into the modules dir, where it's looked for. A link, like make does. */
        if (strcmp(src, modules_dir) != 0) {
                snprintf(link, sizeof link, "%s/%s.lua", modules_dir, name);
                mkdir_parents(link);
                if (!realpath(path, abs) || symlink(abs, link) != 0) {
                        set_status("Wrote %s, but could not link it to %s", path, link);
                        goto done;
                }
        }
        modules_scan(false); /* now, not when it settles */
        Module *m = module_find(name);
        if (!m) {
                set_status("Wrote %s, but it doesn't load", path);
                goto done;
        }

        /* Its square where the selection was, plugged as it was */
        LinkList old = { 0 };
        Da_foreach(k, links)
        {
                Da_append(&old, *k);
        }
        Vector2 center = { box.x + box.width / 2, box.y + box.height / 2 };
        each_selected(square_delete);
        Square *s = square_new(m, center);
        if (s) {
                Da_foreach(k, old)
                {
                        Square *a = square_get(k->from), *b = square_get(k->to);
                        Da_foreach(p, ins)
                        {
                                if (k->from == p->sq && k->from_pin == p->pin && a && !b) link_add(a, k->from_pin, s, pin_named(&s->inputs, p->name));
                        }
                        Da_foreach(p, outs)
                        {
                                if (k->from == p->sq && k->from_pin == p->pin && !a && b) link_add(s, pin_named(&s->outputs, p->name), b, k->to_pin);
                        }
                }
                /* Several inside squares fed by one output: one wire now */
                for (int i = 0; i < links.count; i++)
                        for (int j = links.count - 1; j > i; j--)
                                if (!memcmp(&links.items[i], &links.items[j], offsetof(Link, flash))) Da_remove(&links, j);
                select_all(false);
                s->selected = true;
        }
        Da_destroy(&old);
        set_status("Made %s, in %s", name, path);
done:
        Da_destroy(&ins);
        Da_destroy(&outs);
}

static void
subgraph_ask(void)
{
        if (!selected_count()) return;
        prompt_open("Subgraph", TextFormat("%s/", cfg.modules.source), subgraph_make);
}

static void
menu_subgraph(int id, void *arg)
{
        (void) id, (void) arg;
        subgraph_ask();
}

/* ---- Unsaved changes: asked before they are lost ---- */

/* Closing a tab or the window with unsaved changes asks: Save or Don't
 * save. The x at its top right (or Esc) leaves everything as it was.
 * Saving an untitled one asks where first (Save as). */

static bool quitting; /* the editor closes after this frame */

static struct {
        bool open;
        Tab *tab; /* the one closing, NULL: the window */
        char text[256];
} ask;

enum { ASK_SAVE,
       ASK_DONT_SAVE,
       ASK_BUTTONS,
       ASK_DISMISS }; /* the x */
static const char *ask_labels[ASK_BUTTONS] = { "Save", "Don't save" };

static int
tab_index(Tab *t)
{
        for (int i = 0; i < tabs.count; i++)
                if (tabs.items[i] == t) return i;
        return -1;
}

/* Save tab I where it was, false if it has no path or it can't */
static bool
tab_save(int i)
{
        Tab *t = tabs.items[i];
        tab_visit(i);
        bool ok = canvas_path && canvas_save(canvas_path);
        if (ok) {
                free(history.current);
                history.current = canvas_text(0);
                free(t->saved);
                t->saved = strdup(history.current);
        }
        if (!ok && canvas_path) set_status("Could not save %s", canvas_path);
        tab_visit(i);
        return ok;
}

static void
ask_close_tab(int i)
{
        tab_switch(i); /* show what it's about */
        ask.open = true;
        ask.tab  = tabs.items[i];
        snprintf(ask.text, sizeof ask.text, "%s has unsaved changes", tab_name(i));
}

static void quit_request(void);

static void
save_as_then_close(const char *path)
{
        file_save_as_done(path);
        if (!tab_dirty(tab_cur)) tab_close(tab_cur, true);
}

static void
save_as_then_quit(const char *path)
{
        file_save_as_done(path);
        if (!tab_dirty(tab_cur)) quit_request();
}

/* The window is closing: ask if something isn't saved */
static void
quit_request(void)
{
        char names[200] = "";
        int n = 0;
        for (int i = 0; i < tabs.count; i++) {
                if (!tab_dirty(i)) continue;
                size_t used = strlen(names);
                snprintf(names + used, sizeof names - used, "%s%s", n++ ? ", " : "", tab_name(i));
        }
        if (!n) {
                quitting = true;
                return;
        }
        ask.open = true;
        ask.tab  = NULL;
        snprintf(ask.text, sizeof ask.text, "Unsaved changes in %s", names);
}

static void
ask_answer(int button)
{
        ask.open = false;
        if (button == ASK_DISMISS) return;

        if (ask.tab) {
                int i = tab_index(ask.tab);
                if (i < 0) return;
                if (button == ASK_DONT_SAVE) {
                        tab_close(i, true);
                } else if (!canvas_path) {
                        prompt_open("Save as", file_dir(), save_as_then_close);
                } else if (tab_save(i)) {
                        tab_close(i, true);
                }
                return;
        }

        if (button == ASK_DONT_SAVE) {
                quitting = true;
                return;
        }
        /* Save all that have a path, then ask where for the first untitled */
        for (int i = 0; i < tabs.count; i++) {
                const char *path = i == tab_cur ? canvas_path : tabs.items[i]->path;
                if (tab_dirty(i) && path && !tab_save(i)) return;
        }
        for (int i = 0; i < tabs.count; i++) {
                if (!tab_dirty(i)) continue;
                tab_switch(i);
                prompt_open("Save as", file_dir(), save_as_then_quit);
                return;
        }
        quitting = true;
}

static Rectangle
ask_rect(void)
{
        float w = fminf(fmaxf(measure_text(ask.text, cfg.menu.font_size) + 6 * cfg.menu.padding, 460), GetScreenWidth() - 32);
        float h = 3 * menu_item_height() + 2 * cfg.menu.padding;
        return (Rectangle) { (GetScreenWidth() - w) / 2, (GetScreenHeight() - h) / 2, w, h };
}

/* Right to left, at the bottom of it */
static Rectangle
ask_button_rect(int b)
{
        Rectangle r = ask_rect();
        float pad   = cfg.menu.padding;
        float x     = r.x + r.width - pad;
        float w     = 0;
        for (int k = ASK_BUTTONS - 1; k >= b; k--) {
                w = measure_text(ask_labels[k], cfg.menu.font_size) + 4 * pad;
                x -= w + pad;
        }
        return (Rectangle) { x + pad, r.y + r.height - menu_item_height() - pad, w, menu_item_height() };
}

/* The x, at the top right */
static Rectangle
ask_dismiss_rect(void)
{
        Rectangle r = ask_rect();
        float side  = menu_item_height();
        return (Rectangle) { r.x + r.width - side, r.y, side, side };
}

/* Enter saves, Esc is the x */
static bool
update_ask(void)
{
        if (!ask.open) return false;
        Vector2 mouse = GetMousePosition();
        if (IsKeyPressed(KEY_ENTER) || IsKeyPressed(KEY_KP_ENTER)) {
                ask_answer(ASK_SAVE);
        } else if (IsKeyPressed(KEY_ESCAPE)) {
                ask_answer(ASK_DISMISS);
        } else if (IsMouseButtonPressed(MOUSE_BUTTON_LEFT)) {
                if (CheckCollisionPointRec(mouse, ask_dismiss_rect())) ask_answer(ASK_DISMISS);
                for (int b = 0; ask.open && b < ASK_BUTTONS; b++)
                        if (CheckCollisionPointRec(mouse, ask_button_rect(b))) ask_answer(b);
        }
        return true; /* nothing else while it's open */
}

static void
draw_ask(void)
{
        if (!ask.open) return;
        Rectangle r = ask_rect();
        float pad   = cfg.menu.padding;
        float fs    = cfg.menu.font_size;

        DrawRectangle(0, 0, GetScreenWidth(), GetScreenHeight(), Fade(BLACK, 0.4f));
        DrawRectangleRec(r, cfg.colors.menu);
        DrawRectangleLinesEx(r, 1.0f, cfg.colors.menu_border);
        draw_text(ask.text, r.x + 3 * pad, r.y + 2 * pad, fs, cfg.colors.menu_text);
        draw_text(ask.tab ? "Save it before closing it?" : "Save before closing lineit?", r.x + 3 * pad,
                  r.y + 2 * pad + menu_item_height() * 0.8f, fs, cfg.colors.name);
        for (int b = 0; b < ASK_BUTTONS; b++) {
                Rectangle br = ask_button_rect(b);
                bool hover   = CheckCollisionPointRec(GetMousePosition(), br);
                DrawRectangleRec(br, hover || b == ASK_SAVE ? cfg.colors.menu_hover : cfg.colors.bar);
                DrawRectangleLinesEx(br, 1.0f, cfg.colors.menu_border);
                draw_text(ask_labels[b], br.x + (br.width - measure_text(ask_labels[b], fs)) / 2, br.y + pad, fs,
                          cfg.colors.menu_text);
        }
        Rectangle x = ask_dismiss_rect();
        if (CheckCollisionPointRec(GetMousePosition(), x)) DrawRectangleRec(x, cfg.colors.menu_hover);
        draw_text(TAB_CLOSE, x.x + (x.width - measure_text(TAB_CLOSE, fs)) / 2, x.y + pad, fs, cfg.colors.name);
}

/* ---- One editor: ./lineit --tab FILE opens FILE in the one running ---- */

static int tab_socket = -1; /* listening, if this is the one */
static bool tab_socket_owned;
static char tab_socket_path[108]; /* sun_path */

static void
tab_socket_name(void)
{
        const char *dir = getenv("XDG_RUNTIME_DIR");
        if (dir && *dir)
                snprintf(tab_socket_path, sizeof tab_socket_path, "%s/lineit.sock", dir);
        else
                snprintf(tab_socket_path, sizeof tab_socket_path, "/tmp/lineit-%d.sock", (int) getuid());
}

static int
tab_socket_connect(void)
{
        struct sockaddr_un a = { .sun_family = AF_UNIX };
        int fd               = socket(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0);
        snprintf(a.sun_path, sizeof a.sun_path, "%s", tab_socket_path);
        if (fd >= 0 && connect(fd, (struct sockaddr *) &a, sizeof a) == 0) return fd;
        if (fd >= 0) close(fd);
        return -1;
}

/* Give PATH to the editor running, if there is one. False if there isn't. */
static bool
tab_socket_send(const char *path)
{
        char abs[2 * PATH_MAX], cwd[PATH_MAX];
        tab_socket_name();
        int fd = tab_socket_connect();
        if (fd < 0) return false;
        /* Absolute: it runs somewhere else. It may not exist yet. */
        if (!realpath(path, abs)) {
                if (*path == '/' || !getcwd(cwd, sizeof cwd))
                        snprintf(abs, sizeof abs, "%s", path);
                else
                        snprintf(abs, sizeof abs, "%s/%s", cwd, path);
        }
        size_t len = strlen(abs);
        bool ok    = write(fd, abs, len) == (ssize_t) len && write(fd, "\n", 1) == 1;
        close(fd);
        return ok;
}

/* Listen, unless another editor does. After a reload the old one hands
 * its socket over (LINEIT_SOCKET_FD). */
static void
tab_socket_listen(void)
{
        const char *fd = getenv("LINEIT_SOCKET_FD");
        tab_socket_name();
        if (fd) {
                tab_socket       = atoi(fd);
                tab_socket_owned = true;
                unsetenv("LINEIT_SOCKET_FD");
                return;
        }

        int other = tab_socket_connect();
        if (other >= 0) {
                close(other);
                return;
        }
        struct sockaddr_un a = { .sun_family = AF_UNIX };
        snprintf(a.sun_path, sizeof a.sun_path, "%s", tab_socket_path);
        unlink(tab_socket_path); /* left by one that crashed */
        tab_socket = socket(AF_UNIX, SOCK_STREAM | SOCK_NONBLOCK, 0);
        if (tab_socket < 0 || bind(tab_socket, (struct sockaddr *) &a, sizeof a) != 0 || listen(tab_socket, 8) != 0) {
                if (tab_socket >= 0) close(tab_socket);
                tab_socket = -1;
                return;
        }
        tab_socket_owned = true;
}

/* Paths sent by ./lineit --tab, one per line */
static void
tab_socket_poll(void)
{
        int c;
        if (tab_socket < 0) return;
        while ((c = accept(tab_socket, NULL, NULL)) >= 0) {
                char buf[PATH_MAX + 1];
                size_t len   = 0;
                ssize_t n    = 0;
                struct timeval tv = { .tv_usec = 200000 };
                setsockopt(c, SOL_SOCKET, SO_RCVTIMEO, &tv, sizeof tv);
                while (len < sizeof buf - 1 && (n = read(c, buf + len, sizeof buf - 1 - len)) > 0)
                        len += n;
                close(c);
                buf[len] = '\0';
                for (char *save, *line = strtok_r(buf, "\n", &save); line; line = strtok_r(NULL, "\n", &save))
                        tab_open(line);
                if (len) SetWindowFocused();
        }
}

static void
tab_socket_close(void)
{
        if (tab_socket < 0) return;
        close(tab_socket);
        /* After a reload the new one has it */
        if (tab_socket_owned && !handed_over) unlink(tab_socket_path);
        tab_socket = -1;
}

static bool
prompt_is_open(void)
{
        return prompt.open;
}

/* ---- Reloading the editor itself ---- */

/* When the lineit binary is rebuilt, the editor hands over to the new one:
 * it writes everything to a session file (the canvas with its camera and
 * module state, plus the window, the selection, the output, undo and redo,
 * for every tab), and the socket of --tab,
 * starts the new binary with --restore on it, and waits for it to say it's
 * drawing. Then this one quits, and the new one carries on where it left. If
 * the new one crashes or doesn't answer, this one stays, and tries again when
 * the binary changes again. Running canvases are started again: variables
 * and lists are not in the session. */

static char exe_path[PATH_MAX]; /* this binary, as it was at start */
static long long exe_mtime;
static int ready_fd = -1;     /* in the new one: where to say it's ready */
#define HANDOVER_TIMEOUT_MS 10000
#define READY_FRAMES        3 /* drawn before saying it's ready */

static void
exe_watch(void)
{
        ssize_t n = readlink("/proc/self/exe", exe_path, sizeof exe_path - 1);
        exe_path[n > 0 ? n : 0] = '\0';
        exe_mtime               = file_mtime(exe_path);

        /* Started by the old one: tell it once drawing works */
        const char *fd = getenv("LINEIT_READY_FD");
        if (fd) {
                ready_fd = atoi(fd);
                unsetenv("LINEIT_READY_FD");
        }
}

/* Call once per frame drawn */
static void
exe_ready_check(void)
{
        static int frames;
        if (ready_fd < 0 || ++frames < READY_FRAMES) return;
        if (write(ready_fd, "k", 1) != 1) perror("lineit: ready");
        close(ready_fd);
        ready_fd = -1;
}

static void
session_write_list(FILE *f, const char *name, void *list)
{
        Da(char *) *l = list;
        fprintf(f, "        %s = {\n", name);
        Da_foreach(t, *l)
        {
                fprintf(f, "                ");
                lua_quote(f, *t);
                fprintf(f, ",\n");
        }
        fprintf(f, "        },\n");
}

static bool
session_write(const char *path)
{
        FILE *f = fopen(path, "w");
        if (!f) return false;
        Vector2 win = GetWindowPosition();

        fprintf(f, "-- lineit session, see --restore\nsession = {\n");
        fprintf(f, "        active = %d,\n", tab_cur + 1);
        fprintf(f, "        output_open = %s,\n", output.open ? "true" : "false");
        fprintf(f, "        window = { x = %d, y = %d, width = %d, height = %d },\n",
                (int) win.x, (int) win.y, GetScreenWidth(), GetScreenHeight());
        fprintf(f, "        output = {\n");
        Da_foreach(l, output.lines)
        {
                fprintf(f, "                { info = %s, text = ", l->info ? "true" : "false");
                lua_quote(f, l->text);
                fprintf(f, " },\n");
        }
        fprintf(f, "        },\n        tabs = {\n");
        for (int i = 0; i < tabs.count; i++) {
                tab_visit(i);
                char *canvas = canvas_text(WRITE_CAMERA | WRITE_STATE);
                fprintf(f, "        {\n");
                if (canvas_path) {
                        fprintf(f, "        path = ");
                        lua_quote(f, canvas_path);
                        fprintf(f, ",\n");
                }
                if (tabs.items[i]->saved) {
                        fprintf(f, "        saved = ");
                        lua_quote(f, tabs.items[i]->saved);
                        fprintf(f, ",\n");
                }
                fprintf(f, "        running = %s,\n        canvas = ", running ? "true" : "false");
                lua_quote(f, canvas);
                fprintf(f, ",\n        selected = {");
                Da_foreach(sp, squares)
                {
                        if ((*sp)->selected) fprintf(f, " %d,", (*sp)->id);
                }
                fprintf(f, " },\n");
                session_write_list(f, "undo", &history.undo);
                session_write_list(f, "redo", &history.redo);
                fprintf(f, "        },\n");
                free(canvas);
                tab_visit(i);
        }
        fprintf(f, "        },\n}\n");
        return fclose(f) == 0;
}

/* Start the new binary on a session of this one. True once it's running:
 * then this one has to quit. */
static bool
exe_handover(void)
{
        char session[PATH_MAX];
        const char *tmp = getenv("TMPDIR");
        int fds[2];

        snprintf(session, sizeof session, "%s/lineit-session-XXXXXX", tmp && *tmp ? tmp : "/tmp");
        int sfd = mkstemp(session);
        if (sfd < 0) return false;
        close(sfd);
        if (!session_write(session) || pipe(fds) != 0) {
                remove(session);
                return false;
        }
        fcntl(fds[0], F_SETFD, FD_CLOEXEC);

        fflush(stdout), fflush(stderr);
        pid_t pid = fork();
        if (pid == 0) {
                char fd[16];
                snprintf(fd, sizeof fd, "%d", fds[1]);
                setenv("LINEIT_READY_FD", fd, 1);
                if (tab_socket >= 0) {
                        snprintf(fd, sizeof fd, "%d", tab_socket);
                        setenv("LINEIT_SOCKET_FD", fd, 1);
                }
                execl(exe_path, exe_path, "--config", config_path, "--restore", session, (char *) NULL);
                perror("lineit: exec");
                _exit(127);
        }
        close(fds[1]);
        if (pid < 0) {
                close(fds[0]);
                remove(session);
                return false;
        }

        /* This window doesn't draw meanwhile: it's the old picture anyway */
        struct pollfd p = { .fd = fds[0], .events = POLLIN };
        char c;
        bool ok = poll(&p, 1, HANDOVER_TIMEOUT_MS) == 1 && read(fds[0], &c, 1) == 1;
        close(fds[0]);
        if (!ok) {
                kill(pid, SIGKILL);
                waitpid(pid, NULL, 0);
                remove(session); /* the new one removes it when it reads it */
        }
        return ok;
}

/* The binary changed: hand over to it */
static void
exe_reload(void)
{
        long long mtime = file_mtime(exe_path);
        if (mtime < 0 || mtime == exe_mtime || now_ns() - mtime < MODULE_SETTLE_NS) return;
        exe_mtime = mtime; /* a broken build is tried again when it changes */
        if (exe_handover())
                handed_over = true;
        else
                set_status("The new lineit didn't start, keeping this one");
}

static void
session_read_list(Conf conf, const char *key, void *list)
{
        Da(char *) *l = list;
        const char *str;
        int n = 0;
        Conf_get_len(conf, &n, "%s", key);
        for (int i = 1; i <= n; i++) {
                if (Conf_get_str(conf, &str, "%s.%d", key, i) == CONF_OK) Da_append(l, strdup(str));
        }
}

/* Before the window opens: its size */
static void
session_window(const char *path)
{
        Conf conf;
        if (Conf_open(&conf, path) != CONF_OK) return;
        Conf_get_int(conf, &cfg.window.width, "session.window.width");
        Conf_get_int(conf, &cfg.window.height, "session.window.height");
        Conf_close(conf);
}

/* Everything else, once the window and the modules are there, into the
 * tab there is: the first one */
static bool
session_restore(const char *path)
{
        Conf conf;
        const char *str;
        int b = 0, n = 0, x = 0, y = 0, active = 1;

        if (Conf_open(&conf, path) != CONF_OK) return false;
        if (Conf_get_int(conf, &x, "session.window.x") == CONF_OK && Conf_get_int(conf, &y, "session.window.y") == CONF_OK)
                SetWindowPosition(x, y);

        Conf_get_len(conf, &n, "session.output");
        for (int i = 1; i <= n; i++) {
                int info = 0;
                Conf_get_bool(conf, &info, "session.output.%d.info", i);
                if (Conf_get_str(conf, &str, "session.output.%d.text", i) == CONF_OK) output_add(str, info);
        }
        if (Conf_get_bool(conf, &b, "session.output_open") == CONF_OK) output.open = b;
        output.unseen = 0;

        int ntabs = 0;
        Conf_get_len(conf, &ntabs, "session.tabs");
        Conf_get_int(conf, &active, "session.active");
        for (int t = 1; t <= ntabs; t++) {
                char key[64];
                if (t > 1) tab_new(NULL);

                if (Conf_get_str(conf, &str, "session.tabs.%d.canvas", t) == CONF_OK) {
                        char *canvas = strdup(str);
                        canvas_read_text(canvas, READ_LOAD);
                        free(canvas);
                }
                free((char *) canvas_path);
                canvas_path = Conf_get_str(conf, &str, "session.tabs.%d.path", t) == CONF_OK ? strdup(str) : NULL;

                n = 0;
                Conf_get_len(conf, &n, "session.tabs.%d.selected", t);
                for (int i = 1; i <= n; i++) {
                        int id    = -1;
                        Square *s = Conf_get_int(conf, &id, "session.tabs.%d.selected.%d", t, i) == CONF_OK ? square_get(id) : NULL;
                        if (s) s->selected = true;
                }

                history_reset();
                snprintf(key, sizeof key, "session.tabs.%d.undo", t);
                session_read_list(conf, key, &history.undo);
                snprintf(key, sizeof key, "session.tabs.%d.redo", t);
                session_read_list(conf, key, &history.redo);
                Tab *tab = tabs.items[tab_cur];
                free(tab->saved);
                tab->saved = Conf_get_str(conf, &str, "session.tabs.%d.saved", t) == CONF_OK ? strdup(str) : NULL;

                b = 0;
                Conf_get_bool(conf, &b, "session.tabs.%d.running", t);
                if (b) bar_start_stop();
        }

        /* From an editor without tabs: one canvas, at the top */
        if (!ntabs) {
                canvas_read(conf, READ_LOAD);
                if (Conf_get_str(conf, &str, "session.canvas_path") == CONF_OK) {
                        free((char *) canvas_path);
                        canvas_path = strdup(str);
                }
                n = 0;
                Conf_get_len(conf, &n, "session.selected");
                for (int i = 1; i <= n; i++) {
                        int id    = -1;
                        Square *s = Conf_get_int(conf, &id, "session.selected.%d", i) == CONF_OK ? square_get(id) : NULL;
                        if (s) s->selected = true;
                }
                history_reset();
                session_read_list(conf, "session.undo", &history.undo);
                session_read_list(conf, "session.redo", &history.redo);
                b = 0;
                Conf_get_bool(conf, &b, "session.running");
                if (b) bar_start_stop();
        }

        Conf_close(conf);
        remove(path);
        tab_switch(active - 1);
        tab_title();
        return true;
}

static void
hot_reload(void)
{
        static double last;
        if (GetTime() - last < 0.5) return;
        last = GetTime();

        long long mtime = file_mtime(config_path);
        if (mtime != config_mtime) {
                config_mtime = mtime;
                if (mtime >= 0) config_reload();
        }
        modules_scan(true);
        exe_reload();
}

/* Keys that act on the whole canvas. Only when no menu is open and nothing
 * is being typed. */
static void
update_shortcuts(void)
{
        bool ctrl  = IsKeyDown(KEY_LEFT_CONTROL) || IsKeyDown(KEY_RIGHT_CONTROL);
        bool shift = IsKeyDown(KEY_LEFT_SHIFT) || IsKeyDown(KEY_RIGHT_SHIFT);

        if (editing_text()) return;

        if (!ctrl) {
                if (IsKeyPressed(KEY_DELETE) || IsKeyPressed(KEY_BACKSPACE)) each_selected(square_delete);
                if (IsKeyPressed(KEY_ESCAPE)) select_all(false);
                return;
        }

        if (IsKeyPressed(KEY_S) && !shift) bar_save();
        if (IsKeyPressed(KEY_S) && shift) file_save_as();
        if (IsKeyPressed(KEY_O)) file_open();
        if (IsKeyPressed(KEY_T)) file_new();
        if (IsKeyPressed(KEY_G)) subgraph_ask();
        if (IsKeyPressed(KEY_W)) file_close();
        if (IsKeyPressed(KEY_TAB) || IsKeyPressed(KEY_PAGE_DOWN) || IsKeyPressed(KEY_PAGE_UP))
                file_next_tab(shift || IsKeyPressed(KEY_PAGE_UP) ? -1 : 1);
        if (IsKeyPressed(KEY_Z) && !shift) history_step(true);
        if (IsKeyPressed(KEY_Y) || (IsKeyPressed(KEY_Z) && shift)) history_step(false);
        if (IsKeyPressed(KEY_A)) select_all(true);
        if (IsKeyPressed(KEY_C)) selection_copy();
        if (IsKeyPressed(KEY_X)) {
                selection_copy();
                each_selected(square_delete);
        }
        if (IsKeyPressed(KEY_V)) selection_paste(mouse_world());
        if (IsKeyPressed(KEY_D)) selection_duplicate();
}

static float
output_line_height(void)
{
        return cfg.output.font_size + 4;
}

static Rectangle
output_header_rect(void)
{
        Rectangle r = output_rect();
        return (Rectangle) { r.x, r.y, r.width, cfg.output.font_size + 12 };
}

static Rectangle
output_clear_rect(void)
{
        Rectangle h = output_header_rect();
        float w     = measure_text("Clear", cfg.output.font_size) + 16;
        return (Rectangle) { h.x + h.width - w - 4, h.y + 3, w, h.height - 6 };
}

static int
output_visible_lines(void)
{
        Rectangle r = output_rect();
        return Max(1, (int) ((r.height - output_header_rect().height - 4) / output_line_height()));
}

/* Returns true if the panel consumed the input this frame. */
static bool
update_output(void)
{
        if (!output.open) return false;

        Vector2 mouse = GetMousePosition();
        if (!CheckCollisionPointRec(mouse, output_rect())) return false;

        /* The wheel scrolls it (the canvas doesn't zoom from here) */
        float wheel = GetMouseWheelMove();
        if (wheel) {
                int max       = Max(0, output.lines.count - output_visible_lines());
                output.scroll = Clamp(output.scroll + (int) (wheel * 3), 0, max);
        }
        if (IsMouseButtonPressed(MOUSE_BUTTON_LEFT)) {
                if (CheckCollisionPointRec(mouse, output_clear_rect())) output_clear();
                return true;
        }
        return IsMouseButtonPressed(MOUSE_BUTTON_RIGHT);
}

static void
draw_output(void)
{
        if (!output.open) return;

        Rectangle r = output_rect();
        Rectangle h = output_header_rect();
        Rectangle c = output_clear_rect();
        float fs    = cfg.output.font_size;
        float lh    = output_line_height();
        int n       = output_visible_lines();

        DrawRectangleRec(r, cfg.colors.output);
        DrawLine(r.x, r.y, r.x + r.width, r.y, cfg.colors.bar_border);

        const char *title = output.scroll ? TextFormat("Output (%d newer below)", output.scroll) : "Output";
        draw_text(title, h.x + 10, h.y + (h.height - fs) / 2, fs, cfg.colors.bar_text);
        DrawRectangleRec(c, CheckCollisionPointRec(GetMousePosition(), c) ? cfg.colors.menu_hover : cfg.colors.menu);
        DrawRectangleLinesEx(c, 1.0f, cfg.colors.menu_border);
        draw_text("Clear", c.x + 8, c.y + (c.height - fs) / 2, fs, cfg.colors.bar_text);
        DrawLine(r.x, h.y + h.height, r.x + r.width, h.y + h.height, cfg.colors.bar_border);

        /* Newest at the bottom */
        BeginScissorMode(r.x, h.y + h.height, r.width, r.height - h.height);
        int last = output.lines.count - 1 - output.scroll;
        for (int k = 0; k < n && last - k >= 0; k++) {
                OutputLine *l = &output.lines.items[last - k];
                float y       = r.y + r.height - 4 - (k + 1) * lh;
                draw_text(l->text, r.x + 10, y + 2, fs, l->info ? cfg.colors.name : cfg.colors.output_text);
        }
        EndScissorMode();
}

/* Value of output OUT of S as text, for the tooltip */
static void
output_tooltip(Square *s, int out, char *buf, int size)
{
        PinInfo *p = pin_info(s, PART_OUTPUT, out);
        char v[128];

        if (!p) return;
        if (p->type == LI_TICK) {
                double t = out < (int) countof(s->fired) ? s->fired[out] : 0;
                if (t > 0)
                        snprintf(buf, size, "tick, %.1f s ago", GetTime() - t);
                else
                        snprintf(buf, size, "tick, not fired yet");
                return;
        }
        /* Pure squares compute it when read */
        if (s->fn.eval && !s->evaluating) {
                s->evaluating = true;
                s->fn.eval(s);
                s->evaluating = false;
        }
        if (p->value.type == LI_NONE) {
                snprintf(buf, size, "no value");
                return;
        }
        li_format(p->value, v, sizeof v);
        snprintf(buf, size, "%s %s", type_names[p->value.type], v);
}

/* What's on the wire or output pin under the mouse */
static void
draw_tooltip(void)
{
        char buf[192] = "";
        Vector2 mouse = GetMousePosition();
        Vector2 world = mouse_world();

        if (drag.kind != DRAG_NONE || menu.depth || !CheckCollisionPointRec(mouse, canvas_rect())) return;

        Hit h = square_hit(world);
        if (h.part == PART_OUTPUT) {
                output_tooltip(h.square, h.pin, buf, sizeof buf);
        } else if (h.part == PART_NONE) {
                int l = link_hit(world);
                if (l < 0) return;
                Square *a = square_get(links.items[l].from);
                Square *b = square_get(links.items[l].to);
                if (!link_valid(links.items[l]))
                        snprintf(buf, sizeof buf, "%s -> %s: carries nothing",
                                 type_names[pin_info(a, PART_OUTPUT, links.items[l].from_pin)->type],
                                 type_names[pin_info(b, PART_INPUT, links.items[l].to_pin)->type]);
                else
                        output_tooltip(a, links.items[l].from_pin, buf, sizeof buf);
        }
        if (!*buf) return;

        float fs    = cfg.menu.font_size;
        float pad   = cfg.menu.padding;
        Rectangle r = { mouse.x + 16, mouse.y + 16, measure_text(buf, fs) + 2 * pad, fs + 2 * pad };
        r.x         = fminf(r.x, GetScreenWidth() - r.width);
        r.y         = fminf(r.y, GetScreenHeight() - r.height);
        DrawRectangleRec(r, cfg.colors.menu);
        DrawRectangleLinesEx(r, 1.0f, cfg.colors.menu_border);
        draw_text(buf, r.x + pad, r.y + pad, fs, cfg.colors.menu_text);
}

static void
draw_bar(void)
{
        int w         = GetScreenWidth();
        int h         = cfg.bar.height;
        int fs        = cfg.bar.font_size;
        int y         = GetScreenHeight() - h;
        Vector2 world = GetScreenToWorld2D(GetMousePosition(), camera);

        DrawRectangle(0, y, w, h, cfg.colors.bar);
        DrawLine(0, y, w, y, cfg.colors.bar_border);
        draw_text(TextFormat("x: %.0f  y: %.0f  zoom: %.2f  squares: %d  links: %d  modules: %d  FPS: %d",
                             world.x, world.y, camera.zoom, squares.count, links.count, modules.count, GetFPS()),
                  10, y + (h - fs) / 2, fs, cfg.colors.bar_text);

        Rectangle b = { GetScreenWidth(), 0, 0, 0 };
        for (int i = 0; i < countof(bar_buttons); i++) {
                const char *text = bar_buttons[i].get_label ? bar_buttons[i].get_label() : bar_buttons[i].label;
                bool hover       = CheckCollisionPointRec(GetMousePosition(), b = bar_button_rect(i));
                DrawRectangleRec(b, hover ? cfg.colors.menu_hover : cfg.colors.menu);
                DrawRectangleLinesEx(b, 1.0f, cfg.colors.menu_border);
                draw_text(text, b.x + (b.width - measure_text(text, fs)) / 2, b.y + (b.height - fs) / 2, fs,
                          cfg.colors.bar_text);
        }

        /* Status at the left of the buttons */
        if (GetTime() < status_until) {
                /* Over the stats if they are long: it's the newer thing */
                float sw = measure_text(status, fs);
                DrawRectangle(b.x - sw - 24, y + 1, sw + 20, h - 1, cfg.colors.bar);
                draw_text(status, b.x - sw - 12, y + (h - fs) / 2, fs, cfg.colors.bar_text);
        }
}

int
main(int argc, char **argv)
{
        const char *f_version;
        const char *f_config;
        const char *f_file;
        const char *f_restore;
        const char *f_tab;

        flag_program();
        flag_add(&f_version, "--version");
        flag_add(&f_config, "--config", "-c", .defaults = CONFIG_DEFAULT, .nargs = 1,
                 .help = "lua config file");
        flag_add(&f_file, "--file", "-f", .nargs = 1,
                 .help = "canvas file to load at start, and to save to (default: save.path in the config)");
        flag_add(&f_tab, "--tab", "-t", .nargs = 1,
                 .help = "open the canvas file in a new tab of the editor running (or start one with it)");
        flag_add(&f_restore, "--restore", .nargs = 1,
                 .help = "session to carry on with (the editor uses it to reload itself when rebuilt)");
        if (flag_parse(&argc, &argv)) {
                flag_show_help(STDERR_FILENO);
                exit(1);
        }

        if (f_version) {
                fprintf(stdout, "%s, version %s\n", basename(argv[0]), VERSION);
                fprintf(stdout, "Copyright (C) 2026 Hugo Coto Flórez.\n");
                fprintf(stdout, "License GPLv3+: GNU GPL version 3 or later <http://gnu.org/licenses/gpl.html>\n");
                fprintf(stdout, "\n");
                fprintf(stdout, "This is free software; you are free to change and redistribute it.\n");
                fprintf(stdout, "There is NO WARRANTY, to the extent permitted by law.\n");
                exit(0);
        }

        if (f_tab) {
                if (tab_socket_send(f_tab)) return 0;
                f_file = f_tab; /* none running: this one */
        }

        cfg_defaults = cfg;
        config_path  = f_config;
        config_mtime = file_mtime(config_path);
        load_config(config_path);
        exe_watch();
        if (f_restore) session_window(f_restore);
        init_raylib();
        load_font();
        load_modules(cfg.modules.path);

        tab_new(f_file ? f_file : cfg.save.path);
        tab_socket_listen();
        if (f_restore) {
                if (session_restore(f_restore))
                        set_status("Reloaded lineit");
                else
                        set_status("Could not restore %s", f_restore);
        }

        while (!handed_over && !quitting) {
                if (WindowShouldClose() && !ask.open) quit_request();
                tab_socket_poll();
                update_camera();
                if (!update_ask() && !update_prompt() && !update_field() && !update_panel() && !update_output() && !update_bar() &&
                    !update_tabs() && !update_menu()) {
                        update_shortcuts();
                        update_mouse();
                }
                run_frame();
                history_check();
                hot_reload();

                BeginDrawing();
                ClearBackground(cfg.colors.bg);
                draw_canvas();
                draw_tooltip();
                draw_panel();
                draw_output();
                draw_tabs();
                draw_bar();
                draw_menu();
                draw_prompt();
                draw_ask();
                EndDrawing();
                exe_ready_check();
        }

        tab_socket_close();
        /* Their squares may have something to close */
        for (int i = tabs.count - 1; i >= 0; i--) {
                if (i != tab_cur) tab_close(i, true);
        }
        canvas_clear(-1, NULL);
        history_destroy();
        unload_modules();
        unload_fonts();
        output_clear();
        Da_destroy(&output.lines);
        CloseWindow();
        Da_destroy(&squares);
        Da_destroy(&links);
        flag_free();
        return 0;
}

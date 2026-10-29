#define _POSIX_C_SOURCE 200809L /* strdup, strtok_r */
#define INCLUDE_CONF_IMPLEMENTATION
#include "conf.h"
#include "cum.h"
#include "flag.h"
#include "lineit.h"
#include "raylib.h"
#include "raymath.h"

#include <dlfcn.h>
#include <fontconfig/fontconfig.h>
#include <libgen.h>
#include <stdarg.h>
#include <stdint.h>
#include <unistd.h>

#ifndef VERSION
#define VERSION "unknown"
#endif

#define CONFIG_DEFAULT "config.lua"

#define countof(a) ((int) (sizeof(a) / sizeof *(a)))

/* Names used in the config (colors.types.<name>) */
static const char *type_names[LI_TYPE_COUNT] = {
        [LI_NONE] = "none", [LI_TICK] = "tick", [LI_ANY] = "any", [LI_BOOL] = "bool",
        [LI_INT] = "int", [LI_FLOAT] = "float", [LI_STRING] = "string", [LI_PTR] = "ptr",
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
        } modules;
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
                Color error;
                Color error_text;
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
        },
        .bar    = { .height = 32, .font_size = 20 },
        .menu   = { .font_size = 20, .padding = 8 },
        .font    = { .name = "Iosevka NFM", .spacing = 0 },
        .modules = { .path = "modules" },
        .colors = {
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
        .error         = { 239, 68, 68, 255 },
        .error_text    = { 255, 255, 255, 255 },
        .types = {
                [LI_NONE]   = { 113, 113, 122, 255 },
                [LI_TICK]   = { 228, 228, 231, 255 },
                [LI_ANY]    = { 161, 161, 170, 255 },
                [LI_BOOL]   = { 248, 113, 113, 255 },
                [LI_INT]    = { 74, 222, 128, 255 },
                [LI_FLOAT]  = { 190, 242, 100, 255 },
                [LI_STRING] = { 244, 114, 182, 255 },
                [LI_PTR]    = { 192, 132, 252, 255 },
        },
        .menu          = { 39, 39, 42, 255 },
        .menu_border   = { 63, 63, 70, 255 },
        .menu_hover    = { 63, 63, 70, 255 },
        .menu_text     = { 228, 228, 231, 255 },
        },
};

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

static void
load_config(const char *path)
{
        Conf conf;

        if (Conf_open(&conf, path) != CONF_OK) {
                fprintf(stderr, "config: could not load %s, using defaults\n", path);
                return;
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

        conf_int(conf, &cfg.bar.height, "bar.height");
        conf_int(conf, &cfg.bar.font_size, "bar.font_size");

        conf_int(conf, &cfg.menu.font_size, "menu.font_size");
        conf_int(conf, &cfg.menu.padding, "menu.padding");

        conf_str(conf, &cfg.font.name, "font.name");
        conf_int(conf, &cfg.font.spacing, "font.spacing");

        conf_str(conf, &cfg.modules.path, "modules.path");

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
        conf_color(conf, &cfg.colors.error, "colors.error");
        conf_color(conf, &cfg.colors.error_text, "colors.error_text");
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
}

typedef struct Module Module;

typedef struct PinInfo {
        char *name;
        Li_Type type;
        Li_Value value; /* last value set, outputs only */
} PinInfo;

typedef Da(PinInfo) PinList;

/* A square on the canvas, an instance of a module. Modules see it as an
 * opaque Li_Node. */
struct Li_Node {
        int id;
        Vector2 pos; /* top-left corner, world space */
        Module *module;
        PinList inputs;
        PinList outputs;
        void *data;      /* module state, see li_set_data */
        char text[64];   /* drawn inside, see li_text */
        bool evaluating; /* guards module_eval against cycles */
};
typedef struct Li_Node Square;

/* Wire from an output of square `from` to an input of square `to` */
typedef struct Link {
        int from, from_pin; /* square id, output index */
        int to, to_pin;     /* square id, input index */
} Link;

/* A loaded .so */
struct Module {
        char *name;        /* path inside the modules dir, no extension: "math/add" */
        const char *label; /* last part of the name: "add" */
        void *handle;
        void (*init)(Li_Node *n);
        void (*tick)(Li_Node *n, int input);
        void (*eval)(Li_Node *n);
        void (*frame)(Li_Node *n);
        void (*destroy)(Li_Node *n);
};

typedef enum Part {
        PART_NONE,
        PART_INPUT,
        PART_OUTPUT,
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
               DRAG_SQUARE,
               DRAG_WIRE } kind;
        Square *square; /* square being moved, or wire source */
        int pin;        /* wire source output */
        Vector2 offset; /* grab point relative to square position */
} drag;

static Font font;
static Camera2D camera = { .zoom = 1.0f };
static Da(Square *) squares; /* draw order: last is on top */
static Da(Link) links;
static int next_id;
static Da(Module *) modules; /* sorted by name */
static Module *last_module;  /* the last one placed, first in the canvas menu */
static bool running;         /* module_frame is only called while running */

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
        void (*fn)(int id, void *arg);
        void *arg;
        Menu *sub;
};

#define MENU_DEPTH 8

typedef struct MenuLevel {
        Menu *menu;
        Vector2 pos; /* top-left corner, screen space */
} MenuLevel;

static struct {
        int depth; /* open levels (submenus), 0 when closed */
        MenuLevel level[MENU_DEPTH];
        int id;              /* object the menu belongs to */
        Vector2 world;       /* where it was opened, world space */
        Vector2 right_press; /* where the right button went down */
} menu;

static Menu module_menu; /* one entry per module, folders are submenus */
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

/* Load cfg.font.name, or keep raylib's default font if it fails. */
static void
load_font(void)
{
        /* Rasterize at the biggest size used, smaller ones are scaled down */
        int size   = Max(Max(cfg.bar.font_size, cfg.menu.font_size),
                         Max(cfg.canvas.pin_font_size, cfg.canvas.name_font_size));
        char *path = FileExists(cfg.font.name) ? strdup(cfg.font.name) : font_find(cfg.font.name);

        font = GetFontDefault();
        if (path) font = LoadFontEx(path, size, NULL, 0);
        if (!IsFontValid(font) || font.texture.id == GetFontDefault().texture.id) {
                fprintf(stderr, "font: could not load %s, using default\n", path ? path : cfg.font.name);
                font = GetFontDefault();
        }
        SetTextureFilter(font.texture, TEXTURE_FILTER_BILINEAR);
        free(path);
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

/* Area of the screen where the canvas is drawn (everything but the bar). */
static Rectangle
canvas_rect(void)
{
        return (Rectangle) { 0, 0, GetScreenWidth(), GetScreenHeight() - cfg.bar.height };
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

/* Always a square, as tall as its pins need */
static float
square_side(Square *s)
{
        int pins = Max(s->inputs.count, s->outputs.count);
        return fmaxf((pins + 1) * cfg.canvas.pin_spacing, cfg.canvas.square_size);
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

/* Inputs on the left side, outputs on the right, centered vertically */
static Vector2
pin_pos(Square *s, Part part, int i)
{
        float side = square_side(s);
        int count  = part == PART_INPUT ? s->inputs.count : s->outputs.count;
        float y    = s->pos.y + side / 2 + (i - (count - 1) / 2.0f) * cfg.canvas.pin_spacing;
        float x    = part == PART_INPUT ? s->pos.x : s->pos.x + side;
        return (Vector2) { x, y };
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
                        if (CheckCollisionPointCircle(p, pin_pos(s, PART_INPUT, j), r))
                                return (Hit) { s, PART_INPUT, j };
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
        return pin_info(from, PART_OUTPUT, from_pin) && pin_info(to, PART_INPUT, to_pin) && from != to;
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
        Da_append(&links, (Link) { from->id, from_pin, to->id, to_pin });
}

/* ---- Module API (lineit.h). Exported to the modules with -rdynamic ---- */

static int
pin_add(PinList *l, const char *name, Li_Type type)
{
        Da_append(l, (PinInfo) { .name = strdup(name ? name : ""), .type = type });
        return l->count - 1;
}

int
li_input(Li_Node *n, const char *name, Li_Type type)
{
        return pin_add(&n->inputs, name, type);
}

int
li_output(Li_Node *n, const char *name, Li_Type type)
{
        return pin_add(&n->outputs, name, type);
}

Li_Value
li_get(Li_Node *n, int input)
{
        int l = link_to(n->id, input);
        if (l < 0) return (Li_Value) { .type = LI_NONE };

        Link k      = links.items[l];
        Square *src = square_get(k.from);
        if (!src || !link_valid(k)) return (Li_Value) { .type = LI_NONE };

        /* Pure squares compute their outputs when someone reads them */
        if (src->module->eval && !src->evaluating) {
                src->evaluating = true;
                src->module->eval(src);
                src->evaluating = false;
        }
        PinInfo *out = pin_info(src, PART_OUTPUT, k.from_pin);
        return out ? out->value : (Li_Value) { .type = LI_NONE };
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
        static int depth;
        static bool warned;

        if (depth >= TICK_DEPTH_MAX) {
                if (!warned) fprintf(stderr, "tick: more than %d squares deep, is there a loop?\n", TICK_DEPTH_MAX);
                warned = true;
                return;
        }

        depth++;
        for (int i = 0; i < links.count; i++) {
                Link k = links.items[i];
                if (k.from != n->id || k.from_pin != output || !link_valid(k)) continue;
                Square *dst = square_get(k.to);
                if (dst && dst->module->tick) dst->module->tick(dst, k.to_pin);
        }
        depth--;
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

double
li_time(void)
{
        return GetTime();
}

/* ---- Squares ---- */

static Square *
square_new(Module *m, Vector2 center)
{
        Square *s = calloc(1, sizeof *s);
        s->id     = next_id++;
        s->module = m;
        m->init(s);

        s->pos = Vector2SubtractValue(center, square_side(s) / 2);
        Da_append(&squares, s);
        last_module = m;
        return s;
}

static void
pins_free(PinList *l)
{
        Da_foreach(p, *l)
        {
                free(p->name);
        }
        Da_destroy(l);
}

static void
square_free(Square *s)
{
        if (s->module->destroy) s->module->destroy(s);
        pins_free(&s->inputs);
        pins_free(&s->outputs);
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
square_duplicate(int id, void *arg)
{
        (void) arg;
        Square *s = square_get(id);
        if (!s) return;
        float off = square_side(s) / 2 + cfg.canvas.grid_step;
        square_new(s->module, Vector2AddValue(s->pos, off));
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
}

/* Options shown on right click over a square. Add new entries here. */
static MenuItem square_items[] = {
        { .label = "Duplicate", .fn = square_duplicate },
        { .label = "Disconnect inputs", .fn = square_disconnect_inputs },
        { .label = "Disconnect outputs", .fn = square_disconnect_outputs },
        { .label = "Disconnect all", .fn = square_disconnect_all },
        { .label = "Delete", .fn = square_delete },
};
static Menu square_menu = { 0, countof(square_items), square_items };

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

/* Load every .so under DIR, and build module_menu with the same folders */
static void
load_modules(const char *dir)
{
        if (!DirectoryExists(dir)) {
                fprintf(stderr, "modules: %s not found\n", dir);
                return;
        }

        FilePathList files = LoadDirectoryFilesEx(dir, ".so", true);
        for (unsigned i = 0; i < files.count; i++) {
                const char *path = files.paths[i];
                void *handle     = dlopen(path, RTLD_NOW | RTLD_LOCAL);
                if (!handle) {
                        fprintf(stderr, "modules: %s\n", dlerror());
                        continue;
                }

                Module *m  = calloc(1, sizeof *m);
                m->handle  = handle;
                m->init    = (void (*)(Li_Node *)) dlsym(handle, "module_init");
                m->tick    = (void (*)(Li_Node *, int)) dlsym(handle, "module_tick");
                m->eval    = (void (*)(Li_Node *)) dlsym(handle, "module_eval");
                m->frame   = (void (*)(Li_Node *)) dlsym(handle, "module_frame");
                m->destroy = (void (*)(Li_Node *)) dlsym(handle, "module_destroy");
                if (!m->init) {
                        fprintf(stderr, "modules: %s has no module_init, skipped\n", path);
                        dlclose(handle);
                        free(m);
                        continue;
                }

                /* "modules/math/add.so" -> "math/add" */
                const char *rel = path + strlen(dir);
                while (*rel == '/')
                        rel++;
                m->name = strdup(rel);
                *strrchr(m->name, '.') = '\0';
                m->label = strrchr(m->name, '/') ? strrchr(m->name, '/') + 1 : m->name;
                Da_append(&modules, m);
        }
        UnloadDirectoryFiles(files);

        qsort(modules.items, modules.count, sizeof *modules.items, module_cmp);
        Da_foreach(mp, modules)
        {
                Module *mod = *mp;
                Menu *m     = &module_menu;
                char *name  = strdup(mod->name);
                char *save, *next;
                char *part = strtok_r(name, "/", &save);

                while ((next = strtok_r(NULL, "/", &save))) {
                        m    = menu_submenu(m, part);
                        part = next;
                }
                Da_append(m, (MenuItem) { .label = mod->label, .fn = canvas_add_module, .arg = mod });
                free(name);
        }
        if (modules.count) last_module = modules.items[0];
}

static void
unload_modules(void)
{
        menu_free(&module_menu);
        Da_destroy(&canvas_menu);
        Da_foreach(mp, modules)
        {
                dlclose((*mp)->handle);
                free((*mp)->name);
                free(*mp);
        }
        Da_destroy(&modules);
}

/* Let squares start tick chains */
static void
run_frame(void)
{
        if (!running) return;
        for (int i = 0; i < squares.count; i++) {
                Square *s = squares.items[i];
                if (s->module->frame) s->module->frame(s);
        }
}

/* ---- Menu ---- */

static void
menu_open(Menu *m, int id, Vector2 pos)
{
        menu.depth    = 1;
        menu.level[0] = (MenuLevel) { m, pos };
        menu.id       = id;
        menu.world    = GetScreenToWorld2D(pos, camera);
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
                w       = fmaxf(w, measure_text(it->label, cfg.menu.font_size));
                has_sub = has_sub || it->sub;
        }
        w += 4 * cfg.menu.padding;
        if (has_sub) w += measure_text(MENU_ARROW, cfg.menu.font_size) + 2 * cfg.menu.padding;

        Rectangle r = { pos.x, pos.y, w, m->count * menu_item_height() };

        /* Keep it inside the window. Submenus flip to the parent's left. */
        if (r.x + r.width > GetScreenWidth())
                r.x = lvl > 0 ? menu_rect(lvl - 1).x - r.width : GetScreenWidth() - r.width;
        r.y = fminf(r.y, GetScreenHeight() - r.height);
        r.x = fmaxf(r.x, 0);
        r.y = fmaxf(r.y, 0);
        return r;
}

/* Item under screen point P, deepest menu first, or -1. Its level goes to LVL. */
static int
menu_hit(Vector2 p, int *lvl)
{
        for (int k = menu.depth - 1; k >= 0; k--) {
                Rectangle r = menu_rect(k);
                if (!CheckCollisionPointRec(p, r)) continue;
                *lvl = k;
                return Min((int) ((p.y - r.y) / menu_item_height()), menu.level[k].menu->count - 1);
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
                        menu_open(&square_menu, h.square->id, mouse);
                } else {
                        canvas_menu_build();
                        menu_open(&canvas_menu, -1, mouse);
                }
        }

        if (!menu.depth) return false;

        if (IsKeyPressed(KEY_ESCAPE)) {
                menu.depth = 0;
                return true;
        }

        int lvl      = 0;
        int i        = menu_hit(mouse, &lvl);
        MenuItem *it = i >= 0 ? &menu.level[lvl].menu->items[i] : NULL;

        /* Hovering an entry opens its submenu and closes deeper ones */
        if (it) {
                menu.depth = lvl + 1;
                if (it->sub && menu.depth < MENU_DEPTH) {
                        Rectangle r                = menu_rect(lvl);
                        menu.level[menu.depth++] = (MenuLevel) {
                                it->sub, { r.x + r.width, r.y + i * menu_item_height() }
                        };
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

                DrawRectangleRec(r, cfg.colors.menu);
                for (int i = 0; i < m->count; i++) {
                        MenuItem *it   = &m->items[i];
                        Rectangle item = { r.x, r.y + i * h, r.width, h };
                        float ty       = item.y + cfg.menu.padding;

                        /* Highlight the hovered entry and the ones whose submenu is open */
                        bool open = it->sub && k + 1 < menu.depth && menu.level[k + 1].menu == it->sub;
                        if ((k == hover_lvl && i == hover) || open) DrawRectangleRec(item, cfg.colors.menu_hover);

                        draw_text(it->label, item.x + 2 * cfg.menu.padding, ty, cfg.menu.font_size, cfg.colors.menu_text);
                        if (it->sub) {
                                float aw = measure_text(MENU_ARROW, cfg.menu.font_size);
                                draw_text(MENU_ARROW, item.x + item.width - aw - 2 * cfg.menu.padding, ty,
                                          cfg.menu.font_size, cfg.colors.menu_text);
                        }
                }
                DrawRectangleLinesEx(r, 1.0f, cfg.colors.menu_border);
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

                case PART_BODY:
                        /* Move to the top of the draw order */
                        Da_remove(&squares, square_index(h.square));
                        Da_append(&squares, h.square);
                        drag.kind   = DRAG_SQUARE;
                        drag.square = h.square;
                        drag.offset = Vector2Subtract(world, h.square->pos);
                        break;

                case PART_NONE:
                        break;
                }
        }

        if (drag.kind == DRAG_SQUARE) drag.square->pos = Vector2Subtract(world, drag.offset);

        if (IsMouseButtonReleased(MOUSE_BUTTON_LEFT)) {
                if (drag.kind == DRAG_WIRE) {
                        /* Dropped outside an input: the wire is discarded */
                        Hit h = square_hit(world);
                        if (h.part == PART_INPUT) link_add(drag.square, drag.pin, h.square, h.pin);
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

        float wheel = GetMouseWheelMove();
        if (wheel != 0 && CheckCollisionPointRec(mouse, canvas_rect())) {
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
draw_wire(Vector2 a, Vector2 b, Color color)
{
        Vector2 c1, c2;
        wire_controls(a, b, &c1, &c2);
        DrawSplineSegmentBezierCubic(a, c1, c2, b, cfg.canvas.wire_thickness, color);
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
        float tw        = measure_text(msg, fs);
        Rectangle r     = { mid.x - tw / 2 - pad, mid.y - fs - 2 * pad - cfg.canvas.wire_thickness - 4,
                            tw + 2 * pad, fs + 2 * pad };

        DrawRectangleRec(r, cfg.colors.error);
        draw_text(msg, r.x + pad, r.y + pad, fs, cfg.colors.error_text);
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

/* HOVER_TINT is the color of the hovered pin, if it's one of S */
static void
draw_square(Square *s, Hit hover, Color hover_tint)
{
        Rectangle r = square_rect(s);
        float fs    = cfg.canvas.pin_font_size;
        float gap   = cfg.canvas.pin_radius + 4;

        DrawRectangleRec(r, cfg.colors.square);
        DrawRectangleLinesEx(r, 2.0f, cfg.colors.square_border);

        for (int i = 0; i < s->inputs.count; i++) {
                PinInfo *in = &s->inputs.items[i];
                Vector2 p   = pin_pos(s, PART_INPUT, i);
                bool hov    = hover.square == s && hover.part == PART_INPUT && hover.pin == i;
                draw_pin(p, in->type, pin_connected(s, PART_INPUT, i), hov ? &hover_tint : NULL);
                draw_text(in->name, p.x + gap, p.y - fs / 2, fs, cfg.colors.square_text);
        }
        for (int i = 0; i < s->outputs.count; i++) {
                PinInfo *out = &s->outputs.items[i];
                Vector2 p    = pin_pos(s, PART_OUTPUT, i);
                bool hov     = hover.square == s && hover.part == PART_OUTPUT && hover.pin == i;
                draw_pin(p, out->type, pin_connected(s, PART_OUTPUT, i), hov ? &hover_tint : NULL);
                draw_text(out->name, p.x - gap - measure_text(out->name, fs), p.y - fs / 2, fs, cfg.colors.square_text);
        }

        if (*s->text) {
                float tw = measure_text(s->text, fs);
                draw_text(s->text, r.x + (r.width - tw) / 2, r.y + (r.height - fs) / 2, fs, cfg.colors.square_text);
        }

        /* Module name below */
        float nfs = cfg.canvas.name_font_size;
        float nw  = measure_text(s->module->label, nfs);
        draw_text(s->module->label, r.x + (r.width - nw) / 2, r.y + r.height + 4, nfs, cfg.colors.name);
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

        /* Wires go below the squares. Their type is the output type. */
        Da_foreach(l, links)
        {
                Square *a = square_get(l->from);
                Square *b = square_get(l->to);
                if (!a || !b) continue;
                Color c = cfg.colors.types[pin_info(a, PART_OUTPUT, l->from_pin)->type];
                draw_wire(pin_pos(a, PART_OUTPUT, l->from_pin), pin_pos(b, PART_INPUT, l->to_pin), c);
        }

        Da_foreach(s, squares)
        {
                draw_square(*s, hover, tint);
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

/* Start/stop button, at the right of the bar */
static Rectangle
bar_button_rect(void)
{
        float fs  = cfg.bar.font_size;
        float pad = 4;
        float w   = fmaxf(measure_text("Start", fs), measure_text("Stop", fs)) + 4 * pad;
        float h   = cfg.bar.height - 2 * pad;
        return (Rectangle) { GetScreenWidth() - w - pad, GetScreenHeight() - cfg.bar.height + pad, w, h };
}

/* Returns true if the bar consumed the input this frame. */
static bool
update_bar(void)
{
        if (IsMouseButtonPressed(MOUSE_BUTTON_LEFT) &&
            CheckCollisionPointRec(GetMousePosition(), bar_button_rect())) {
                running    = !running;
                menu.depth = 0;
                return true;
        }
        return false;
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

        Rectangle b      = bar_button_rect();
        const char *text = running ? "Stop" : "Start";
        bool hover       = CheckCollisionPointRec(GetMousePosition(), b);
        DrawRectangleRec(b, hover ? cfg.colors.menu_hover : cfg.colors.menu);
        DrawRectangleLinesEx(b, 1.0f, cfg.colors.menu_border);
        draw_text(text, b.x + (b.width - measure_text(text, fs)) / 2, b.y + (b.height - fs) / 2, fs,
                  cfg.colors.bar_text);
}

int
main(int argc, char **argv)
{
        const char *f_version;
        const char *f_config;

        flag_program();
        flag_add(&f_version, "--version");
        flag_add(&f_config, "--config", "-c", .defaults = CONFIG_DEFAULT, .nargs = 1,
                 .help = "lua config file");
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

        load_config(f_config);
        init_raylib();
        load_font();
        load_modules(cfg.modules.path);

        while (!WindowShouldClose()) {
                update_camera();
                if (!update_bar() && !update_menu()) update_mouse();
                run_frame();

                BeginDrawing();
                ClearBackground(cfg.colors.bg);
                draw_canvas();
                draw_bar();
                draw_menu();
                EndDrawing();
        }

        canvas_clear(-1, NULL);
        unload_modules();
        if (font.texture.id != GetFontDefault().texture.id) UnloadFont(font);
        CloseWindow();
        Da_destroy(&squares);
        Da_destroy(&links);
        flag_free();
        return 0;
}

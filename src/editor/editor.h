/* editor.h - what the files of the editor (src/editor/) share: its
 * types, and the functions and variables more than one of them uses.
 * What only one file uses is static in it. src/main.c is main(). */
#ifndef EDITOR_H_
#define EDITOR_H_

#define _POSIX_C_SOURCE 200809L /* strdup, strtok_r */
#define _XOPEN_SOURCE 700       /* realpath */
#define _DEFAULT_SOURCE         /* SOCK_NONBLOCK, struct timeval */
#include "../lineit.h"
#include "conf.h"
#include "cum.h"
#include "raylib.h"
#include "raymath.h"

#include <assert.h>
#include <ctype.h>
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <fontconfig/fontconfig.h>
#include <limits.h>
#include <math.h>
#include <poll.h>
#include <pthread.h>
#include <signal.h>
#include <stdarg.h>
#include <stdatomic.h>
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

#include <openssl/evp.h>

#include "../branches.h"
#include "../value.h"

#define countof(a) ((int) (sizeof(a) / sizeof *(a)))

/* The tests (tools/run_tests.c) run frames of fake time */
#ifdef LINEIT_FAKE_TIME
double fake_time(void);
#define GetTime fake_time
#endif

/* ---- Types, and the variables more than one file uses ---- */

/* Names used in the config (colors.types.<name>) */
extern const char *type_names[LI_TYPE_COUNT];

/* The colors of the editor: a theme gives them all, and colors.* in the
 * config changes some (see themes.c) */
typedef struct Colors {
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
} Colors;

struct CfgState {
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
                const char *path;   /* loaded, read only */
                const char *user;   /* loaded too, and where Make subgraph and New type write */
                const char *source; /* their C, for Compile */
                const char *cache;  /* packages built here, see package_build */
        } modules;
        struct {
                const char *path;        /* --file overrides it */
                bool autosave;           /* at start, File > Autosave toggles it */
                float autosave_interval; /* seconds */
        } save;
        struct {
                int height; /* of the panel, pixels */
                int font_size;
                int lines; /* kept, older ones are dropped */
                bool clear_on_run;
        } output;
        struct {
                const char *output;  /* where programs go */
                const char *runtime; /* liblineit-rt.a and its headers, see src/rt */
                const char *cc;
                const char *cflags;
                const char *objcopy; /* copies the object of a package per square */
        } compile;
        const char *theme; /* the colors, see themes.c */
        Colors colors;
};

/* Default values, overwritten by the config file */
extern struct CfgState cfg;

/* cfg as it was before reading the config, to read it again from scratch */
extern Typeof(cfg) cfg_defaults;

struct ThemesState {
        const char *name;
        Colors colors;
};

/* The built-in ones, the first is the default. theme = "NAME" in the config
 * picks one, or one of the config's own:
 *
 *   themes = {
 *           { name = "mine", base = "light", colors = { bg = { 255, 250, 240 }, types = { int = { 0, 128, 0 } } } },
 *   }
 *
 * which is its base (dark if it has none) with the colors it sets. Then
 * colors.* in the config changes the ones it sets, whatever the theme. */
extern const struct ThemesState themes[];

extern const int themes_count;

typedef Da(char *) UserThemesDa;

/* Names of the config's own themes, for Settings */
extern UserThemesDa user_themes;

typedef enum SettingType {
        SET_INT,
        SET_FLOAT,
        SET_BOOL,
        SET_STR,
        SET_COLOR,
} SettingType;

/* One key of the config, read into VAL. The settings tab (File > Settings)
 * shows them in this order, under SECTION and SUB. */
typedef struct Setting {
        const char *key; /* "canvas.grid_step" */
        SettingType type;
        void *val; /* in cfg */
        const char *section, *sub;
        const char *help;
        float min, max; /* numbers out of [min, max] aren't taken from Settings, if max > min */
        bool start;     /* only read at start: a change waits for the next one */
        bool empty;     /* an empty string is fine */
} Setting;

extern const Setting settings[];

extern const int settings_count;
#define SETTINGS_MAX 128 /* settings_count at most */

/* Where the value of each setting comes from, for Settings */
enum { FROM_DEFAULT,
       FROM_THEME,
       FROM_CONFIG,
       FROM_SETTINGS };

extern char setting_from[];

/* settings.lua, next to the config: what Settings changes, read after the
 * config so it wins. lineit writes it, the config is never touched. */
extern char settings_path[PATH_MAX];

extern char *setting_over[];

/* MAJOR.MINOR.PATCH: LI_VERSION (lineit.h), the one version of lineit, its
 * module API and the files it writes (canvases, types). Something made for
 * version V can be used by a lineit that has version H if they have the
 * same major and V's minor isn't newer than H's: a minor only adds things,
 * a major breaks them. The patch is only for people, never checked.
 * Integers, not a float: 0.123.0 is always 0.123.0. */
typedef struct Version {
        int major, minor, patch;
} Version;

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
        const char *(*emit)(Li_Node *n); /* only used by Compile */
        const char *(*libs)(void);       /* the same */
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
        PinList was;      /* the inputs before li_pins_reset, for their typed values */
        ParamList params; /* changed from the settings panel */
        float side;       /* cached by square_side, 0 to recompute */
        float label_w;    /* longest input name, cached with side */
        float field_w;    /* widest input field, cached with side */
        bool selected;
        double flash;          /* last time a tick reached it */
        double fired[16];      /* last time each tick output fired, for the tooltip */
        void *data;            /* module state, see li_set_data */
        char text[64];         /* drawn inside, see li_text */
        bool evaluating;       /* guards module_eval against cycles */
        int ir;                /* its node in the program, see program.c */
        struct Graph *graph;   /* a subgraph: its own canvas, see subgraphs.c */
        struct Li_Node *owner; /* subgraph/input and subgraph/output in one: the subgraph */
        int port;              /* and which of its inputs or outputs they are */
        bool conflict;         /* a var/var that says another thing than one of its variable, see decls.c */
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
        char *path;               /* the .so, or the .lua of a subgraph. NULL for built-in modules */
        const ModuleFns *builtin; /* callbacks in the editor itself instead of a .so */
        char *lib;                /* a package (.li): its .so for here, taken out (see packages.c) */
        Li_Type type;             /* make_ and break_ of a custom type: which (see types.c) */
        char *name;               /* path inside the modules dir, no extension: "math/add" */
        const char *label;        /* last part of the name: "add" */
        long long mtime;          /* of the .so loaded, ns, to see when it's rebuilt */
        int dir;                  /* found in modules_dirs()[dir] */
        bool gone;                /* the .so was removed: not in the menu, its squares still work */
        bool seen;                /* found in the last scan */
};

typedef enum Part {
        PART_NONE,
        PART_INPUT,
        PART_OUTPUT,
        PART_FIELD, /* the typed value of an input, pin is its index */
        PART_PARAM, /* the field of a param, pin is its index */
        PART_BODY,
} Part;

/* What is under a point: a square, which part of it, and the pin index */
typedef struct Hit {
        Square *square;
        Part part;
        int pin;
} Hit;

struct DragBoxState {
        enum { DRAG_NONE,
               DRAG_SQUARE, /* moving the selection */
               DRAG_WIRE,
               DRAG_BOX } kind; /* selecting with a box */
        Square *square;         /* wire source */
        int pin;                /* wire source output */
        bool picked;            /* the wire was plugged in, not a new one */
        Vector2 start;          /* where it started, world space */
        Vector2 last;           /* mouse on the last frame, world space */
};

/* What the left mouse button is currently dragging */
extern struct DragBoxState drag;

extern Font font;

extern Camera2D camera;

typedef Da(Square *) SquareList;

typedef Da(Link) LinkList;

extern SquareList squares;

extern LinkList links;

extern int next_id;

typedef Da(Module *) ModulesDa;

extern ModulesDa modules;

extern Module *last_module;

extern bool running;

extern bool autosave;

extern long long squares_version;

extern bool stepping;

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

struct MenuState {
        int depth; /* open levels (submenus), 0 when closed */
        MenuLevel level[MENU_DEPTH];
        int id;              /* object the menu belongs to */
        Vector2 world;       /* where it was opened, world space */
        Vector2 right_press; /* where the right button went down */
        bool searchable;     /* the canvas menu: typing searches the modules */
        float search_w;      /* its width, fixed while it's open so it doesn't move */
        int selected;        /* entry chosen with the arrow keys, -1 if none */
        /* Opened by dropping a new wire on the canvas: the square (-1 if
         * not) and output it comes from, wired to the module added */
        int wire_from, wire_pin;
};

extern struct MenuState menu;

extern Menu module_menu;

extern Menu search_menu;

extern Menu canvas_menu;

typedef struct OutputLine {
        char *text;
        bool info; /* a message of the editor itself, not of a module */
} OutputLine;

struct OutputPanelState {
        Da(OutputLine) lines;
        bool open;
        int scroll; /* lines up from the newest one */
        int unseen; /* added while it was closed */
};

extern struct OutputPanelState output_panel;

/* Branches print from their threads: one at a time */
extern pthread_mutex_t output_lock;

/* Drawn at the right of option fields */
#define FIELD_ARROW "v"

/* Types made in the editor: a name and typed fields, like a C struct (see
 * Types, by the modules, for where they come from). Their ids come after
 * the built-in ones: LI_TYPE_COUNT + their index in `types`, which doesn't
 * change while lineit runs. A value of one has .p pointing to a Record. */

typedef struct Field {
        char *name;
        char *type; /* by name: types can use types defined after them */
} Field;

typedef struct TypeDef {
        char *name;        /* "geo/point" */
        const char *label; /* "point" */
        Da(Field) fields;
        Color color;
} TypeDef;

typedef Da(TypeDef *) TypesDa;

extern TypesDa types;

typedef struct TextInput {
        char buf[1024];
        int len;
        int cursor; /* where typing goes, 0 to len */
        int anchor; /* the other end of the selection, none if it's cursor */
} TextInput;

typedef enum TextResult {
        TEXT_TYPING,
        TEXT_DONE,   /* Enter */
        TEXT_CANCEL, /* Esc */
        TEXT_NEXT,   /* Tab */
} TextResult;

struct FieldEditState {
        Square *square; /* NULL if none */
        int input;      /* or param */
        bool param;
        TextInput text;
        bool error; /* what is typed is not valid */
};

/* Input field, or param field, being typed in the square */
extern struct FieldEditState field_edit;

/* The list of an options input, opened by clicking its field */
extern Menu options_menu;

extern int options_input;

struct PanelState {
        Square *square; /* NULL when closed */
        int field;      /* param being typed, -1 if none */
        TextInput text;
        bool error; /* what was typed is not valid for the param */
};

/* Settings panel of a square, next to it */
extern struct PanelState panel;

/* Where the copies of the modules go, created on first use */
extern char tmp_dir[PATH_MAX];

extern Menu square_menu;

/* A .so is only used once it has been left alone this long: make may be
 * writing it */
#define MODULE_SETTLE_NS 300000000LL

extern const ModuleFns type_make_fns;

extern const ModuleFns type_break_fns;

/* File used by Save and Load */
extern const char *canvas_path;

/* Message shown in the bar for a few seconds */
extern char status[256];

/* What canvas_write writes */
enum {
        WRITE_CAMERA   = 1 << 0,
        WRITE_STATE    = 1 << 1, /* what module_save returns */
        WRITE_SELECTED = 1 << 2, /* only selected squares and the links between them */
};

typedef enum ReadMode {
        READ_LOAD,    /* replace the canvas, camera included */
        READ_RESTORE, /* make the canvas match (undo): squares that are still there keep their state */
        READ_PASTE,   /* add, with new ids, at the mouse, selected */
        READ_GRAPH,   /* add, into a subgraph being made (see subgraphs.c) */
} ReadMode;

/* Where READ_PASTE puts the top-left corner of what it pastes */
extern Vector2 paste_at;

/* A module can also be a canvas: a .lua under the modules dir. Each square
 * of it (a subgraph) gets its own copy of that canvas, hidden, with its own
 * squares and wires. Its subgraph/input squares are the inputs of the
 * subgraph, and its subgraph/output squares the outputs, named by their name
 * param, in the order they are from top to bottom (left to right on the
 * same row).
 *
 * The inner squares are not in `squares` and `links`, they are in its
 * Graph. To read or change them as if they were the canvas (loading it,
 * Make subgraph), graph_swap puts its lists there, and the next graph_swap
 * puts them back.
 *
 * Running, a subgraph square does nothing itself: the program (see
 * Program) has its inner squares inlined, with the wires through its ports
 * already followed, like a C compiler inlines a function. Its variables,
 * lists and trees are its locals, like a C function's: scope_enter puts
 * them in the globals while one of its squares runs, so the ones outside
 * can't be seen from inside (nor the other way around). Each square of the
 * subgraph has its own, and keeps them between ticks. Data goes in and out
 * through its pins, or through globals (li_global_*). */

typedef struct Graph {
        SquareList squares; /* while swapped in: the ones outside */
        LinkList links;
        VarList vars;        /* its locals */
        Da(Square *) inputs; /* subgraph/input squares inside, by port */
        Da(Square *) outputs;
} Graph;

/* Whose locals the thread uses: NULL for the canvas's, or a subgraph
 * square's while one of its squares runs. Each thread its own: branches run
 * in parallel. */
extern _Thread_local Graph *scope;

/* Port squares: pick a type, and an input or output of that type */
extern const Li_Type port_types[];

extern const int port_types_count;

enum { PORT_OUT_TYPE,
       PORT_OUT_VALUE }; /* of subgraph/output */

/* Nothing to run: the program follows the wires through them (see
 * Program). Outside of a subgraph there is nothing to read from them, and
 * ticks to them go nowhere. */
extern const ModuleFns port_in_fns;

extern const ModuleFns port_out_fns;

extern const ModuleFns graph_fns;

/* Run and Compile don't use the canvas as it is: they lower it first to a
 * program, a flat list of the squares that run with their wires already
 * followed. It's to the canvas what a resolved AST is to the text of a
 * language:
 *
 *   - Subgraphs are inlined: their inner squares are in the list, and the
 *     wires through their ports (subgraph/input and subgraph/output) go
 *     straight to the squares on the other side. Each square knows its
 *     scope, whose locals it uses.
 *   - Each input knows where its value comes from: an output of another
 *     square, a value typed in a square (its own, or the one of the
 *     subgraph square that has it inside), or nothing (unplugged, or wired
 *     to an incompatible type).
 *   - Each tick output knows the inputs it ticks, in the order of its wires.
 *
 * Run walks it (li_get, li_tick, run_frame), Compile writes it as C (see
 * Compile). It's lowered again every frame, so editing while running works.
 * Squares don't move in memory, but they can go away: a program is only
 * used while squares_version is the one it was lowered for. */

typedef enum SourceKind {
        SRC_NONE,
        SRC_OUTPUT, /* an output of a node */
        SRC_TYPED,  /* what's typed in an input */
} SourceKind;

/* Where an input gets its value */
typedef struct Source {
        SourceKind kind;
        int node, pin;  /* SRC_OUTPUT: output PIN of node NODE */
        Square *square; /* SRC_TYPED: input PIN of SQUARE */
        int wires;      /* the wires it goes through: COUNT from WIRES in prog.wires */
        int wire_count;
} Source;

/* An input that a tick output ticks */
typedef struct Target {
        int node, pin; /* input PIN of node NODE */
        int wires, wire_count;
} Target;

/* What glows when something goes through: a wire, and the square it goes
 * into (NULL for data, only ticks light squares) */
typedef struct Wire {
        LinkList *list;
        int link;
        Square *to;
} Wire;

typedef struct Node {
        Square *square;
        Graph *scope;   /* whose locals it uses, NULL for the canvas's (see subgraphs.c) */
        int inputs;     /* its first in prog.sources, one per input */
        int outputs;    /* its first in prog.outs, one per output */
        int level;      /* the canvas it's in, while lowering */
        unsigned merge; /* its tick inputs with more than one wire, a bit each: see branches.h */
} Node;

/* The targets of an output: COUNT from FIRST in prog.targets, none for data */
typedef struct Out {
        int first, count;
} Out;

typedef struct Program {
        Da(Node) nodes; /* in the order they get module_start and module_frame */
        Da(Source) sources;
        Da(Out) outs;
        Da(Target) targets;
        Da(Wire) wires;
        long long version; /* squares_version it was lowered for */
} Program;

extern Program prog;

/* A canvas being lowered: the tab's, or the one inside a subgraph square */
typedef struct Level {
        SquareList *squares;
        LinkList *links;
        Graph *graph;  /* NULL for the tab's */
        Square *owner; /* the subgraph square */
        int up;        /* the level it is in, -1 for the tab's */
} Level;

typedef Da(Level) LevelsDa;

extern LevelsDa levels;

typedef Da(Wire) LowerPathDa;

extern LowerPathDa lower_path;

/* The branches of the tab shown (see branches.h): every tab has its own, so
 * one can stop stepping while others run */
extern Sched *sched;

struct HistoryState {
        Da(char *) undo;
        Da(char *) redo;
        char *current;
};

/* Snapshots (canvas_text without camera or state), oldest first */
extern struct HistoryState history;

/* Copies only live here: pasting runs the text as Lua, so the system
 * clipboard (anything copied from anywhere) is never read */
extern char *clipboard;

enum ActCompileState {
        ACT_STEP,
        ACT_RUN,
        ACT_COMPILE,
};

/* The run button does the last one chosen in its list, Step at first */
extern enum ActCompileState bar_act;

enum { BAR_ACT  = 1,
       BAR_FILE = 3 };

extern const char *config_path;

extern long long config_mtime, settings_mtime;

/* Every half second: the config, settings.lua, the modules and lineit */

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
        Typeof(history) history;
        Program prog;
        bool running, stepping;
        Sched *sched;     /* its branches, see branches.h */
        const char *path; /* canvas_path: NULL until it's saved somewhere */
        bool settings;    /* the settings tab: its canvas is empty and not shown */

        char *saved; /* canvas_text(0) when it was last loaded or saved, to see changes */
} Tab;

typedef Da(Tab *) TabsDa;

extern TabsDa tabs;

extern int tab_cur;

extern bool handed_over;

extern bool quitting;

#define TAB_CLOSE "x"

struct PromptState {
        bool open;
        const char *label;
        TextInput input;
        void (*done)(const char *text);
        char matches[1024]; /* what Tab found, shown below */
        const char *hint;   /* shown below while nothing else is, NULL: the default */
        bool no_complete;   /* Tab doesn't complete paths: it isn't one */
};

extern struct PromptState prompt;

struct AskState {
        bool open;
        Tab *tab; /* the one closing, NULL: the window */
        char text[256];
        int choice; /* the button Enter presses */
};

/* Closing a tab or the window with unsaved changes asks: Save or Don't
 * save. The x at its top right (or Esc) leaves everything as it was. Enter
 * presses the button chosen, Save at first; Tab chooses the next one.
 * Saving an untitled one asks where first (Save as). */
extern struct AskState ask;

extern int tab_socket;

/* ---- keys.c: the keys pressed this frame ---- */

void keys_poll(void);
bool key_pressed(int key);

/* ---- themes.c: the built-in themes and the config's own ---- */

bool theme_colors(Conf conf, const char *name, Colors *out, int depth);
void user_themes_read(Conf conf);

/* ---- config.c: the config: its keys, config.lua and settings.lua, read again when they change ---- */

bool conf_color(Conf conf, Color *val, const char *path);
char *setting_lua(const Setting *s, const void *val);
bool setting_read(Conf conf, const Setting *s, const char *path);
void config_reload(void);

/* ---- paths.c: where lineit finds its config, modules and runtime ---- */

const char *config_start(const char *given);
bool load_config(const char *path, bool required);

/* ---- version.c: versions of canvases, packages and lineit ---- */

bool conf_version(Conf conf, const char *key, Version *v, char *why, int size);
bool version_ok(Version made, const char *what, char *why, int size);

/* ---- output.c: the output panel: what print writes ---- */

void output_append(const char *text, bool info);
void output_add(const char *text, bool info);
void output_clear(void);
Rectangle output_rect(void);
bool update_output(void);
void draw_output(void);

/* ---- module_api.c: the module API (lineit.h): pins, values, text, params ---- */

void pin_store(PinInfo *p, Li_Value value);
int li_input(Li_Node *n, const char *name, Li_Type type);
void li_pins_reset(Li_Node *n);
int li_input_value(Li_Node *n, const char *name, Li_Type type, Li_Value value);
int li_input_options(Li_Node *n, const char *name, const char *options, int chosen);
const char *li_option(Li_Node *n, int input);
int li_output(Li_Node *n, const char *name, Li_Type type);
void li_output_type(Li_Node *n, int output, Li_Type type);
void li_input_type(Li_Node *n, int input, Li_Type type);
void li_set(Li_Node *n, int output, Li_Value value);
void *li_data(Li_Node *n);
void li_set_data(Li_Node *n, void *data);
void li_text(Li_Node *n, const char *fmt, ...);
void li_print(Li_Node *n, const char *fmt, ...);
double li_time(void);
int li_param(Li_Node *n, const char *name, Li_Value value);
Li_Value li_get_param(Li_Node *n, int param);

/* ---- records.c: custom types: records, and the names of the types ---- */

TypeDef *type_def(Li_Type t);
bool type_is_record(Li_Type t);
const char *type_field_name(Li_Type t, int field);
int type_field_count(Li_Type t);
const char *type_field_type(Li_Type t, int field);
Li_Type type_named(const char *name);
const char *type_name(Li_Type t);
int type_from_name(const char *name);
Color type_color(Li_Type t);

/* ---- fields.c: typed values: params and input fields, and typing in them ---- */

void param_text(Param *p, char *buf, int size, bool exact);
bool param_set_text(Square *s, int i, const char *text);
void input_text(Square *s, int i, char *buf, int size, bool exact);
bool input_set_text(Square *s, int i, const char *text, Li_Type type);
int input_find(Square *s, const char *name);
void field_edit_start(Square *s, int i, bool param);
bool field_typing(Square *s, int i, bool param);
bool update_field(void);
void option_choose(int id, void *arg);
int param_find(Square *s, const char *name);

/* ---- text_input.c: typing text: one line, with a cursor ---- */

void text_input_set(TextInput *t, const char *text);
void text_input_sync(TextInput *t);
TextResult text_input_update(TextInput *t, bool *changed);
void text_input_draw(const TextInput *t, Rectangle r, float fs, Color color, bool canvas);
int text_input_hit(const TextInput *t, Rectangle r, float fs, bool canvas, float x);
float text_input_wrapped_height(const TextInput *t, float w, float fs, float lh);
void text_input_draw_wrapped(const TextInput *t, Rectangle r, float fs, float lh, Color color);
int text_input_hit_wrapped(const TextInput *t, Rectangle r, float fs, float lh, Vector2 p);
void text_input_click(TextInput *t, int at);
void text_input_drag(TextInput *t, int at);

/* ---- squares.c: squares: made from a module, freed, disconnected ---- */

Square *square_new_id(Module *m, Vector2 center, int id);
Square *square_new(Module *m, Vector2 center);
void pins_free(PinList *l);
void square_free(Square *s);
void square_disconnect_inputs(int id, void *arg);
void square_disconnect_outputs(int id, void *arg);
void square_disconnect_all(int id, void *arg);
void square_configure(int id, void *arg);
void square_delete(int id, void *arg);

/* ---- modules.c: modules: finding, loading and checking them ---- */

void *module_open(const char *path, ModuleFns *fn);
int modules_dirs(const char *dirs[2]);
char *module_name_of(const char *path, const char *dir, size_t ext);
void modules_scan(bool reload);
void load_modules(const char *dir);
void unload_modules(void);
Module *module_find(const char *name);
void module_reload_squares(Module *m);

/* ---- packages.c: packages: modules in a .li ---- */

bool package_extract(const unsigned char *zip, size_t zsize, const char *dir, char *why, int why_size);
bool package_make(const char *dir, const char *rule, const char *out);
char *package_build(const char *path, const unsigned char *zip, size_t size, char *why, int why_size);
char *package_lib(const char *path, char *why, int why_size);

/* ---- types.c: types: the files that make them, and their make and break squares ---- */

bool is_type_file(const char *path);
Color type_color_for(const char *name);
void fields_free(void *list);
bool field_type_known(const char *name, char **known, int nknown);
void type_module_name(const char *type, const char *what, char *buf, int size);
void types_scan(bool reload, bool *menu_changed);
void types_free(void);

/* ---- menu.c: menus: the canvas menu, submenus and search ---- */

TextInput *menu_search(void);
Menu *menu_submenu(Menu *m, const char *label);
void menu_free(Menu *m);
int module_cmp(const void *a, const void *b);
void menu_open(Menu *m, int id, Vector2 pos);
float menu_item_height(void);
Rectangle menu_rect(int lvl);
void menu_search_open(Menu *m, Vector2 pos);
Rectangle menu_search_rect(void);
bool update_menu(void);
void draw_menu(void);

/* ---- panel.c: the settings panel of a square: its params ---- */

void panel_open(Square *s);
void panel_close(void);
bool update_panel(void);
void draw_panel(void);

/* ---- save.c: saving and loading canvases ---- */

void lua_quote(FILE *f, const char *s);
void canvas_write(FILE *f, int flags);
bool canvas_save(const char *path);
char *canvas_text(int flags);
int pin_named(PinList *l, const char *name);
bool canvas_check(const char *path, char *why, int size);
bool canvas_read_file(const char *path, ReadMode mode);
bool canvas_read_text(const char *text, ReadMode mode);

/* ---- subgraphs.c: subgraphs: canvases used as modules, and their ports ---- */

void graph_swap(Graph *g);
Graph *scope_enter(Graph *g);
void graph_builtins(void);

/* ---- decls.c: declarations: var/var squares of one variable agree ---- */

bool is_decl(Square *s);
const char *decls_check(void);
void vars_reset(void);

/* ---- program.c: the program: the canvas lowered, and what follows its wires (li_get, li_tick, branches) ---- */

void program_free(Program *p);
void program_lower(void);
int got_mark(void);
void got_free(int mark);
Li_Value li_get(Li_Node *n, int input);
Sched *sched_get(void);
void li_tick(Li_Node *n, int output);
bool square_is_ticked(Square *s);
bool program_each(bool start);

/* ---- compile.c: Compile: the program, as C ---- */

bool compile_canvas(const char *name, char *bin, int size);
int compile_file(const char *config, const char *path);
bool square_uses(Square *s, Module *m);

/* ---- undo.c: undo and redo ---- */

void history_free_list(void *list);
void history_reset(void);
bool editing_text(void);
void history_check(void);
void history_step(bool undo);
void history_destroy(void);

/* ---- selection.c: the selection: copy, paste, duplicate ---- */

void select_all(bool on);
void each_selected(void (*fn)(int id, void *arg));
int selected_count(void);
void selection_copy(void);
void selection_paste(Vector2 at);
void selection_duplicate(void);

/* ---- tabs.c: tabs: a canvas each ---- */

void tab_visit(int i);
const char *tab_name(int i);
bool tab_dirty(int i);
void tab_mark_saved(void);
void tab_title(void);
void tab_switch(int i);
void tab_add(const char *path, bool is_settings);
void tab_new(const char *path);
void closed_tabs_free(void);
void tab_close(int i, bool force);
void tab_open(const char *path);
void tab_reopen(void);
void tabs_run_background(void);
void square_open_tab(int id, void *arg);
float tabs_height(void);

/* ---- tab_strip.c: the tab strip, above the canvas ---- */

bool update_tabs(void);
void draw_tabs(void);

/* ---- prompt.c: the prompt: a path typed for Open and Save as ---- */

void prompt_open(const char *label, const char *text, void (*done)(const char *));
bool update_prompt(void);
void draw_prompt(void);

/* ---- file_menu.c: the File menu, and the menu of the run button ---- */

void file_save_as_done(const char *path);
const char *file_dir(void);
void file_new(void);
void file_open(void);
void file_save_as(void);
void file_close(void);
void file_next_tab(int step);
void file_menu_open(void);
void act_menu_open(void);

/* ---- settings_tab.c: the Settings tab: every setting, searched and changed ---- */

bool settings_shown(void);
bool not_canvas(void);
void settings_open(void);
void settings_edit_end(void);
void update_settings(void);
void draw_settings(void);

/* ---- make_subgraph.c: Make subgraph: the selection becomes a module ---- */

void subgraph_make(const char *typed);
void subgraph_ask(void);
void menu_subgraph(int id, void *arg);

/* ---- type_edit.c: New type and Edit type ---- */

void type_new_ask(void);
void type_edit(int id, void *arg);

/* ---- ask.c: unsaved changes: asked before they are lost ---- */

bool tab_save(int i);
void ask_close_tab(int i);
void quit_request(void);
bool update_ask(void);
void draw_ask(void);

/* ---- autosave.c: autosave ---- */

void autosave_tick(void);

/* ---- single.c: one editor: ./lineit --tab FILE opens FILE in the one running ---- */

bool tab_socket_send(const char *path);
void tab_socket_listen(void);
void tab_socket_poll(void);
void tab_socket_close(void);
bool prompt_is_open(void);

/* ---- reload.c: reloading the editor itself, and the session it hands over ---- */

void exe_watch(void);
void exe_ready_check(void);
void session_window(const char *path);
bool session_restore(const char *path);
void hot_reload(void);

/* ---- bar.c: the bar: Run, Step, Compile, Save, Load and the status ---- */

void set_status(const char *fmt, ...) LI_PRINTF(1, 2);
void bar_save(void);
void bar_load(void);
void bar_run_stop(void);
void bar_step(void);
void bar_act_do(void);
Rectangle bar_button_rect(int i);
bool update_bar(void);
void draw_bar(void);

/* ---- canvas.c: the canvas: its squares, links and camera ---- */

Rectangle canvas_rect(void);
Vector2 mouse_world(void);
Square *square_get(int id);
int square_index(Square *s);
void canvas_add_module(int id, void *arg);
void canvas_reset_view(int id, void *arg);
void canvas_clear(int id, void *arg);

/* ---- context_menu.c: the menus of a square and of the canvas ---- */

void square_menu_build(Square *s);
void canvas_menu_build(void);

/* ---- draw.c: drawing the canvas: the grid, wires and squares ---- */

float glow(double t);
int link_hit(Vector2 p);
void draw_canvas(void);

/* ---- input.c: the mouse on the canvas: dragging, selecting, the camera ---- */

Rectangle drag_box(void);
void update_mouse(void);
void update_camera(void);

/* ---- layout.c: where the parts of a square are: its pins, fields and params ---- */

float square_text_band(void);
float pin_label_gap(void);
float field_width(PinInfo *p);
float params_height(Square *s);
float square_side(Square *s);
Rectangle square_rect(Square *s);
PinInfo *pin_info(Square *s, Part part, int i);
Vector2 pin_pos(Square *s, Part part, int i);
Rectangle field_rect(Square *s, int i);
Rectangle field_text_rect(Rectangle f, bool options);
Rectangle field_editor_rect(void);
void draw_field_editor(void);
float param_y(Square *s, int i);
Rectangle param_rect(Square *s, int i);
bool field_visible(Square *s, int i);
Hit square_hit(Vector2 p);

/* ---- links.c: links: which wires are allowed, and making them ---- */

int link_to(int id, int pin);
bool pin_connected(Square *s, Part part, int pin);
bool types_compatible(Li_Type out, Li_Type in);
bool link_allowed(Square *from, int from_pin, Square *to, int to_pin);
bool link_valid(Link k);
void link_add(Square *from, int from_pin, Square *to, int to_pin);
void links_check(Square *s);

/* ---- src/main.c: main: the window, and every frame ---- */

int main(int argc, char **argv);

/* ---- run.c: Run and Step: starting, stepping and stopping the program ---- */

void run_frame(void);
void run_stop(void);
void run_check_end(void);
void squares_settle(void);
bool run_begin(void);
bool step_next(Square *s);
const char *step_names(void);

/* ---- shortcuts.c: keyboard shortcuts ---- */

void update_shortcuts(void);

/* ---- text.c: fonts and text ---- */

void load_font(void);
void unload_fonts(void);
void draw_text(const char *text, float x, float y, float size, Color color);
float measure_text(const char *text, float size);
void draw_canvas_text(const char *text, float x, float y, float size, Color color);
float measure_canvas_text(const char *text, float size);
void fit_canvas_text(const char *in, char *out, int size, float max, float fs);
const char *text_fit(const char *text, float size, float w);

/* ---- tooltips.c: tooltips of pins ---- */

void draw_tooltip(void);

/* ---- util.c: files, processes and hashes ---- */

bool copy_file(const char *from, const char *to);
bool tmp_dir_make(void);
long long file_mtime(const char *path);
long long now_ns(void);
unsigned le16(const unsigned char *p);
unsigned long le32(const unsigned char *p);
bool write_if_changed(const char *path, const void *data, size_t len);
bool mkdir_p(const char *dir);
void shell_quote(char *buf, int size, const char *s);
bool run_to_output(const char *cmd);
bool run_to_buf(const char *cmd, char *buf, int size);
bool sha256_hex(const void *data, size_t len, char hex[65]);
int find_icase(const char *hay, const char *needle);
void file_name(char *buf, int size, const char *name);
int str_cmp(const void *a, const void *b);
void mkdir_parents(const char *path);

#endif /* EDITOR_H_ */

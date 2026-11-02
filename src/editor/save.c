/* save.c - saving and loading canvases */
#include "editor.h"

/* File used by Save and Load */
const char *canvas_path;

/* Write S as a Lua string literal */
void
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

/* The canvas as Lua: camera, squares (draw order) and links. Squares keep
 * their params and typed inputs. */
/* Canvases say the LI_VERSION that wrote them (lineit_canvas), and
 * canvas_read reads the ones of the same major and an older minor (see
 * Versions) */

void
canvas_write(FILE *f, int flags)
{
        fprintf(f, "lineit_canvas = \"%s\" -- the lineit that wrote it, see LI_VERSION\n\n", LI_VERSION);
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
                                if (in->kind == PIN_VALUE) fprintf(f, ", type = \"%s\"", type_name(in->value.type));
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

bool
canvas_save(const char *path)
{
        FILE *f = fopen(path, "w");
        if (!f) return false;
        fprintf(f, "-- lineit canvas\n");
        canvas_write(f, WRITE_CAMERA | WRITE_STATE);
        return fclose(f) == 0;
}

/* canvas_write to a string. Must be freed. */
char *
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

/* Index of the pin of L named NAME, -1 if none is */
int
pin_named(PinList *l, const char *name)
{
        for (int i = 0; i < l->count; i++)
                if (!strcmp(l->items[i].name, name)) return i;
        return -1;
}

/* Where READ_PASTE puts the top-left corner of what it pastes */
Vector2 paste_at;

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

/* Whether CONF is a canvas: a squares table, with a module in each square,
 * and a links table if there are links, of a version this lineit reads. If
 * not, why in WHY. */
static bool
canvas_conf_ok(Conf conf, char *why, int size)
{
        int n = 0, links_len = 0;
        Version version;
        const char *str;
        if (Conf_get_len(conf, &n, "squares") != CONF_OK) {
                snprintf(why, size, "not a lineit canvas, it has no squares table");
                return false;
        }
        if (!conf_version(conf, "lineit_canvas", &version, why, size)) return false;
        if (!version_ok(version, "canvas", why, size)) return false;
        for (int i = 1; i <= n; i++) {
                if (Conf_get_str(conf, &str, "squares.%d.module", i) != CONF_OK) {
                        snprintf(why, size, "square %d has no module", i);
                        return false;
                }
        }
        int err = Conf_get_len(conf, &links_len, "links");
        if (err != CONF_OK && err != CONF_UNDEF) {
                snprintf(why, size, "links isn't a table");
                return false;
        }
        return true;
}

/* Whether PATH is a canvas file to open. An empty file is an empty
 * canvas. If not, why in WHY. */
bool
canvas_check(const char *path, char *why, int size)
{
        Conf conf;
        if (DirectoryExists(path)) {
                snprintf(why, size, "it's a folder");
                return false;
        }
        if (!FileExists(path)) {
                snprintf(why, size, "it isn't there");
                return false;
        }
        if (GetFileLength(path) == 0) return true;
        if (Conf_open(&conf, path) != CONF_OK) {
                snprintf(why, size, "it isn't valid Lua (the error is in the terminal)");
                return false;
        }
        bool ok = canvas_conf_ok(conf, why, size);
        Conf_close(conf);
        return ok;
}

/* Replace the canvas with the one in PATH. Squares of missing modules are
 * skipped with their wires. Returns false, changing nothing, if the file
 * can't be read or isn't a canvas (see canvas_check). */
bool
canvas_read_file(const char *path, ReadMode mode)
{
        Conf conf;
        char why[128];
        if (Conf_open(&conf, path) != CONF_OK) return false;
        if (!canvas_conf_ok(conf, why, sizeof why)) {
                fprintf(stderr, "load: %s: %s\n", path, why);
                Conf_close(conf);
                return false;
        }
        canvas_read(conf, mode);
        Conf_close(conf);
        return true;
}

/* Same, from what canvas_text returned (conf.h only reads files) */
bool
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

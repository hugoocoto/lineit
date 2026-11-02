/* types.c - types: the files that make them, and their make and break squares */
#include "editor.h"

/* A custom type (see records.c) is a NAME.type.lua under the modules
 * dir, made with File > New type, Edit type... on its squares, or by hand:
 *
 *   lineit_type = "0.0.1"
 *   fields = {
 *           { name = "x", type = "float" },
 *           { name = "y", type = "float" },
 *   }
 *   color = { 251, 146, 60 } -- of its wires, optional
 *
 * modules/geo/point.type.lua is the type geo/point, with two modules next
 * to it: geo/make_point (its fields in, a point out) and geo/break_point (a
 * point in, its fields out). Fields are bool, int, float, string, any,
 * ptr, or another custom type, by its name; and, for a type that's a C
 * struct (see li_c_size), char, int8..int64, uint8..uint64, float32,
 * float64: ints and floats in lineit, of their size in C. */

/* Type files say the LI_VERSION that wrote them (lineit_type), and
 * type_read reads the ones of the same major and an older minor (see
 * Versions) */

bool
is_type_file(const char *path)
{
        size_t n = strlen(path);
        return n > 9 && !strcmp(path + n - 9, ".type.lua");
}

/* What a field can be: data, not ticks */
static bool
field_type_ok(int t)
{
        return t == LI_BOOL || t == LI_INT || t == LI_FLOAT || t == LI_STRING || t == LI_ANY || t == LI_PTR ||
               type_is_record(t);
}

/* Field types with a size, for records that are C structs (see li_c_size):
 * in lineit they're ints, floats and ptrs */
static const struct {
        const char *name;
        Li_Type type;
} c_field_types[] = {
        { "char", LI_INT },
        { "int8", LI_INT },
        { "uint8", LI_INT },
        { "int16", LI_INT },
        { "uint16", LI_INT },
        { "int32", LI_INT },
        { "uint32", LI_INT },
        { "int64", LI_INT },
        { "uint64", LI_INT },
        { "float32", LI_FLOAT },
        { "float64", LI_FLOAT },
};

static int
field_type_named(const char *name)
{
        for (int i = 0; i < countof(c_field_types); i++)
                if (!strcmp(c_field_types[i].name, name)) return c_field_types[i].type;
        return type_from_name(name);
}

static Li_Type
field_type(Field *f)
{
        int t = field_type_named(f->type);
        return t >= 0 ? (Li_Type) t : LI_ANY;
}

/* What a field starts as, typed in the make square */
static Li_Value
field_default(Li_Type t)
{
        switch (t) {
        case LI_BOOL: return li_bool(false);
        case LI_INT: return li_int(0);
        case LI_FLOAT: return li_float(0);
        case LI_STRING: return li_string("");
        default: return (Li_Value) { .type = LI_NONE };
        }
}

/* make_NAME: its fields in, typed in the square or wired, a record out. The
 * record is the square's; it's made again only when a field changes, so
 * what was read from it lives until then. */
static void
type_make_init(Li_Node *n)
{
        TypeDef *d = type_def(n->module->type);
        for (int i = 0; d && i < d->fields.count; i++) {
                Field *f  = &d->fields.items[i];
                Li_Type t = field_type(f);
                if (type_is_record(t))
                        li_input(n, f->name, t);
                else
                        li_input_value(n, f->name, t, field_default(t));
        }
        li_output(n, d ? d->label : "value", n->module->type);
}

static void
type_make_eval(Li_Node *n)
{
        Li_Type t  = n->module->type;
        TypeDef *d = type_def(t);
        if (!d) return;
        Record *r = malloc(sizeof *r + d->fields.count * sizeof *r->fields);
        r->type   = t;
        r->count  = d->fields.count;
        for (int i = 0; i < r->count; i++)
                r->fields[i] = value_dup(li_get(n, i));

        /* The same as before: keep that one, what was read from it lives */
        Li_Value now = { .type = t, .p = r };
        Li_Value old = { .type = t, .p = n->data };
        if (n->data && value_equal(old, now)) {
                for (int i = 0; i < r->count; i++)
                        value_drop(r->fields[i]);
                free(r);
        } else {
                n->data = r;
                value_drop(old);
        }
        Li_Value v = { .type = t, .p = n->data };
        char buf[64];
        li_set(n, 0, v);
        li_format(v, buf, sizeof buf);
        li_text(n, "%s", buf);
}

static void
type_make_destroy(Li_Node *n)
{
        value_drop((Li_Value) { .type = n->module->type, .p = n->data });
        n->data = NULL;
}

/* break_NAME: a record in, its fields out. They are the record's: they live
 * while it does. */
static void
type_break_init(Li_Node *n)
{
        TypeDef *d = type_def(n->module->type);
        li_input(n, d ? d->label : "value", n->module->type);
        for (int i = 0; d && i < d->fields.count; i++)
                li_output(n, d->fields.items[i].name, field_type(&d->fields.items[i]));
}

static void
type_break_eval(Li_Node *n)
{
        TypeDef *d = type_def(n->module->type);
        Li_Value v = li_get(n, 0);
        Record *r  = v.type == n->module->type ? v.p : NULL;
        for (int i = 0; d && i < d->fields.count; i++)
                li_set(n, i, r && i < r->count ? r->fields[i] : (Li_Value) { .type = LI_NONE });
}

const ModuleFns type_make_fns  = { .init = type_make_init, .eval = type_make_eval, .draw = type_make_eval, .destroy = type_make_destroy };
const ModuleFns type_break_fns = { .init = type_break_init, .eval = type_break_eval };

/* A color from NAME, for types that don't pick one */
Color
type_color_for(const char *name)
{
        unsigned h = 2166136261u;
        for (const char *c = name; *c; c++)
                h = (h ^ (unsigned char) *c) * 16777619u;
        return ColorFromHSV(h % 360, 0.55f, 0.95f);
}

static int
type_index(TypeDef *d)
{
        for (int i = 0; i < types.count; i++)
                if (types.items[i] == d) return i;
        return -1;
}

void
fields_free(void *list)
{
        Da(Field) *l = list;
        Da_foreach(f, *l)
        {
                free(f->name);
                free(f->type);
        }
        Da_destroy(l);
}

/* Is NAME a field type, KNOWN being the names of the types there are
 * (the ones of this scan too, which may not be read yet)? */
bool
field_type_known(const char *name, char **known, int nknown)
{
        int t = field_type_named(name);
        if (t >= 0) return field_type_ok(t);
        for (int i = 0; i < nknown; i++) {
                const char *label = strrchr(known[i], '/') ? strrchr(known[i], '/') + 1 : known[i];
                if (!strcmp(known[i], name) || !strcmp(label, name)) return true;
        }
        return false;
}

/* The fields and color of the type file PATH, into D. False, with why in
 * WHY, if it isn't a type. */
static bool
type_read(const char *path, TypeDef *d, char **known, int nknown, char *why, int size)
{
        Conf conf;
        int n = 0;
        Version version;
        if (Conf_open(&conf, path) != CONF_OK) {
                snprintf(why, size, "it isn't valid Lua");
                return false;
        }
        bool ok = conf_version(conf, "lineit_type", &version, why, size);
        ok      = ok && version_ok(version, "type file", why, size);
        if (ok && !(ok = Conf_get_len(conf, &n, "fields") == CONF_OK)) snprintf(why, size, "it has no fields table");
        for (int i = 1; ok && i <= n; i++) {
                const char *name, *type;
                char nbuf[64];
                if (Conf_get_str(conf, &name, "fields.%d.name", i) != CONF_OK || !*name) {
                        snprintf(why, size, "field %d has no name", i);
                        ok = false;
                        break;
                }
                snprintf(nbuf, sizeof nbuf, "%s", name);
                if (Conf_get_str(conf, &type, "fields.%d.type", i) != CONF_OK) type = "any";
                if (!field_type_known(type, known, nknown)) {
                        snprintf(why, size, "field %s: no type %s", nbuf, type);
                        ok = false;
                        break;
                }
                Da_append(&d->fields, (Field) { strdup(nbuf), strdup(type) });
        }
        d->color    = type_color_for(path);
        double c[4] = { 0, 0, 0, 255 };
        int nc      = 0;
        if (ok && Conf_get_len(conf, &nc, "color") == CONF_OK && nc >= 3) {
                for (int i = 0; i < nc && i < 4; i++)
                        Conf_get_num(conf, &c[i], "color.%d", i + 1);
                d->color = (Color) { c[0], c[1], c[2], c[3] };
        }
        Conf_close(conf);
        if (!ok) fields_free(&d->fields);
        return ok;
}

/* "geo/point" -> "geo/make_point" (WHAT "make" or "break") */
void
type_module_name(const char *type, const char *what, char *buf, int size)
{
        const char *slash = strrchr(type, '/');
        if (slash)
                snprintf(buf, size, "%.*s/%s_%s", (int) (slash - type), type, what, slash + 1);
        else
                snprintf(buf, size, "%s_%s", what, type);
}

static Module *
type_module(TypeDef *d, Li_Type t, const char *what, const char *path, const ModuleFns *fns, bool *added)
{
        char name[PATH_MAX];
        type_module_name(d->name, what, name, sizeof name);
        Module *m = module_find(name);
        if (m) return m;
        m          = calloc(1, sizeof *m);
        m->name    = strdup(name);
        m->label   = strrchr(m->name, '/') ? strrchr(m->name, '/') + 1 : m->name;
        m->path    = strdup(path);
        m->builtin = fns;
        m->type    = t;
        Da_append(&modules, m);
        *added = true;
        return m;
}

/* Every .type.lua in the modules dirs: new ones get their modules, changed
 * ones are read again and their squares made again. Like modules_scan,
 * which calls it. */
void
types_scan(bool reload, bool *menu_changed)
{
        long long now = now_ns();
        const char *dirs[2];
        int ndirs        = modules_dirs(dirs);
        Da(char *) known = { 0 }; /* their names */
        Da(char *) paths = { 0 }; /* and their files, the first dir's if in both */

        for (int di = 0; di < ndirs; di++) {
                if (!DirectoryExists(dirs[di])) continue;
                FilePathList files = LoadDirectoryFilesEx(dirs[di], ".lua", true);
                for (unsigned i = 0; i < files.count; i++) {
                        if (!is_type_file(files.paths[i])) continue;
                        char *name = module_name_of(files.paths[i], dirs[di], 9);
                        bool dup   = false;
                        Da_foreach(n, known)
                        {
                                dup = dup || !strcmp(*n, name);
                        }
                        if (dup) {
                                free(name);
                                continue;
                        }
                        Da_append(&known, name);
                        Da_append(&paths, strdup(files.paths[i]));
                }
                UnloadDirectoryFiles(files);
        }

        for (int k = 0; k < known.count; k++) {
                const char *path = paths.items[k];
                const char *name = known.items[k];
                long long mtime  = file_mtime(path);
                TypeDef *d       = NULL;
                char make_name[PATH_MAX], break_name[PATH_MAX], why[256];
                for (int t = 0; t < types.count; t++)
                        if (!strcmp(types.items[t]->name, name)) d = types.items[t];
                type_module_name(name, "make", make_name, sizeof make_name);
                type_module_name(name, "break", break_name, sizeof break_name);
                Module *make = module_find(make_name), *brk = module_find(break_name);

                /* Known and unchanged, or still being written */
                if (d && make && brk && (mtime == make->mtime || (reload && now - mtime < MODULE_SETTLE_NS))) {
                        make->seen = brk->seen = true;
                        if (make->gone || brk->gone) *menu_changed = true;
                        make->gone = brk->gone = false;
                        continue;
                }
                if (reload && now - mtime < MODULE_SETTLE_NS) continue;

                TypeDef read = { 0 };
                if (!type_read(path, &read, known.items, known.count, why, sizeof why)) {
                        fprintf(stderr, "types: %s skipped: %s\n", path, why);
                        if (reload) set_status("Type %s: %s", name, why);
                        if (make) make->mtime = mtime, make->seen = true;
                        if (brk) brk->mtime = mtime, brk->seen = true;
                        continue;
                }
                bool existed = d != NULL;
                if (!d) {
                        d        = calloc(1, sizeof *d);
                        d->name  = strdup(name);
                        d->label = strrchr(d->name, '/') ? strrchr(d->name, '/') + 1 : d->name;
                        Da_append(&types, d);
                }
                fields_free(&d->fields);
                d->fields = read.fields;
                d->color  = read.color;

                Li_Type t   = LI_TYPE_COUNT + type_index(d);
                make        = type_module(d, t, "make", path, &type_make_fns, menu_changed);
                brk         = type_module(d, t, "break", path, &type_break_fns, menu_changed);
                make->mtime = brk->mtime = mtime;
                make->seen = brk->seen = true;
                if (make->gone || brk->gone) *menu_changed = true;
                make->gone = brk->gone = false;
                if (existed) {
                        module_reload_squares(make);
                        module_reload_squares(brk);
                        set_status("Reloaded type %s", name);
                } else if (reload) {
                        set_status("Added type %s", name);
                }
        }
        for (int k = 0; k < known.count; k++) {
                free(known.items[k]);
                free(paths.items[k]);
        }
        Da_destroy(&known);
        Da_destroy(&paths);
}

void
types_free(void)
{
        Da_foreach(dp, types)
        {
                fields_free(&(*dp)->fields);
                free((*dp)->name);
                free(*dp);
        }
        Da_destroy(&types);
}

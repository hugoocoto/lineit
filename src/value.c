/* value.c - see value.h */
#define _GNU_SOURCE  /* strdup, recursive mutex initializer */
#include <pthread.h> /* first: bt.h changes the feature macros */
#define BT_IMPLEMENTATION
#include "value.h"

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* What .p of a LI_LIST and a LI_MAP point to. Each owns its values. */
typedef struct List {
        Da(Li_Value) items;
} List;

/* Sorted by key, found with bsearch */
typedef struct Entry {
        char *key;
        Li_Value value;
} Entry;

typedef struct Map {
        Da(Entry) items;
} Map;

/* A copy of V that owns what it points to (strings, lists, maps, records),
 * for whoever keeps it longer than the module that made it: free it with
 * value_drop */
Li_Value
value_dup(Li_Value v)
{
        if (v.type == LI_STRING) return li_string(strdup(v.s ? v.s : ""));
        if (v.type == LI_BYTES && v.p) return li_bytes(li_bytes_data(v), li_bytes_size(v));
        if (v.type == LI_LIST && v.p) {
                List *l = v.p, *c = calloc(1, sizeof *c);
                Da_foreach(it, l->items)
                {
                        Da_append(&c->items, value_dup(*it));
                }
                v.p = c;
                return v;
        }
        if (v.type == LI_MAP && v.p) {
                Map *m = v.p, *c = calloc(1, sizeof *c);
                Da_foreach(e, m->items)
                {
                        Da_append(&c->items, ((Entry) { strdup(e->key), value_dup(e->value) }));
                }
                v.p = c;
                return v;
        }
        if (type_is_record(v.type) && v.p) {
                Record *r = v.p;
                Record *c = malloc(sizeof *c + r->count * sizeof *c->fields);
                c->type   = r->type;
                c->count  = r->count;
                for (int i = 0; i < r->count; i++)
                        c->fields[i] = value_dup(r->fields[i]);
                v.p = c;
        }
        return v;
}

void
value_drop(Li_Value v)
{
        if (v.type == LI_STRING || v.type == LI_BYTES) {
                free(v.p); /* .s is .p */
        } else if (v.type == LI_LIST && v.p) {
                List *l = v.p;
                Da_foreach(it, l->items)
                {
                        value_drop(*it);
                }
                Da_destroy(&l->items);
                free(l);
        } else if (v.type == LI_MAP && v.p) {
                Map *m = v.p;
                Da_foreach(e, m->items)
                {
                        free(e->key);
                        value_drop(e->value);
                }
                Da_destroy(&m->items);
                free(m);
        } else if (type_is_record(v.type) && v.p) {
                Record *r = v.p;
                for (int i = 0; i < r->count; i++)
                        value_drop(r->fields[i]);
                free(r);
        }
}

/* V for an input of type T it's wired to, as C would: an int into a float,
 * a bool into an int (0, 1) and an int into a bool (not 0) */
Li_Value
value_widen(Li_Value v, Li_Type t)
{
        if (t == LI_FLOAT && v.type == LI_INT) return li_float((double) v.i);
        if (t == LI_INT && v.type == LI_BOOL) return li_int(v.b);
        if (t == LI_BOOL && v.type == LI_INT) return li_bool(v.i != 0);
        return v;
}

Li_Value
li_copy(Li_Value v)
{
        return value_dup(v);
}

void
li_drop(Li_Value *v)
{
        value_drop(*v);
        *v = (Li_Value) { .type = LI_NONE };
}

bool
value_equal(Li_Value a, Li_Value b)
{
        if (a.type != b.type) return false;
        switch (a.type) {
        case LI_BOOL: return a.b == b.b;
        case LI_INT: return a.i == b.i;
        case LI_FLOAT: return a.f == b.f;
        case LI_STRING:
        case LI_REF: return !strcmp(a.s ? a.s : "", b.s ? b.s : "");
        case LI_PTR: return a.p == b.p;
        case LI_BYTES:
                return li_bytes_size(a) == li_bytes_size(b) &&
                       !memcmp(li_bytes_data(a), li_bytes_data(b), li_bytes_size(a));
        case LI_LIST: {
                long n = li_list_len(a);
                if (n != li_list_len(b)) return false;
                for (long i = 0; i < n; i++)
                        if (!value_equal(li_list_get(a, i), li_list_get(b, i))) return false;
                return true;
        }
        case LI_MAP: {
                long n = li_map_len(a);
                if (n != li_map_len(b)) return false;
                for (long i = 0; i < n; i++) {
                        const char *k = li_map_key(a, i);
                        if (!li_map_has(b, k) || !value_equal(li_map_get(a, k), li_map_get(b, k))) return false;
                }
                return true;
        }
        default: break;
        }
        if (!type_is_record(a.type)) return true;
        Record *r = a.p, *s = b.p;
        if (!r || !s) return r == s;
        if (r->count != s->count) return false;
        for (int i = 0; i < r->count; i++)
                if (!value_equal(r->fields[i], s->fields[i])) return false;
        return true;
}

/* {x: 1, y: 2} */
static int
record_format(Record *r, char *buf, int size)
{
        int n = snprintf(buf, size, "{");
        for (int i = 0; i < r->count && n < size; i++) {
                n += snprintf(buf + n, size - n, "%s%s: ", i ? ", " : "", type_field_name(r->type, i));
                if (n < size) n += li_format(r->fields[i], buf + n, size - n);
        }
        if (n < size) n += snprintf(buf + n, size - n, "}");
        return n;
}

/* [1, "a"] and {a: 1, b: [2]}. Strings inside are quoted, so "1" isn't 1. */
static int
item_format(Li_Value v, char *buf, int size)
{
        if (v.type == LI_STRING) return snprintf(buf, size, "\"%s\"", v.s ? v.s : "");
        return li_format(v, buf, size);
}

static int
list_format(Li_Value v, char *buf, int size)
{
        int n = snprintf(buf, size, "[");
        for (long i = 0; i < li_list_len(v) && n < size; i++) {
                if (i) n += snprintf(buf + n, size - n, ", ");
                if (n < size) n += item_format(li_list_get(v, i), buf + n, size - n);
        }
        if (n < size) n += snprintf(buf + n, size - n, "]");
        return n;
}

static int
map_format(Li_Value v, char *buf, int size)
{
        int n = snprintf(buf, size, "{");
        for (long i = 0; i < li_map_len(v) && n < size; i++) {
                const char *k = li_map_key(v, i);
                n += snprintf(buf + n, size - n, "%s%s: ", i ? ", " : "", k);
                if (n < size) n += item_format(li_map_get(v, k), buf + n, size - n);
        }
        if (n < size) n += snprintf(buf + n, size - n, "}");
        return n;
}

/* Variables, by name. Modules can't share globals (each square has its own
 * copy), so they live here. Like in C, they are local to the canvas they
 * are in: a subgraph square has its own (see Subgraphs), the canvas of a
 * tab its own. Names that start with "::" are globals, shared by all of
 * them in the tab. */
VarList vars;    /* the canvas's: the tab's, or a subgraph's while it's being made */
VarList globals; /* of the tab */

/* Where the thread is: the locals of the subgraph square it runs a square
 * of (NULL: vars). Each thread has its own, branches run in parallel. */
static _Thread_local VarList *here;

void
value_locals(VarList *l)
{
        here = l;
}

/* Branches run in parallel: one at a time in here. Recursive, so a
 * li_var_update callback can read another variable. */
static pthread_mutex_t lock = PTHREAD_RECURSIVE_MUTEX_INITIALIZER_NP;

#define GLOBAL "::"

/* The variable NAME, made if CREATE and it isn't there. With the lock. */
static Var *
var_find(const char *name, bool create)
{
        VarList *l = here ? here : &vars;
        if (!name) name = "";
        if (!strncmp(name, GLOBAL, strlen(GLOBAL))) {
                l = &globals;
                name += strlen(GLOBAL);
        }
        Da_foreach(v, *l)
        {
                if (strcmp(v->name, name) == 0) return v;
        }
        if (!create) return NULL;
        Da_append(l, ((Var) { .name = strdup(name), .type = LI_ANY }));
        return &l->items[l->count - 1];
}

/* FN on the value of NAME, in place, with the lock: nobody else touches it
 * meanwhile. Never set, it's LI_NONE. */
void
li_var_update(const char *name, void (*fn)(Li_Value *value, void *ctx), void *ctx)
{
        pthread_mutex_lock(&lock);
        Var *v = var_find(name, true);
        v->set = true;
        fn(&v->value, ctx);
        pthread_mutex_unlock(&lock);
}

void
li_var_set(const char *name, Li_Value value)
{
        /* Copied first: the module's strings and lists may not live long,
         * and VALUE may be in the variable */
        Li_Value c = value_dup(value);
        pthread_mutex_lock(&lock);
        Var *v       = var_find(name, true);
        Li_Value old = v->value;
        v->value     = c;
        v->set       = true;
        pthread_mutex_unlock(&lock);
        value_drop(old);
}

Li_Value
li_var_get(const char *name)
{
        pthread_mutex_lock(&lock);
        Var *v     = var_find(name, false);
        Li_Value c = v ? value_dup(v->value) : (Li_Value) { .type = LI_NONE };
        pthread_mutex_unlock(&lock);
        return c;
}

bool
li_var_has(const char *name)
{
        pthread_mutex_lock(&lock);
        Var *v   = var_find(name, false);
        bool has = v && v->set;
        pthread_mutex_unlock(&lock);
        return has;
}

void
li_var_declare(const char *name, Li_Type type, Li_Value start)
{
        pthread_mutex_lock(&lock);
        Var *v = var_find(name, true);
        if (!(v->set && v->declared && v->type == type && value_equal(v->start, start))) {
                value_drop(v->start);
                v->start    = value_dup(start);
                v->type     = type;
                v->declared = true;
                li_var_set(name, start);
        }
        pthread_mutex_unlock(&lock);
}

Li_Type
li_var_type(const char *name)
{
        pthread_mutex_lock(&lock);
        Var *v    = var_find(name, false);
        Li_Type t = v && v->declared ? v->type : LI_ANY;
        pthread_mutex_unlock(&lock);
        return t;
}

void
var_list_clear(VarList *l)
{
        Da_foreach(v, *l)
        {
                free(v->name);
                value_drop(v->value);
                value_drop(v->start);
        }
        Da_destroy(l);
}

/* The variables of the canvas running, and the globals of the tab */
void
vars_clear(void)
{
        var_list_clear(&vars);
        var_list_clear(&globals);
}

/* ---- Lists ---- */

static List *
list_of(Li_Value v)
{
        return v.type == LI_LIST ? v.p : NULL;
}

/* *V as a list: an empty one if it isn't */
static List *
list_make(Li_Value *v)
{
        if (v->type == LI_LIST && v->p) return v->p;
        value_drop(*v);
        *v = (Li_Value) { .type = LI_LIST, .p = calloc(1, sizeof(List)) };
        return v->p;
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
li_list_len(Li_Value list)
{
        List *l = list_of(list);
        return l ? l->items.count : 0;
}

Li_Value
li_list_get(Li_Value list, long index)
{
        List *l = list_of(list);
        long i  = list_index(l, index);
        return i < 0 ? (Li_Value) { .type = LI_NONE } : l->items.items[i];
}

void
li_list_push(Li_Value *list, Li_Value value)
{
        /* First: VALUE may be in *LIST, and making it one would free it */
        Li_Value c = value_dup(value);
        List *l    = list_make(list);
        Da_append(&l->items, c);
}

Li_Value
li_list_pop(Li_Value *list)
{
        List *l = list_of(*list);
        if (!l || !l->items.count) return (Li_Value) { .type = LI_NONE };
        return l->items.items[--l->items.count];
}

bool
li_list_set(Li_Value *list, long index, Li_Value value)
{
        List *l = list_of(*list);
        long i  = list_index(l, index);
        if (i < 0) return false;
        Li_Value old      = l->items.items[i];
        l->items.items[i] = value_dup(value);
        value_drop(old);
        return true;
}

bool
li_list_remove(Li_Value *list, long index)
{
        List *l = list_of(*list);
        long i  = list_index(l, index);
        if (i < 0) return false;
        value_drop(l->items.items[i]);
        Da_remove(&l->items, i);
        return true;
}

void
li_list_clear(Li_Value *list)
{
        List *l = list_make(list);
        Da_foreach(it, l->items)
        {
                value_drop(*it);
        }
        l->items.count = 0;
}

/* ---- Maps ---- */

static Map *
map_of(Li_Value v)
{
        return v.type == LI_MAP ? v.p : NULL;
}

static Map *
map_make(Li_Value *v)
{
        if (v->type == LI_MAP && v->p) return v->p;
        value_drop(*v);
        *v = (Li_Value) { .type = LI_MAP, .p = calloc(1, sizeof(Map)) };
        return v->p;
}

static int
entry_cmp(const void *key, const void *e)
{
        return strcmp(key, ((const Entry *) e)->key);
}

/* The entry of KEY, NULL if it has none */
static Entry *
map_find(Map *m, const char *key)
{
        if (!m || !m->items.count) return NULL;
        return bsearch(key ? key : "", m->items.items, m->items.count, sizeof(Entry), entry_cmp);
}

long
li_map_len(Li_Value map)
{
        Map *m = map_of(map);
        return m ? m->items.count : 0;
}

Li_Value
li_map_get(Li_Value map, const char *key)
{
        Entry *e = map_find(map_of(map), key);
        return e ? e->value : (Li_Value) { .type = LI_NONE };
}

bool
li_map_has(Li_Value map, const char *key)
{
        return map_find(map_of(map), key) != NULL;
}

const char *
li_map_key(Li_Value map, long index)
{
        Map *m = map_of(map);
        return m && index >= 0 && index < m->items.count ? m->items.items[index].key : NULL;
}

void
li_map_set(Li_Value *map, const char *key, Li_Value value)
{
        /* What *MAP was is freed last: VALUE may be in it */
        Li_Value was = { .type = LI_NONE };
        if (!map_of(*map)) {
                was  = *map;
                *map = (Li_Value) { .type = LI_MAP, .p = calloc(1, sizeof(Map)) };
        }
        Map *m = map->p;
        if (!key) key = "";
        Entry *e = map_find(m, key);
        if (e) {
                value_drop(e->value);
                e->value = value_dup(value);
                value_drop(was);
                return;
        }
        /* In its place: after the keys before it */
        int at = 0;
        while (at < m->items.count && strcmp(m->items.items[at].key, key) < 0)
                at++;
        Da_append(&m->items, ((Entry) { 0 }));
        memmove(&m->items.items[at + 1], &m->items.items[at], (m->items.count - 1 - at) * sizeof(Entry));
        m->items.items[at] = (Entry) { strdup(key), value_dup(value) };
        value_drop(was);
}

void
li_map_del(Li_Value *map, const char *key)
{
        Map *m   = map_of(*map);
        Entry *e = map_find(m, key);
        if (!e) return;
        free(e->key);
        value_drop(e->value);
        Da_remove(&m->items, e - m->items.items);
}

void
li_map_clear(Li_Value *map)
{
        if (map->type == LI_MAP) li_drop(map);
        map_make(map);
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
        case LI_LIST: return list_format(v, buf, size);
        case LI_MAP: return map_format(v, buf, size);
        case LI_REF: return snprintf(buf, size, "%s", v.s ? v.s : "");
        case LI_BYTES: return snprintf(buf, size, "%zu bytes", li_bytes_size(v));
        default:
                if (type_is_record(v.type)) return v.p ? record_format(v.p, buf, size) : snprintf(buf, size, "-");
                return snprintf(buf, size, "-");
        }
}

/* ---- Bytes ---- */

typedef struct Bytes {
        size_t size;
        unsigned char data[];
} Bytes;

Li_Value
li_bytes(const void *data, size_t size)
{
        Bytes *b = malloc(sizeof *b + size + 1);
        b->size  = size;
        if (data)
                memcpy(b->data, data, size);
        else
                memset(b->data, 0, size);
        b->data[size] = '\0'; /* read as a string, it ends */
        return (Li_Value) { .type = LI_BYTES, .p = b };
}

size_t
li_bytes_size(Li_Value v)
{
        return v.type == LI_BYTES && v.p ? ((Bytes *) v.p)->size : 0;
}

unsigned char *
li_bytes_data(Li_Value v)
{
        return v.type == LI_BYTES && v.p ? ((Bytes *) v.p)->data : NULL;
}

/* ---- Records as C structs ---- */

/* What a field of type NAME is in C: KIND ('i' signed, 'u' unsigned, 'f'
 * floating point, 'b' bool, 's' string, 'p' pointer, 'r' a record by value,
 * REC), its size and alignment. False if it has no C form (any, a list). */
static bool c_layout(Li_Type t, size_t *size, size_t *align, int depth);

static bool
c_field(const char *name, char *kind, size_t *size, size_t *align, Li_Type *rec, int depth)
{
        static const struct {
                const char *name;
                char kind;
                size_t size;
        } prims[] = {
                { "int", 'i', sizeof(long) },
                { "float", 'f', sizeof(double) },
                { "bool", 'b', sizeof(bool) },
                { "char", 'i', 1 },
                { "int8", 'i', 1 },
                { "uint8", 'u', 1 },
                { "int16", 'i', 2 },
                { "uint16", 'u', 2 },
                { "int32", 'i', 4 },
                { "uint32", 'u', 4 },
                { "int64", 'i', 8 },
                { "uint64", 'u', 8 },
                { "float32", 'f', 4 },
                { "float64", 'f', 8 },
                { "string", 's', sizeof(char *) },
                { "ptr", 'p', sizeof(void *) },
        };
        for (size_t i = 0; i < sizeof prims / sizeof *prims; i++) {
                if (strcmp(prims[i].name, name ? name : "")) continue;
                *kind = prims[i].kind;
                *size = *align = prims[i].size;
                *rec           = LI_NONE;
                return true;
        }
        *rec  = type_named(name ? name : "");
        *kind = 'r';
        return *rec != LI_NONE && c_layout(*rec, size, align, depth + 1);
}

/* Offset of each field of T as C lays it out: in order, each aligned */
static bool
c_layout(Li_Type t, size_t *size, size_t *align, int depth)
{
        *size  = 0;
        *align = 1;
        if (!type_is_record(t) || depth > 16) return false;
        for (int i = 0; i < type_field_count(t); i++) {
                char k;
                size_t fs, fa;
                Li_Type r;
                if (!c_field(type_field_type(t, i), &k, &fs, &fa, &r, depth)) return false;
                *size = (*size + fa - 1) / fa * fa + fs;
                if (fa > *align) *align = fa;
        }
        *size = (*size + *align - 1) / *align * *align;
        return *size > 0;
}

size_t
li_c_size(Li_Type t)
{
        size_t size, align;
        return c_layout(t, &size, &align, 0) ? size : 0;
}

size_t
li_c_align(Li_Type t)
{
        size_t size, align;
        return c_layout(t, &size, &align, 0) ? align : 0;
}

/* Field I of T: its kind, size and offset (and REC for a record) */
static bool
c_at(Li_Type t, int i, char *kind, size_t *size, size_t *offset, Li_Type *rec)
{
        size_t off = 0, fs = 0, fa;
        for (int k = 0; k <= i; k++) {
                if (!c_field(type_field_type(t, k), kind, &fs, &fa, rec, 0)) return false;
                off = (off + fa - 1) / fa * fa;
                if (k < i) off += fs;
        }
        *size   = fs;
        *offset = off;
        return true;
}

static long long
as_whole(Li_Value v)
{
        return v.type == LI_INT ? v.i : v.type == LI_FLOAT ? (long long) v.f :
                                        v.type == LI_BOOL  ? v.b :
                                                             0;
}

bool
li_c_write(Li_Type t, Li_Value v, void *mem)
{
        Record *r = v.type == t ? v.p : NULL;
        if (!r || !li_c_size(t)) return false;
        for (int i = 0; i < r->count; i++) {
                char kind;
                size_t size, off;
                Li_Type rec;
                Li_Value f = r->fields[i];
                char *p    = (char *) mem + 0;
                if (!c_at(t, i, &kind, &size, &off, &rec)) return false;
                p += off;
                switch (kind) {
                case 'i':
                case 'u': {
                        long long x = as_whole(f);
                        if (size == 1) *(int8_t *) p = (int8_t) x;
                        if (size == 2) *(int16_t *) p = (int16_t) x;
                        if (size == 4) *(int32_t *) p = (int32_t) x;
                        if (size == 8) *(int64_t *) p = (int64_t) x;
                        break;
                }
                case 'f': {
                        double d = f.type == LI_FLOAT ? f.f : (double) as_whole(f);
                        if (size == 4)
                                *(float *) p = (float) d;
                        else
                                *(double *) p = d;
                        break;
                }
                case 'b': *(bool *) p = as_whole(f) != 0; break;
                case 's': *(const char **) p = f.type == LI_STRING ? f.s : NULL; break;
                case 'p':
                        *(void **) p = NULL;
                        if (f.type == LI_PTR) *(void **) p = f.p;
                        if (f.type == LI_BYTES) *(void **) p = li_bytes_data(f);
                        break;
                case 'r':
                        if (!li_c_write(rec, f, p)) memset(p, 0, size);
                        break;
                }
        }
        return true;
}

Li_Value
li_c_read(Li_Type t, const void *mem)
{
        int n = type_field_count(t);
        if (!li_c_size(t) || n <= 0) return (Li_Value) { .type = LI_NONE };
        Record *r = malloc(sizeof *r + n * sizeof *r->fields);
        r->type   = t;
        r->count  = n;
        for (int i = 0; i < n; i++) {
                char kind;
                size_t size, off;
                Li_Type rec;
                const char *p = (const char *) mem;
                c_at(t, i, &kind, &size, &off, &rec);
                p += off;
                Li_Value f = { .type = LI_NONE };
                switch (kind) {
                case 'i':
                        f = li_int(size == 1 ? *(const int8_t *) p : size == 2 ? *(const int16_t *) p :
                                                                     size == 4 ? *(const int32_t *) p :
                                                                                 (long) *(const int64_t *) p);
                        break;
                case 'u':
                        f = li_int(size == 1 ? *(const uint8_t *) p : size == 2 ? *(const uint16_t *) p :
                                                                      size == 4 ? (long) *(const uint32_t *) p :
                                                                                  (long) *(const uint64_t *) p);
                        break;
                case 'f': f = li_float(size == 4 ? *(const float *) p : *(const double *) p); break;
                case 'b': f = li_bool(*(const bool *) p); break;
                case 's': {
                        const char *str = *(const char *const *) p;
                        f               = str ? li_string(strdup(str)) : (Li_Value) { .type = LI_NONE };
                        break;
                }
                case 'p': f = li_ptr(*(void *const *) p); break;
                case 'r': f = li_c_read(rec, p); break;
                }
                r->fields[i] = f;
        }
        return (Li_Value) { .type = t, .p = r };
}

static int
c_members(Li_Type t, size_t base, Li_C_Member *out, int max, int n)
{
        for (int i = 0; i < type_field_count(t); i++) {
                char kind;
                size_t size, off;
                Li_Type rec;
                if (!c_at(t, i, &kind, &size, &off, &rec)) return -1;
                if (kind == 'r') {
                        n = c_members(rec, base + off, out, max, n);
                        if (n < 0) return -1;
                        continue;
                }
                if (n >= max) return -1;
                out[n++] = (Li_C_Member) { (int) (base + off), (int) size,
                                           kind == 'b' ? 'u' : kind == 's' ? 'p' :
                                                                             kind };
        }
        return n;
}

int
li_c_members(Li_Type t, Li_C_Member *out, int max)
{
        return li_c_size(t) ? c_members(t, 0, out, max, 0) : -1;
}

Li_Type
li_type_named(const char *name)
{
        return type_named(name);
}

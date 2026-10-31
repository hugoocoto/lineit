/* value.c - see value.h */
#define _POSIX_C_SOURCE 200809L /* strdup */
#define BT_IMPLEMENTATION
#include "value.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* A copy of V that owns what it points to (strings, records), for whoever
 * keeps it longer than the module that made it: free it with value_drop */
Li_Value
value_dup(Li_Value v)
{
        if (v.type == LI_STRING) return li_string(strdup(v.s ? v.s : ""));
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
        if (v.type == LI_STRING) free((char *) v.s);
        if (type_is_record(v.type) && v.p) {
                Record *r = v.p;
                for (int i = 0; i < r->count; i++)
                        value_drop(r->fields[i]);
                free(r);
        }
}

bool
value_equal(Li_Value a, Li_Value b)
{
        if (a.type != b.type) return false;
        switch (a.type) {
        case LI_BOOL: return a.b == b.b;
        case LI_INT: return a.i == b.i;
        case LI_FLOAT: return a.f == b.f;
        case LI_STRING: return !strcmp(a.s ? a.s : "", b.s ? b.s : "");
        case LI_PTR: return a.p == b.p;
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

/* Variables, by name. Modules can't share globals (each square has its own
 * copy), so they live here. Like in C, they are local to the canvas they
 * are in: a subgraph square has its own (see Subgraphs), the canvas of a
 * tab its own. Globals (li_global_*) are shared by all of them in the tab. */
VarList vars;    /* of the canvas running: the tab's, or a subgraph's */
VarList globals; /* of the tab */

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

        /* Copied: the module's strings and records may not live long */
        Li_Value old = v->value;
        v->value     = value_dup(value);
        value_drop(old);
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

void
var_list_clear(VarList *l)
{
        Da_foreach(v, *l)
        {
                free(v->name);
                value_drop(v->value);
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

/* ---- Lists (Da) and trees (bt.h), shared by every square, by name ---- */

/* A value that owns its string */
typedef struct Stored {
        Li_Value value; /* a copy it owns (value_dup) */
} Stored;

static void
stored_set(Stored *st, Li_Value value)
{
        Li_Value old = st->value;
        st->value    = value_dup(value);
        value_drop(old);
}

static void
stored_free(Stored *st)
{
        value_drop(st->value);
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
BT *lists; /* name -> List * */
BT *trees; /* name -> Tree * */

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

void
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
                it->value = NULL; /* bt_destroy reads it */
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
                        e->value = NULL;
                }
                bt_destroy(&t->map);
                free(t);
                it->value = NULL;
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
        default:
                if (type_is_record(v.type)) return v.p ? record_format(v.p, buf, size) : snprintf(buf, size, "-");
                return snprintf(buf, size, "-");
        }
}

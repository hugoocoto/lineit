/* Shared by the map modules: the map is the variable of their "map" input,
 * wired from a var/var (of type map, or any) or its name typed in it, and
 * they show it with its size.
 * Keys are strings, other values are turned into one (42 -> "42"). */
#include "lineit.h"

/* The variable wired to input IN if it can hold a map, NULL if not */
static inline const char *
map_var(Li_Node *n, int in)
{
        const char *name = li_ref(n, in);
        Li_Type t        = name ? li_var_type(name) : LI_NONE;
        return t == LI_MAP || t == LI_ANY ? name : NULL;
}

/* KEY input as a string, in BUF */
static inline const char *
key(Li_Node *n, int input, char *buf, int size)
{
        Li_Value v = li_get(n, input);
        if (v.type == LI_STRING) return v.s;
        li_format(v, buf, size);
        return buf;
}

/* What they do to the map of a variable, each one with li_var_update:
 * whole, even with branches in parallel */
typedef struct MapOp {
        const char *key;
        Li_Value value; /* what goes in, or what comes out (a copy, yours) */
        long len;
        bool has;
} MapOp;

static inline void
op_read(Li_Value *m, void *ctx)
{
        MapOp *op = ctx;
        op->len   = li_map_len(*m);
        if (!op->key) return;
        op->has   = li_map_has(*m, op->key);
        op->value = li_copy(li_map_get(*m, op->key));
}

/* Its size, and what KEY has (a copy: li_drop it) */
static inline MapOp
map_read(const char *name, const char *key)
{
        MapOp op = { .key = key };
        li_var_update(name, op_read, &op);
        return op;
}

/* "m {3}", or what's wrong */
static inline void
map_show(Li_Node *n, int in)
{
        const char *name = li_ref(n, in);
        if (!name)
                li_text(n, "wire or type a map");
        else if (!map_var(n, in))
                li_text(n, "%s is not a map", name);
        else
                li_text(n, "%s {%ld}", name, map_read(name, NULL).len);
}

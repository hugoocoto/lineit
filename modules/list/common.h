/* Shared by the list modules: the list is the variable of their "list"
 * input, wired from a var/var (of type list, or any) or its name typed in
 * it, and they show it with its length. */
#include "lineit.h"

/* The variable wired to input IN if it can hold a list, NULL if not */
static inline const char *
list_var(Li_Node *n, int in)
{
        const char *name = li_ref(n, in);
        Li_Type t        = name ? li_var_type(name) : LI_NONE;
        return t == LI_LIST || t == LI_ANY ? name : NULL;
}

/* An index from any number, 0 if it isn't one (or nothing is typed) */
static inline long
as_index(Li_Value v)
{
        return v.type == LI_INT ? v.i : v.type == LI_FLOAT ? (long) v.f :
                                        v.type == LI_BOOL  ? v.b :
                                                             0;
}

/* What they do to the list of a variable, each one with li_var_update:
 * whole, even with branches in parallel */
typedef struct ListOp {
        long index;
        Li_Value value; /* what goes in, or what comes out (a copy, yours) */
        long len;
        bool ok;
} ListOp;

static inline void
op_len(Li_Value *l, void *ctx)
{
        ((ListOp *) ctx)->len = li_list_len(*l);
}

static inline void
op_get(Li_Value *l, void *ctx)
{
        ListOp *op = ctx;
        op->value  = li_copy(li_list_get(*l, op->index));
}

static inline long
list_len(const char *name)
{
        ListOp op = { 0 };
        li_var_update(name, op_len, &op);
        return op.len;
}

/* "l [3]", or what's wrong */
static inline void
list_show(Li_Node *n, int in)
{
        const char *name = li_ref(n, in);
        if (!name)
                li_text(n, "wire or type a list");
        else if (!list_var(n, in))
                li_text(n, "%s is not a list", name);
        else
                li_text(n, "%s [%ld]", name, list_len(name));
}

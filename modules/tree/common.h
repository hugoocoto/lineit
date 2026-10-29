/* Shared by the tree modules: the tree name is their first param, and they
 * show it with its size. Keys are strings, other values are turned into one
 * (42 -> "42"). */
#include "lineit.h"

#define TREE(n) (li_get_param((n), 0).s)

static inline void
tree_param(Li_Node *n)
{
        li_param(n, "tree", li_string("t"));
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

void
module_draw(Li_Node *n)
{
        li_text(n, "%s {%ld}", TREE(n), li_tree_len(TREE(n)));
}

/* Number of keys. Pure: read when needed. */
#include "common.h"

void
module_init(Li_Node *n)
{
        li_output(n, "len", LI_INT);
        tree_param(n);
}

void
module_eval(Li_Node *n)
{
        li_set(n, 0, li_int(li_tree_len(TREE(n))));
}

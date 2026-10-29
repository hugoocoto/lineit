/* Whether "key" is in the tree. Pure: read when needed. */
#include "common.h"

void
module_init(Li_Node *n)
{
        li_input_value(n, "key", LI_ANY, li_string(""));
        li_output(n, "has", LI_BOOL);
        tree_param(n);
}

void
module_eval(Li_Node *n)
{
        char buf[64];
        li_set(n, 0, li_bool(li_tree_has(TREE(n), key(n, 0, buf, sizeof buf))));
}

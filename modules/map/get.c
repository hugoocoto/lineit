/* The value of "key", none if it's not there. Pure: read when needed. */
#include "common.h"

void
module_init(Li_Node *n)
{
        li_input_value(n, "key", LI_ANY, li_string(""));
        li_output(n, "value", LI_ANY);
        map_param(n);
}

void
module_eval(Li_Node *n)
{
        char buf[64];
        li_set(n, 0, li_tree_get(MAP(n), key(n, 0, buf, sizeof buf)));
}

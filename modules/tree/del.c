/* On tick, removes "key" */
#include "common.h"

void
module_init(Li_Node *n)
{
        li_input(n, "tick", LI_TICK);
        li_input_value(n, "key", LI_ANY, li_string(""));
        li_output(n, "then", LI_TICK);
        tree_param(n);
}

void
module_tick(Li_Node *n, int input)
{
        (void) input;
        char buf[64];
        li_tree_del(TREE(n), key(n, 1, buf, sizeof buf));
        li_tick(n, 0);
}

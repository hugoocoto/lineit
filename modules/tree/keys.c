/* On tick, fills the list "list" with the keys, in order. Loop over them
 * with loop + list/get. */
#include "common.h"

void
module_init(Li_Node *n)
{
        li_input(n, "tick", LI_TICK);
        li_output(n, "then", LI_TICK);
        tree_param(n);
        li_param(n, "list", li_string("keys"));
}

void
module_tick(Li_Node *n, int input)
{
        (void) input;
        li_tree_keys(TREE(n), li_get_param(n, 1).s);
        li_tick(n, 0);
}

/* On tick, removes the item at "index", moving the next ones back */
#include "common.h"

void
module_init(Li_Node *n)
{
        li_input(n, "tick", LI_TICK);
        li_input_value(n, "index", LI_ANY, li_int(0));
        li_output(n, "then", LI_TICK);
        list_param(n);
}

void
module_tick(Li_Node *n, int input)
{
        (void) input;
        li_list_remove(LIST(n), as_index(li_get(n, 1)));
        li_tick(n, 0);
}

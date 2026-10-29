/* On tick, removes every item */
#include "common.h"

void
module_init(Li_Node *n)
{
        li_input(n, "tick", LI_TICK);
        li_output(n, "then", LI_TICK);
        list_param(n);
}

void
module_tick(Li_Node *n, int input)
{
        (void) input;
        li_list_clear(LIST(n));
        li_tick(n, 0);
}

/* On tick, removes the last item and gives it in "value" (none if empty) */
#include "common.h"

void
module_init(Li_Node *n)
{
        li_input(n, "tick", LI_TICK);
        li_output(n, "then", LI_TICK);
        li_output(n, "value", LI_ANY);
        list_param(n);
}

void
module_tick(Li_Node *n, int input)
{
        (void) input;
        li_set(n, 1, li_list_pop(LIST(n)));
        li_tick(n, 0);
}

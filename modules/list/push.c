/* On tick, appends "value" to the list */
#include "common.h"

void
module_init(Li_Node *n)
{
        li_input(n, "tick", LI_TICK);
        li_input_value(n, "value", LI_ANY, li_int(0));
        li_output(n, "then", LI_TICK);
        list_param(n);
}

void
module_tick(Li_Node *n, int input)
{
        (void) input;
        li_list_push(LIST(n), li_get(n, 1));
        li_tick(n, 0);
}

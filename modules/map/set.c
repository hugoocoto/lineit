/* On tick, sets "key" to "value", adding it if it's new */
#include "common.h"

void
module_init(Li_Node *n)
{
        li_input(n, "tick", LI_TICK);
        li_input_value(n, "key", LI_ANY, li_string(""));
        li_input_value(n, "value", LI_ANY, li_int(0));
        li_output(n, "then", LI_TICK);
        map_param(n);
}

void
module_tick(Li_Node *n, int input)
{
        (void) input;
        char buf[64];
        li_tree_set(MAP(n), key(n, 1, buf, sizeof buf), li_get(n, 2));
        li_tick(n, 0);
}

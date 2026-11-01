/* On tick, removes every item */
#include "common.h"

enum { IN_TICK,
       IN_LIST };

void
module_init(Li_Node *n)
{
        li_input(n, "tick", LI_TICK);
        li_input_value(n, "list", LI_REF, li_var_ref(""));
        li_output(n, "then", LI_TICK);
}

static void
clear(Li_Value *l, void *ctx)
{
        (void) ctx;
        li_list_clear(l);
}

void
module_tick(Li_Node *n, int input)
{
        (void) input;
        const char *list = list_var(n, IN_LIST);
        if (list) li_var_update(list, clear, NULL);
        li_tick(n, 0);
}

void
module_draw(Li_Node *n)
{
        list_show(n, IN_LIST);
}

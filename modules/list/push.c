/* On tick, appends "value" to the list */
#include "common.h"

enum { IN_TICK,
       IN_LIST,
       IN_VALUE };

void
module_init(Li_Node *n)
{
        li_input(n, "tick", LI_TICK);
        li_input_value(n, "list", LI_REF, li_var_ref(""));
        li_input_value(n, "value", LI_ANY, li_int(0));
        li_output(n, "then", LI_TICK);
}

static void
push(Li_Value *l, void *ctx)
{
        li_list_push(l, ((ListOp *) ctx)->value);
}

void
module_tick(Li_Node *n, int input)
{
        (void) input;
        const char *list = list_var(n, IN_LIST);
        Li_Value v       = li_get(n, IN_VALUE);
        ListOp op        = { .value = v };
        if (list) li_var_update(list, push, &op);
        li_tick(n, 0);
}

void
module_draw(Li_Node *n)
{
        list_show(n, IN_LIST);
}

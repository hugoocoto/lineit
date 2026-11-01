/* On tick, removes the last item and gives it in "value" (none if empty) */
#include "common.h"

enum { IN_TICK,
       IN_LIST };

void
module_init(Li_Node *n)
{
        li_input(n, "tick", LI_TICK);
        li_input_value(n, "list", LI_REF, li_var_ref(""));
        li_output(n, "then", LI_TICK);
        li_output(n, "value", LI_ANY);
}

static void
pop(Li_Value *l, void *ctx)
{
        ((ListOp *) ctx)->value = li_list_pop(l);
}

void
module_tick(Li_Node *n, int input)
{
        (void) input;
        const char *list = list_var(n, IN_LIST);
        ListOp op        = { 0 };
        if (list) li_var_update(list, pop, &op);
        li_set(n, 1, op.value);
        li_drop(&op.value);
        li_tick(n, 0);
}

void
module_draw(Li_Node *n)
{
        list_show(n, IN_LIST);
}

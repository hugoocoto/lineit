/* On tick, removes the item at "index", moving the next ones back */
#include "common.h"

enum { IN_TICK,
       IN_LIST,
       IN_INDEX };

void
module_init(Li_Node *n)
{
        li_input(n, "tick", LI_TICK);
        li_input_value(n, "list", LI_REF, li_var_ref(""));
        li_input_value(n, "index", LI_ANY, li_int(0));
        li_output(n, "then", LI_TICK);
}

static void
del(Li_Value *l, void *ctx)
{
        li_list_remove(l, ((ListOp *) ctx)->index);
}

void
module_tick(Li_Node *n, int input)
{
        (void) input;
        const char *list = list_var(n, IN_LIST);
        long index       = as_index(li_get(n, IN_INDEX));
        ListOp op        = { .index = index };
        if (list) li_var_update(list, del, &op);
        li_tick(n, 0);
}

void
module_draw(Li_Node *n)
{
        list_show(n, IN_LIST);
}

/* The item at "index" (negative counts from the end), none if out of range.
 * Pure: read when needed, no ticks. */
#include "common.h"

enum { IN_LIST,
       IN_INDEX };

void
module_init(Li_Node *n)
{
        li_input_value(n, "list", LI_REF, li_var_ref(""));
        li_input_value(n, "index", LI_ANY, li_int(0));
        li_output(n, "value", LI_ANY);
}

void
module_eval(Li_Node *n)
{
        const char *list = list_var(n, IN_LIST);
        ListOp op        = { .index = as_index(li_get(n, IN_INDEX)) };
        if (list) li_var_update(list, op_get, &op);
        li_set(n, 0, op.value);
        li_drop(&op.value);
}

void
module_draw(Li_Node *n)
{
        list_show(n, IN_LIST);
}

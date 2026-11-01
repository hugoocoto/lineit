/* Number of items. Pure: read when needed, no ticks. */
#include "common.h"

void
module_init(Li_Node *n)
{
        li_input_value(n, "list", LI_REF, li_var_ref(""));
        li_output(n, "len", LI_INT);
}

void
module_eval(Li_Node *n)
{
        const char *list = list_var(n, 0);
        li_set(n, 0, li_int(list ? list_len(list) : 0));
}

void
module_draw(Li_Node *n)
{
        list_show(n, 0);
}

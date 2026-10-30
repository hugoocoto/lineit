/* Number of items. Pure: read when needed, no ticks. */
#include "common.h"

void
module_init(Li_Node *n)
{
        li_output(n, "len", LI_INT);
        list_param(n);
}

void
module_eval(Li_Node *n)
{
        li_set(n, 0, li_int(li_list_len(LIST(n))));
}

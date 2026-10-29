/* The item at "index" (negative counts from the end), none if out of range.
 * Pure: read when needed, no ticks. */
#include "common.h"

void
module_init(Li_Node *n)
{
        li_input_value(n, "index", LI_ANY, li_int(0));
        li_output(n, "value", LI_ANY);
        list_param(n);
}

void
module_eval(Li_Node *n)
{
        li_set(n, 0, li_list_get(LIST(n), as_index(li_get(n, 0))));
}

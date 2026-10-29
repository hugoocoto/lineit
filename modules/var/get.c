/* The value of the variable "name" (set from the editor), stored by
 * var/set. Pure: read when needed, no ticks. */
#include "lineit.h"

enum { PARAM_NAME };

void
module_init(Li_Node *n)
{
        li_output(n, "value", LI_ANY);
        li_param(n, "name", li_string("x"));
}

void
module_eval(Li_Node *n)
{
        li_set(n, 0, li_var_get(li_get_param(n, PARAM_NAME).s));
}

void
module_draw(Li_Node *n)
{
        char buf[64];
        const char *name = li_get_param(n, PARAM_NAME).s;
        li_format(li_var_get(name), buf, sizeof buf);
        li_text(n, "%s = %s", name, buf);
}

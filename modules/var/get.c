/* The value of the variable "name" (set from the editor), stored by
 * var/set. Local to the canvas, or global with the param "global", see
 * var.h. Pure: read when needed, no ticks. */
#include "var.h"

void
module_init(Li_Node *n)
{
        li_output(n, "value", LI_ANY);
        var_params(n);
}

void
module_eval(Li_Node *n)
{
        li_set(n, 0, var_get(n));
}

void
module_draw(Li_Node *n)
{
        var_show(n);
}

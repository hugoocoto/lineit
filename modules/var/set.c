/* On tick, stores "value" in the variable "name" (set from the editor), then
 * ticks "then". Local to the canvas, or global with the param "global", see
 * var.h. Read it with var/get. */
#include "var.h"

enum { IN_TICK,
       IN_VALUE };
enum { OUT_THEN,
       OUT_VALUE };

void
module_init(Li_Node *n)
{
        li_input(n, "tick", LI_TICK);
        li_input_value(n, "value", LI_ANY, li_int(0));
        li_output(n, "then", LI_TICK);
        li_output(n, "value", LI_ANY);
        var_params(n);
}

void
module_tick(Li_Node *n, int input)
{
        (void) input;
        var_set(n, li_get(n, IN_VALUE));
        /* The stored copy: it lives longer than the input's */
        li_set(n, OUT_VALUE, var_get(n));
        li_tick(n, OUT_THEN);
}

void
module_draw(Li_Node *n)
{
        var_show(n);
}

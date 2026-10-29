/* On tick, stores "value" in the variable "name" (set from the editor), then
 * ticks "then". Read it anywhere with var/get. */
#include "lineit.h"

enum { IN_TICK,
       IN_VALUE };
enum { OUT_THEN,
       OUT_VALUE };
enum { PARAM_NAME };

static void
show(Li_Node *n)
{
        char buf[64];
        const char *name = li_get_param(n, PARAM_NAME).s;
        li_format(li_var_get(name), buf, sizeof buf);
        li_text(n, "%s = %s", name, buf);
}

void
module_init(Li_Node *n)
{
        li_input(n, "tick", LI_TICK);
        li_input_value(n, "value", LI_ANY, li_int(0));
        li_output(n, "then", LI_TICK);
        li_output(n, "value", LI_ANY);
        li_param(n, "name", li_string("x"));
}

void
module_tick(Li_Node *n, int input)
{
        (void) input;
        const char *name = li_get_param(n, PARAM_NAME).s;

        li_var_set(name, li_get(n, IN_VALUE));
        /* The stored copy: it lives longer than the input's */
        li_set(n, OUT_VALUE, li_var_get(name));
        li_tick(n, OUT_THEN);
}

void
module_draw(Li_Node *n)
{
        show(n);
}

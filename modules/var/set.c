/* On tick, stores "value" in the variable of "var" (wired from a var/var,
 * or its name), then ticks "then". "value" takes the type of the variable. */
#include "var.h"

enum { IN_TICK,
       IN_VAR,
       IN_VALUE };
enum { OUT_THEN,
       OUT_VALUE };

void
module_init(Li_Node *n)
{
        li_input(n, "tick", LI_TICK);
        li_input_value(n, "var", LI_REF, li_var_ref(""));
        li_input_value(n, "value", LI_ANY, li_int(0));
        li_output(n, "then", LI_TICK);
        li_output(n, "value", LI_ANY);
}

void
module_tick(Li_Node *n, int input)
{
        (void) input;
        const char *name = li_ref(n, IN_VAR);
        if (name) {
                Li_Value v = li_get(n, IN_VALUE);
                li_var_set(name, v);
                li_set(n, OUT_VALUE, v);
        }
        li_tick(n, OUT_THEN);
}

void
module_draw(Li_Node *n)
{
        const char *name = li_ref(n, IN_VAR);
        Li_Type t        = name ? li_var_type(name) : LI_ANY;
        li_input_type(n, IN_VALUE, t);
        li_output_type(n, OUT_VALUE, t);
        var_show(n, name);
}

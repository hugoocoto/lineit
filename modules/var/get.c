/* The value of the variable of "var" (wired from a var/var, or its name).
 * Its output takes the type of the variable. Pure: read when needed, no ticks. */
#include "var.h"

void
module_init(Li_Node *n)
{
        li_input_value(n, "var", LI_REF, li_var_ref(""));
        li_output(n, "value", LI_ANY);
}

void
module_eval(Li_Node *n)
{
        const char *name = li_ref(n, 0);
        li_output_type(n, 0, name ? li_var_type(name) : LI_ANY);
        Li_Value v = name ? li_var_get(name) : (Li_Value) { .type = LI_NONE };
        li_set(n, 0, v);
        li_drop(&v);
}

void
module_draw(Li_Node *n)
{
        module_eval(n);
        var_show(n, li_ref(n, 0));
}

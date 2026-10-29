/* Shared by the two input logic modules. Before including it, define (or
 * define LOGIC_NO_MODULE to get only as_bool()):
 *   OUTPUT          output label, "a & b"
 *   RESULT(a, b)    the result, from the two bools
 * Inputs are wired or ticked in the square. Anything but a bool counts as
 * false. Pure: computed when read, no ticks needed. */
#include "lineit.h"

static inline bool
as_bool(Li_Value v)
{
        return v.type == LI_BOOL && v.b;
}

#ifndef LOGIC_NO_MODULE

void
module_init(Li_Node *n)
{
        li_input_value(n, "a", LI_BOOL, li_bool(false));
        li_input_value(n, "b", LI_BOOL, li_bool(false));
        li_output(n, OUTPUT, LI_BOOL);
}

void
module_eval(Li_Node *n)
{
        bool a      = as_bool(li_get(n, 0));
        bool b      = as_bool(li_get(n, 1));
        bool result = RESULT(a, b);
        li_set(n, 0, li_bool(result));
        li_text(n, "%s", result ? "true" : "false");
}

/* Show it even if nothing reads it */
void
module_draw(Li_Node *n)
{
        module_eval(n);
}

#endif

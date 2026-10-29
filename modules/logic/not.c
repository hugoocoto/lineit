/* !a. a is wired or ticked in the square, anything but a bool counts as
 * false. Pure: computed when read, no ticks needed. */
#include "lineit.h"

void
module_init(Li_Node *n)
{
        li_input_value(n, "a", LI_BOOL, li_bool(false));
        li_output(n, "!a", LI_BOOL);
}

void
module_eval(Li_Node *n)
{
        Li_Value a  = li_get(n, 0);
        bool result = !(a.type == LI_BOOL && a.b);
        li_set(n, 0, li_bool(result));
        li_text(n, "%s", result ? "true" : "false");
}

/* Show it even if nothing reads it */
void
module_draw(Li_Node *n)
{
        module_eval(n);
}

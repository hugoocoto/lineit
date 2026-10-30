/* On tick, fires "true" or "false" depending on "cond": a wire, or the
 * checkbox in the square. */
#include "lineit.h"

enum { IN_TICK,
       IN_COND };
enum { OUT_TRUE,
       OUT_FALSE };

void
module_init(Li_Node *n)
{
        li_input(n, "tick", LI_TICK);
        li_input_value(n, "cond", LI_BOOL, li_bool(false));
        li_output(n, "true", LI_TICK);
        li_output(n, "false", LI_TICK);
}

void
module_tick(Li_Node *n, int input)
{
        (void) input;
        Li_Value cond = li_get(n, IN_COND);
        bool yes      = cond.type == LI_BOOL && cond.b;

        li_text(n, "%s", yes ? "true" : "false");
        li_tick(n, yes ? OUT_TRUE : OUT_FALSE);
}

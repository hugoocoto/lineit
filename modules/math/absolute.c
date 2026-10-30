/* |a|, int or float. Bools count as 0 or 1. Pure: computed when read, no
 * ticks needed. */
#include "lineit.h"
#include <limits.h>
#include <math.h>

void
module_init(Li_Node *n)
{
        li_input_value(n, "a", LI_ANY, li_int(0));
        li_output(n, "|a|", LI_ANY);
}

void
module_eval(Li_Node *n)
{
        Li_Value a = li_get(n, 0), v;
        char buf[64];
        switch (a.type) {
        case LI_NONE: v = li_int(0); break;
        case LI_BOOL: v = li_int(a.b); break;
        case LI_FLOAT: v = li_float(fabs(a.f)); break;
        case LI_INT:
                if (a.i != LONG_MIN) {
                        v = li_int(a.i < 0 ? -a.i : a.i);
                        break;
                }
                li_set(n, 0, (Li_Value) { .type = LI_NONE });
                li_text(n, "overflow");
                return;
        default:
                li_set(n, 0, (Li_Value) { .type = LI_NONE });
                li_text(n, "not a number");
                return;
        }
        li_set(n, 0, v);
        li_format(v, buf, sizeof buf);
        li_text(n, "%s", buf);
}

void
module_draw(Li_Node *n)
{
        module_eval(n);
}

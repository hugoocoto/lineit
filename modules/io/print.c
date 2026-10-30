/* Prints its value on each tick, in the output panel and stdout, and shows
 * it */
#include "lineit.h"

enum { IN_TICK,
       IN_VALUE };
enum { OUT_THEN };

void
module_init(Li_Node *n)
{
        li_input(n, "tick", LI_TICK);
        li_input_value(n, "value", LI_ANY, li_string(""));
        li_output(n, "then", LI_TICK);
}

void
module_tick(Li_Node *n, int input)
{
        (void) input;
        char buf[64];
        Li_Value v = li_get(n, IN_VALUE);

        li_format(v, buf, sizeof buf);
        li_print(n, "%s", buf);
        li_text(n, "%s", buf);
        li_tick(n, OUT_THEN);
}

/* for (index = from; index < to; index++) tick body; then tick done.
 * from and to are typed in the square (0 and 10) or wired.
 * A tick on "break" from inside the body stops it early. */
#include "lineit.h"

enum { IN_TICK,
       IN_FROM,
       IN_TO,
       IN_BREAK };
enum { OUT_BODY,
       OUT_INDEX,
       OUT_DONE };

static bool looping;
static bool stop;

static long
get_int(Li_Node *n, int input)
{
        Li_Value v = li_get(n, input);
        return v.type == LI_INT ? v.i : 0;
}

void
module_init(Li_Node *n)
{
        li_input(n, "tick", LI_TICK);
        li_input_value(n, "from", LI_INT, li_int(0));
        li_input_value(n, "to", LI_INT, li_int(10));
        li_input(n, "break", LI_TICK);
        li_output(n, "body", LI_TICK);
        li_output(n, "index", LI_INT);
        li_output(n, "done", LI_TICK);
        li_set(n, OUT_INDEX, li_int(0));
}

void
module_tick(Li_Node *n, int input)
{
        if (input == IN_BREAK) {
                stop = true;
                return;
        }
        /* The body ticking this same loop again would never end */
        if (looping) return;

        long from = get_int(n, IN_FROM);
        long to   = get_int(n, IN_TO);

        looping = true;
        stop    = false;
        for (long i = from; i < to && !stop; i++) {
                li_set(n, OUT_INDEX, li_int(i));
                li_text(n, "%ld", i);
                li_tick(n, OUT_BODY);
        }
        looping = false;
        li_tick(n, OUT_DONE);
}

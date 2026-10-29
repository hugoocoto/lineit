/* while (cond) tick body; then tick done. cond is read again before each
 * pass, so wire a comparison to it. A tick on "break" from inside the body
 * stops it, and so does reaching "max" passes (set from the editor), so a
 * cond that never turns false doesn't hang the program. */
#include "lineit.h"
#include <stdio.h>

enum { IN_TICK,
       IN_COND,
       IN_BREAK };
enum { OUT_BODY,
       OUT_DONE };
enum { PARAM_MAX };

static bool looping;
static bool stop;

static bool
cond(Li_Node *n)
{
        Li_Value v = li_get(n, IN_COND);
        return v.type == LI_BOOL && v.b;
}

void
module_init(Li_Node *n)
{
        li_input(n, "tick", LI_TICK);
        li_input_value(n, "cond", LI_BOOL, li_bool(false));
        li_input(n, "break", LI_TICK);
        li_output(n, "body", LI_TICK);
        li_output(n, "done", LI_TICK);
        li_param(n, "max", li_int(10000));
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

        long max   = li_get_param(n, PARAM_MAX).i;
        long count = 0;

        looping = true;
        stop    = false;
        while (!stop && count < max && cond(n)) {
                li_tick(n, OUT_BODY);
                count++;
        }
        looping = false;

        if (count >= max && cond(n)) {
                fprintf(stderr, "while: stopped after max = %ld passes\n", max);
                li_text(n, "max!");
        } else {
                li_text(n, "%ld", count);
        }
        li_tick(n, OUT_DONE);
}

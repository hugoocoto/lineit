/* Ticks every "seconds" (typed in the square or wired), counting the ticks.
 * A tick on "reset" sets the count back to 0 and restarts the timer. Each
 * square has its own copy of these globals. The count is saved with the
 * canvas. */
#include "lineit.h"
#include <stdio.h>
#include <stdlib.h>

enum { IN_RESET,
       IN_SECONDS };
enum { OUT_TICK,
       OUT_COUNT };

static double last;
static long count;

static void
show(Li_Node *n)
{
        li_set(n, OUT_COUNT, li_int(count));
        li_text(n, "%ld", count);
}

void
module_init(Li_Node *n)
{
        li_input(n, "reset", LI_TICK);
        li_input_value(n, "seconds", LI_ANY, li_float(1.0));
        li_output(n, "tick", LI_TICK);
        li_output(n, "count", LI_INT);
        show(n);
}

/* Any number, 1 if it isn't one */
static double
seconds(Li_Node *n)
{
        Li_Value v = li_get(n, IN_SECONDS);
        return v.type == LI_FLOAT ? v.f : v.type == LI_INT ? (double) v.i :
                                                             1.0;
}

void
module_tick(Li_Node *n, int input)
{
        (void) input;
        count = 0;
        last  = li_time();
        show(n);
}

void
module_frame(Li_Node *n)
{
        double now = li_time();

        if (now - last < seconds(n)) return;
        last = now;
        count++;
        show(n);
        li_tick(n, OUT_TICK);
}

const char *
module_save(Li_Node *n)
{
        (void) n;
        static char buf[32];
        snprintf(buf, sizeof buf, "%ld", count);
        return buf;
}

void
module_load(Li_Node *n, const char *state)
{
        count = strtol(state, NULL, 10);
        show(n);
}

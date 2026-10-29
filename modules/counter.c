/* Counts the ticks it gets, "reset" sets it back to 0. Each square has its
 * own copy of the globals. The count is saved with the canvas. */
#include "lineit.h"
#include <stdio.h>
#include <stdlib.h>

enum { IN_TICK,
       IN_RESET };
enum { OUT_THEN,
       OUT_COUNT };

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
        li_input(n, "tick", LI_TICK);
        li_input(n, "reset", LI_TICK);
        li_output(n, "then", LI_TICK);
        li_output(n, "count", LI_INT);
        show(n);
}

void
module_tick(Li_Node *n, int input)
{
        count = input == IN_RESET ? 0 : count + 1;
        show(n);
        li_tick(n, OUT_THEN);
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

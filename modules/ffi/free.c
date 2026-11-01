/* On tick, frees "ptr": what ffi/alloc gave, or a C function that says to
 * free its result with free(). Using it after is a crash, as in C. */
#include "lineit.h"
#include <stdlib.h>

void
module_init(Li_Node *n)
{
        li_input(n, "tick", LI_TICK);
        li_input(n, "ptr", LI_PTR);
        li_output(n, "then", LI_TICK);
}

void
module_tick(Li_Node *n, int input)
{
        (void) input;
        Li_Value p = li_get(n, 1);
        if (p.type == LI_PTR) free(p.p);
        li_tick(n, 0);
}

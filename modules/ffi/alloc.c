/* On tick, "size" bytes of memory, all zero, for C functions to use (a
 * buffer, a struct, an out-parameter): its ptr. Free it with ffi/free. */
#include "lineit.h"
#include <stdlib.h>

enum { IN_TICK,
       IN_SIZE };
enum { OUT_THEN,
       OUT_PTR };

/* The last one given: the canvas has it now, and frees it (ffi/free) */
static void *last;

void
module_init(Li_Node *n)
{
        li_input(n, "tick", LI_TICK);
        li_input_value(n, "size", LI_INT, li_int(64));
        li_output(n, "then", LI_TICK);
        li_output(n, "ptr", LI_PTR);
}

void
module_tick(Li_Node *n, int input)
{
        (void) input;
        Li_Value size = li_get(n, IN_SIZE);
        long bytes    = size.type == LI_INT && size.i > 0 ? size.i : 0;
        last          = bytes ? calloc(1, bytes) : NULL;
        li_set(n, OUT_PTR, li_ptr(last));
        li_tick(n, OUT_THEN);
}

void
module_draw(Li_Node *n)
{
        Li_Value size = li_get(n, IN_SIZE);
        li_text(n, "%ld bytes", size.type == LI_INT ? size.i : 0L);
}

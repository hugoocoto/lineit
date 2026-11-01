/* On tick, writes "value" at "ptr" + "offset" bytes, as "type": an argument
 * a C function reads through a pointer, a field of a struct. A string is
 * written with its '\0': the memory has to fit it. */
#include "mem.h"

enum { IN_TICK,
       IN_PTR,
       IN_OFFSET,
       IN_TYPE,
       IN_VALUE };

void
module_init(Li_Node *n)
{
        li_input(n, "tick", LI_TICK);
        li_input(n, "ptr", LI_PTR);
        li_input_value(n, "offset", LI_INT, li_int(0));
        li_input_options(n, "type", TYPES, T_INT);
        li_input_value(n, "value", LI_ANY, li_int(0));
        li_output(n, "then", LI_TICK);
}

void
module_tick(Li_Node *n, int input)
{
        (void) input;
        int type   = (int) li_get(n, IN_TYPE).i;
        Li_Value p = li_get(n, IN_PTR);
        /* Bytes on a wire are a copy: writing in them would change nothing */
        if (p.type == LI_PTR) mem_write(mem_at(p, li_get(n, IN_OFFSET), type), type, li_get(n, IN_VALUE));
        li_tick(n, 0);
}

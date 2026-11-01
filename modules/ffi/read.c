/* What's at "ptr" + "offset" bytes, as "type": an out-parameter a C
 * function filled, a field of a struct, a string in a buffer. Pure: read
 * when needed. */
#include "mem.h"

enum { IN_PTR,
       IN_OFFSET,
       IN_TYPE };

void
module_init(Li_Node *n)
{
        li_input(n, "ptr", LI_PTR);
        li_input_value(n, "offset", LI_INT, li_int(0));
        li_input_options(n, "type", TYPES, T_INT);
        li_output(n, "value", LI_ANY);
}

void
module_eval(Li_Node *n)
{
        li_set(n, 0, mem_read(mem_at(li_get(n, IN_PTR), li_get(n, IN_OFFSET)), (int) li_get(n, IN_TYPE).i));
}

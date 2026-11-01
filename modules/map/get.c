/* The value of "key", none if it's not there. Pure: read when needed. */
#include "common.h"

enum { IN_MAP,
       IN_KEY };

void
module_init(Li_Node *n)
{
        li_input_value(n, "map", LI_REF, li_var_ref(""));
        li_input_value(n, "key", LI_ANY, li_string(""));
        li_output(n, "value", LI_ANY);
}

void
module_eval(Li_Node *n)
{
        char buf[64];
        const char *map = map_var(n, IN_MAP);
        const char *k   = key(n, IN_KEY, buf, sizeof buf);
        MapOp op        = map ? map_read(map, k) : (MapOp) { 0 };
        li_set(n, 0, op.value);
        li_drop(&op.value);
}

void
module_draw(Li_Node *n)
{
        map_show(n, IN_MAP);
}

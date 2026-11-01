/* Number of keys. Pure: read when needed. */
#include "common.h"

void
module_init(Li_Node *n)
{
        li_input_value(n, "map", LI_REF, li_var_ref(""));
        li_output(n, "len", LI_INT);
}

void
module_eval(Li_Node *n)
{
        const char *map = map_var(n, 0);
        li_set(n, 0, li_int(map ? map_read(map, NULL).len : 0));
}

void
module_draw(Li_Node *n)
{
        map_show(n, 0);
}

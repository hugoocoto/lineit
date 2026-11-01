/* The keys of the map, in order, as a list. Loop over them with loop and
 * list/get, or keep them in a list variable with var/set. Pure: read when
 * needed. */
#include "common.h"

void
module_init(Li_Node *n)
{
        li_input_value(n, "map", LI_REF, li_var_ref(""));
        li_output(n, "keys", LI_LIST);
}

static void
list_keys(Li_Value *m, void *ctx)
{
        for (long i = 0; i < li_map_len(*m); i++)
                li_list_push(ctx, li_string(li_map_key(*m, i)));
}

void
module_eval(Li_Node *n)
{
        const char *map = map_var(n, 0);
        Li_Value keys   = { .type = LI_NONE };
        li_list_clear(&keys);
        if (map) li_var_update(map, list_keys, &keys);
        li_set(n, 0, keys);
        li_drop(&keys);
}

void
module_draw(Li_Node *n)
{
        map_show(n, 0);
}

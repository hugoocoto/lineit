/* On tick, removes "key" */
#include "common.h"

enum { IN_TICK,
       IN_MAP,
       IN_KEY };

void
module_init(Li_Node *n)
{
        li_input(n, "tick", LI_TICK);
        li_input_value(n, "map", LI_REF, li_var_ref(""));
        li_input_value(n, "key", LI_ANY, li_string(""));
        li_output(n, "then", LI_TICK);
}

static void
del(Li_Value *m, void *ctx)
{
        li_map_del(m, ((MapOp *) ctx)->key);
}

void
module_tick(Li_Node *n, int input)
{
        (void) input;
        char buf[64];
        const char *map = map_var(n, IN_MAP);
        const char *k   = key(n, IN_KEY, buf, sizeof buf);
        MapOp op        = { .key = k };
        if (map) li_var_update(map, del, &op);
        li_tick(n, 0);
}

void
module_draw(Li_Node *n)
{
        map_show(n, IN_MAP);
}

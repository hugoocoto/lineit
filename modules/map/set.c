/* On tick, sets "key" to "value", adding it if it's new */
#include "common.h"

enum { IN_TICK,
       IN_MAP,
       IN_KEY,
       IN_VALUE };

void
module_init(Li_Node *n)
{
        li_input(n, "tick", LI_TICK);
        li_input_value(n, "map", LI_REF, li_var_ref(""));
        li_input_value(n, "key", LI_ANY, li_string(""));
        li_input_value(n, "value", LI_ANY, li_int(0));
        li_output(n, "then", LI_TICK);
}

static void
set(Li_Value *m, void *ctx)
{
        MapOp *op = ctx;
        li_map_set(m, op->key, op->value);
}

void
module_tick(Li_Node *n, int input)
{
        (void) input;
        char buf[64];
        const char *map = map_var(n, IN_MAP);
        const char *k   = key(n, IN_KEY, buf, sizeof buf);
        MapOp op        = { .key = k, .value = li_get(n, IN_VALUE) };
        if (map) li_var_update(map, set, &op);
        li_tick(n, 0);
}

void
module_draw(Li_Node *n)
{
        map_show(n, IN_MAP);
}

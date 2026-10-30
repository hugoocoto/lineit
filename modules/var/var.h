/* Shared by var/get and var/set: the variable is the param "name", local to
 * the canvas the square is in (a subgraph has its own, like a C function),
 * or global to the tab if the param "global" is set. */
#include "lineit.h"

enum { PARAM_NAME,
       PARAM_GLOBAL };

static inline void
var_params(Li_Node *n)
{
        li_param(n, "name", li_string("x"));
        li_param(n, "global", li_bool(false));
}

static inline const char *
var_name(Li_Node *n)
{
        return li_get_param(n, PARAM_NAME).s;
}

static inline bool
var_global(Li_Node *n)
{
        return li_get_param(n, PARAM_GLOBAL).b;
}

static inline Li_Value
var_get(Li_Node *n)
{
        return var_global(n) ? li_global_get(var_name(n)) : li_var_get(var_name(n));
}

static inline void
var_set(Li_Node *n, Li_Value v)
{
        if (var_global(n))
                li_global_set(var_name(n), v);
        else
                li_var_set(var_name(n), v);
}

/* "name = value", "::name" for globals */
static inline void
var_show(Li_Node *n)
{
        char buf[64];
        li_format(var_get(n), buf, sizeof buf);
        li_text(n, "%s%s = %s", var_global(n) ? "::" : "", var_name(n), buf);
}

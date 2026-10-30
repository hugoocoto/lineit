/* index = where "needle" first is in s, in bytes from 0, or -1 if it isn't.
 * found is index >= 0. Pure: computed when read, no ticks needed. */
#include "str.h"

void
module_init(Li_Node *n)
{
        li_input_value(n, "s", LI_ANY, li_string(""));
        li_input_value(n, "needle", LI_STRING, li_string(""));
        li_output(n, "index", LI_INT);
        li_output(n, "found", LI_BOOL);
}

void
module_eval(Li_Node *n)
{
        char bs[64], bn[64];
        const char *s  = text(li_get(n, 0), bs, sizeof bs);
        const char *at = strstr(s, text(li_get(n, 1), bn, sizeof bn));
        long index     = at ? at - s : -1;
        li_set(n, 0, li_int(index));
        li_set(n, 1, li_bool(at != NULL));
        li_text(n, "%ld", index);
}

void
module_draw(Li_Node *n)
{
        module_eval(n);
}

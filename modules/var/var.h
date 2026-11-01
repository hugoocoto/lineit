/* Shared by var/get and var/set: the variable is the one of their "var"
 * input, wired from a var/var square or its name typed in it ("::x" for a
 * global). */
#include "lineit.h"

/* "x = 5" for the variable NAME, or a hint if there's none */
static inline void
var_show(Li_Node *n, const char *name)
{
        char buf[64];
        if (!name) {
                li_text(n, "wire or type a var");
                return;
        }
        Li_Value v = li_var_get(name);
        li_format(v, buf, sizeof buf);
        li_drop(&v);
        li_text(n, "%s = %s", name, buf);
}

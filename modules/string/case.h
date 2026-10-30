/* Shared by string/upper and string/lower. Before including it, define
 * CONVERT, the <ctype.h> function applied to every byte. ASCII only. Pure:
 * computed when read, no ticks needed. */
#include "str.h"
#include <ctype.h>

void
module_init(Li_Node *n)
{
        li_input_value(n, "s", LI_ANY, li_string(""));
        li_output(n, "text", LI_STRING);
}

void
module_eval(Li_Node *n)
{
        char buf[64];
        char *s = strdup(text(li_get(n, 0), buf, sizeof buf));
        for (char *p = s; *p; p++)
                *p = (char) CONVERT((unsigned char) *p);
        set_result(n, 0, s);
}

void
module_draw(Li_Node *n)
{
        module_eval(n);
}

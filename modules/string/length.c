/* length = bytes in s. Pure: computed when read, no ticks needed. */
#include "str.h"

void
module_init(Li_Node *n)
{
        li_input_value(n, "s", LI_ANY, li_string(""));
        li_output(n, "length", LI_INT);
}

void
module_eval(Li_Node *n)
{
        char buf[64];
        long len = (long) strlen(text(li_get(n, 0), buf, sizeof buf));
        li_set(n, 0, li_int(len));
        li_text(n, "%ld", len);
}

void
module_draw(Li_Node *n)
{
        module_eval(n);
}

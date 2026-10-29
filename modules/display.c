/* Shows the value of its input, always up to date: no ticks needed */
#include "lineit.h"

void
module_init(Li_Node *n)
{
        li_input(n, "", LI_ANY);
}

void
module_draw(Li_Node *n)
{
        char buf[64];
        li_format(li_get(n, 0), buf, sizeof buf);
        li_text(n, "%s", buf);
}

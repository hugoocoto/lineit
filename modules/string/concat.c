/* text = a followed by b. Numbers and bools are written as text. Pure:
 * computed when read, no ticks needed. */
#include "str.h"

void
module_init(Li_Node *n)
{
        li_input_value(n, "a", LI_ANY, li_string(""));
        li_input_value(n, "b", LI_ANY, li_string(""));
        li_output(n, "text", LI_STRING);
}

void
module_eval(Li_Node *n)
{
        char ba[64], bb[64];
        const char *a = text(li_get(n, 0), ba, sizeof ba);
        const char *b = text(li_get(n, 1), bb, sizeof bb);
        size_t la = strlen(a), lb = strlen(b);
        char *s   = malloc(la + lb + 1);
        memcpy(s, a, la);
        memcpy(s + la, b, lb + 1);
        set_result(n, 0, s);
}

void
module_draw(Li_Node *n)
{
        module_eval(n);
}

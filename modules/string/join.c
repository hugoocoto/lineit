/* text = every item of "list" as text, with "sep" between them. Wire a
 * list to it: var/get of a list variable, split, map/keys. Pure: computed
 * when read, no ticks needed. */
#include "str.h"

enum { IN_LIST,
       IN_SEP };

void
module_init(Li_Node *n)
{
        li_input(n, "list", LI_LIST);
        li_input_value(n, "sep", LI_STRING, li_string(","));
        li_output(n, "text", LI_STRING);
}

void
module_eval(Li_Node *n)
{
        char bp[64], buf[64];
        Li_Value list   = li_get(n, IN_LIST);
        const char *sep = text(li_get(n, IN_SEP), bp, sizeof bp);
        char *s;
        size_t size;
        FILE *f = open_memstream(&s, &size);

        for (long i = 0; i < li_list_len(list); i++) {
                if (i) fputs(sep, f);
                fputs(text(li_list_get(list, i), buf, sizeof buf), f);
        }
        fclose(f);
        set_result(n, 0, s);
}

void
module_draw(Li_Node *n)
{
        module_eval(n);
}

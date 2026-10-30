/* text = every item of the list (a param, like the list modules) as text,
 * with "sep" between them. Pure: computed when read, no ticks needed. */
#include "str.h"

#define LIST(n) (li_get_param((n), 0).s)

void
module_init(Li_Node *n)
{
        li_input_value(n, "sep", LI_STRING, li_string(","));
        li_output(n, "text", LI_STRING);
        li_param(n, "list", li_string("l"));
}

void
module_eval(Li_Node *n)
{
        char bp[64], buf[64];
        const char *sep = text(li_get(n, 0), bp, sizeof bp);
        long len        = li_list_len(LIST(n));
        char *s;
        size_t size;
        FILE *f = open_memstream(&s, &size);

        for (long i = 0; i < len; i++) {
                if (i) fputs(sep, f);
                fputs(text(li_list_get(LIST(n), i), buf, sizeof buf), f);
        }
        fclose(f);
        set_result(n, 0, s);
}

void
module_draw(Li_Node *n)
{
        module_eval(n);
}

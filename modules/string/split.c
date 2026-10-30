/* On tick, replaces the list (a param, like the list modules) with the
 * parts of s between each "sep": "a,b,,c" gives "a", "b", "", "c". An empty
 * sep splits it in bytes. */
#include "str.h"

enum { IN_TICK,
       IN_S,
       IN_SEP };

#define LIST(n) (li_get_param((n), 0).s)

void
module_init(Li_Node *n)
{
        li_input(n, "tick", LI_TICK);
        li_input_value(n, "s", LI_ANY, li_string(""));
        li_input_value(n, "sep", LI_STRING, li_string(","));
        li_output(n, "then", LI_TICK);
        li_param(n, "list", li_string("l"));
}

void
module_tick(Li_Node *n, int input)
{
        (void) input;
        char bs[64], bp[64];
        const char *s   = text(li_get(n, IN_S), bs, sizeof bs);
        const char *sep = text(li_get(n, IN_SEP), bp, sizeof bp);
        size_t seplen   = strlen(sep);

        li_list_clear(LIST(n));
        if (!seplen) {
                for (; *s; s++) {
                        char c[2] = { *s, '\0' };
                        li_list_push(LIST(n), li_string(c));
                }
        } else {
                for (;;) {
                        const char *end = strstr(s, sep);
                        char *part      = strndup(s, end ? (size_t) (end - s) : strlen(s));
                        li_list_push(LIST(n), li_string(part)); /* copied */
                        free(part);
                        if (!end) break;
                        s = end + seplen;
                }
        }
        li_tick(n, 0);
}

void
module_draw(Li_Node *n)
{
        li_text(n, "%s [%ld]", LIST(n), li_list_len(LIST(n)));
}

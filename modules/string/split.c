/* The parts of s between each "sep", as a list: "a,b,,c" gives "a", "b",
 * "", "c". An empty sep splits it in bytes. Pure: computed when read. Keep
 * it in a list variable with var/set. */
#define STR_OWN_DESTROY
#include "str.h"

enum { IN_S,
       IN_SEP };

static Li_Value parts; /* the output, until the next time it's read */

void
module_init(Li_Node *n)
{
        li_input_value(n, "s", LI_ANY, li_string(""));
        li_input_value(n, "sep", LI_STRING, li_string(","));
        li_output(n, "parts", LI_LIST);
}

void
module_eval(Li_Node *n)
{
        char bs[64], bp[64];
        const char *s   = text(li_get(n, IN_S), bs, sizeof bs);
        const char *sep = text(li_get(n, IN_SEP), bp, sizeof bp);
        size_t seplen   = strlen(sep);

        li_list_clear(&parts);
        if (!seplen) {
                for (; *s; s++) {
                        char c[2] = { *s, '\0' };
                        li_list_push(&parts, li_string(c));
                }
        } else {
                for (;;) {
                        const char *end = strstr(s, sep);
                        char *part      = strndup(s, end ? (size_t) (end - s) : strlen(s));
                        li_list_push(&parts, li_string(part)); /* copied */
                        free(part);
                        if (!end) break;
                        s = end + seplen;
                }
        }
        li_set(n, 0, parts);
}

void
module_draw(Li_Node *n)
{
        module_eval(n);
        li_text(n, "[%ld]", li_list_len(parts));
}

void
module_destroy(Li_Node *n)
{
        (void) n;
        li_drop(&parts);
        free(result);
        result = NULL;
}

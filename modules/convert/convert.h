/* Shared by the convert modules. Before including it, define OUTPUT (the
 * output label), TYPE (its type) and, after it:
 *   static bool convert(Li_Value v, Li_Value *out);
 * that returns false when V can't be converted: the output has no value
 * then. Pure: computed when read, no ticks needed. */
#include "lineit.h"
#include <errno.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>

static bool convert(Li_Value v, Li_Value *out);

void
module_init(Li_Node *n)
{
        li_input_value(n, "value", LI_ANY, li_string(""));
        li_output(n, OUTPUT, TYPE);
}

/* S as a number, all of it but surrounding spaces. False if it isn't one. */
static bool
parse_number(const char *s, bool *is_float, long *i, double *f)
{
        char *end;
        if (!s) return false;
        errno = 0;
        *i    = strtol(s, &end, 10);
        if (end != s && !errno) {
                while (*end == ' ') end++;
                if (!*end) {
                        *f        = (double) *i;
                        *is_float = false;
                        return true;
                }
        }
        errno = 0;
        *f    = strtod(s, &end);
        if (end == s || errno) return false;
        while (*end == ' ') end++;
        *is_float = true;
        return !*end;
}

void
module_eval(Li_Node *n)
{
        Li_Value v;
        char buf[64];
        if (!convert(li_get(n, 0), &v)) {
                li_set(n, 0, (Li_Value) { .type = LI_NONE });
                li_text(n, "can't");
                return;
        }
        li_set(n, 0, v);
        li_format(v, buf, sizeof buf);
        li_text(n, "%s", buf);
}

void
module_draw(Li_Node *n)
{
        module_eval(n);
}

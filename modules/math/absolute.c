/* |a|, int or float. Bools count as 0 or 1. Pure: computed when read, no
 * ticks needed. Compiled, it's inline (module_emit): the program includes
 * this file again with LI_INLINE defined, which leaves only absolute(). */
#include "lineit.h"
#include <limits.h>
#include <math.h>
#include <stddef.h>

#ifndef ABSOLUTE_C_
#define ABSOLUTE_C_

/* |A|, no value if there's none, with *WHY saying why */
static inline Li_Value
absolute(Li_Value a, const char **why)
{
        *why = NULL;
        switch (a.type) {
        case LI_NONE: return li_int(0);
        case LI_BOOL: return li_int(a.b);
        case LI_FLOAT: return li_float(fabs(a.f));
        case LI_INT:
                if (a.i != LONG_MIN) return li_int(a.i < 0 ? -a.i : a.i);
                *why = "overflow";
                break;
        default: *why = "not a number"; break;
        }
        return (Li_Value) { .type = LI_NONE };
}

#endif /* ABSOLUTE_C_ */

#ifndef LI_INLINE

void
module_init(Li_Node *n)
{
        li_input_value(n, "a", LI_ANY, li_int(0));
        li_output(n, "|a|", LI_ANY);
}

void
module_eval(Li_Node *n)
{
        const char *why;
        char buf[64];
        Li_Value v = absolute(li_get(n, 0), &why);

        li_set(n, 0, v);
        if (why) {
                li_text(n, "%s", why);
                return;
        }
        li_format(v, buf, sizeof buf);
        li_text(n, "%s", buf);
}

void
module_draw(Li_Node *n)
{
        module_eval(n);
}

const char *
module_emit(Li_Node *n)
{
        (void) n;
        return "#define LI_INLINE\n"
               "#include \"$source\"\n"
               "#undef LI_INLINE\n"
               "static Li_Value\n"
               "$_out0(void)\n"
               "{\n"
               "        const char *why;\n"
               "        return absolute($_in0(), &why);\n"
               "}\n";
}

#endif /* LI_INLINE */

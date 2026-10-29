/* a <op> b, with op chosen in the square from a list. Pure: computed when
 * read, no ticks needed. */
#define COMPARE_NO_MODULE
#include "compare.h"

enum { IN_OP,
       IN_A,
       IN_B };

void
module_init(Li_Node *n)
{
        li_input_options(n, "op", "==|!=|<|<=|>|>=", 0);
        li_input_value(n, "a", LI_ANY, li_int(0));
        li_input_value(n, "b", LI_ANY, li_int(0));
        li_output(n, "", LI_BOOL);
}

void
module_eval(Li_Node *n)
{
        int c;
        long op     = li_get(n, IN_OP).i;
        bool result = op == 1; /* unordered: only != is true */

        if (compare(li_get(n, IN_A), li_get(n, IN_B), &c)) {
                bool r[] = { c == 0, c != 0, c < 0, c <= 0, c > 0, c >= 0 };
                result   = r[op];
        }
        li_set(n, 0, li_bool(result));
        li_text(n, "%s", result ? "true" : "false");
}

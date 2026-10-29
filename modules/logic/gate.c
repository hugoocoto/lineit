/* a <op> b, with op chosen in the square from a list. Pure: computed when
 * read, no ticks needed. */
#define LOGIC_NO_MODULE
#include "logic.h"

enum { IN_OP,
       IN_A,
       IN_B };

void
module_init(Li_Node *n)
{
        li_input_options(n, "op", "and|or|xor|nand|nor|xnor", 0);
        li_input_value(n, "a", LI_BOOL, li_bool(false));
        li_input_value(n, "b", LI_BOOL, li_bool(false));
        li_output(n, "", LI_BOOL);
}

void
module_eval(Li_Node *n)
{
        bool a      = as_bool(li_get(n, IN_A));
        bool b      = as_bool(li_get(n, IN_B));
        bool r[]    = { a && b, a || b, a != b, !(a && b), !(a || b), a == b };
        bool result = r[li_get(n, IN_OP).i];
        li_set(n, 0, li_bool(result));
        li_text(n, "%s", result ? "true" : "false");
}

void
module_draw(Li_Node *n)
{
        module_eval(n);
}

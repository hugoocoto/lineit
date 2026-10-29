/* Shared by the comparison modules. Before including it, define (or define
 * COMPARE_NO_MODULE to get only compare()):
 *   OUTPUT          output label, "<"
 *   RESULT(c)       the result, from c: <0, 0 or >0 as a is less, equal or
 *                   greater than b
 *   UNORDERED       the result when a and b can't be compared (a string and
 *                   a number, or an unplugged input)
 * Numbers (int, float, bool) compare by value, strings with strcmp. */
#include "lineit.h"
#include <string.h>

static bool
is_number(Li_Value v)
{
        return v.type == LI_INT || v.type == LI_FLOAT || v.type == LI_BOOL;
}

static double
number(Li_Value v)
{
        return v.type == LI_INT ? (double) v.i : v.type == LI_FLOAT ? v.f :
                                                                      v.b;
}

/* Like strcmp: <0, 0 or >0. Returns false if they can't be compared. */
static bool
compare(Li_Value a, Li_Value b, int *c)
{
        if (a.type == LI_INT && b.type == LI_INT) {
                *c = (a.i > b.i) - (a.i < b.i);
                return true;
        }
        if (is_number(a) && is_number(b)) {
                double x = number(a), y = number(b);
                *c = (x > y) - (x < y);
                return true;
        }
        if (a.type == LI_STRING && b.type == LI_STRING) {
                *c = strcmp(a.s ? a.s : "", b.s ? b.s : "");
                return true;
        }
        return false;
}

#ifndef COMPARE_NO_MODULE

void
module_init(Li_Node *n)
{
        /* Typed in the square when nothing is plugged in */
        li_input_value(n, "a", LI_ANY, li_int(0));
        li_input_value(n, "b", LI_ANY, li_int(0));
        li_output(n, OUTPUT, LI_BOOL);
}

void
module_eval(Li_Node *n)
{
        int c;
        bool result = compare(li_get(n, 0), li_get(n, 1), &c) ? (RESULT(c)) : UNORDERED;
        li_set(n, 0, li_bool(result));
        li_text(n, "%s", result ? "true" : "false");
}

#endif

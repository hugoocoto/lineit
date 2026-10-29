/* Shared by the math modules. Before including it, define OUTPUT (the output
 * label). After it, define:
 *   static const char *int_op(long a, long b, long *result);
 *   static const char *float_op(double a, double b, double *result);
 * They return NULL on success, or the text to show when there is no result
 * ("/ 0", "overflow").
 *
 * Inputs take ints, floats and bools (as 0 or 1), typed or wired. If both are
 * ints the result is an int (int_op), if any is a float it's a float
 * (float_op). Anything else gives no value. Pure: computed when read, no
 * ticks needed. */
#include "lineit.h"
#include <stddef.h>

static const char *int_op(long a, long b, long *result);
static const char *float_op(double a, double b, double *result);

void
module_init(Li_Node *n)
{
        /* Typed in the square when nothing is plugged in */
        li_input_value(n, "a", LI_ANY, li_int(0));
        li_input_value(n, "b", LI_ANY, li_int(0));
        /* int or float, depending on the inputs */
        li_output(n, OUTPUT, LI_ANY);
}

/* Nothing typed counts as int 0. Returns false if V is not a number. */
static bool
number(Li_Value v, bool *is_float, long *i, double *f)
{
        switch (v.type) {
        case LI_NONE: *i = 0; break;
        case LI_INT: *i = v.i; break;
        case LI_BOOL: *i = v.b; break;
        case LI_FLOAT:
                *f        = v.f;
                *is_float = true;
                return true;
        default: return false;
        }
        *f        = (double) *i;
        *is_float = false;
        return true;
}

static void
fail(Li_Node *n, const char *why)
{
        li_set(n, 0, (Li_Value) { .type = LI_NONE });
        li_text(n, "%s", why);
}

void
module_eval(Li_Node *n)
{
        bool af, bf;
        long ai, bi;
        double a, b;
        const char *err;
        char buf[64];

        if (!number(li_get(n, 0), &af, &ai, &a) || !number(li_get(n, 1), &bf, &bi, &b)) {
                fail(n, "not a number");
                return;
        }

        Li_Value v;
        if (af || bf) {
                v.type = LI_FLOAT;
                err    = float_op(a, b, &v.f);
        } else {
                v.type = LI_INT;
                err    = int_op(ai, bi, &v.i);
        }
        if (err) {
                fail(n, err);
                return;
        }

        li_set(n, 0, v);
        li_format(v, buf, sizeof buf);
        li_text(n, "%s", buf);
}

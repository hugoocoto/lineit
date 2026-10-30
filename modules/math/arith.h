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
 * ticks needed.
 *
 * Compiled, they are inline (module_emit): the program includes the module
 * again with LI_INLINE defined, which leaves only int_op, float_op and
 * arith below, renamed for the square, and calls arith with them. */
#include "lineit.h"
#include <stddef.h>

static const char *int_op(long a, long b, long *result);
static const char *float_op(double a, double b, double *result);

#ifndef ARITH_H_
#define ARITH_H_

/* Nothing typed counts as int 0. Returns false if V is not a number. */
static inline bool
arith_number(Li_Value v, bool *is_float, long *i, double *f)
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

/* A op B: with INT if both are ints, with FLT if any is a float. No value
 * if there's none, with *WHY saying why. */
static inline Li_Value
arith(Li_Value a, Li_Value b, const char *(*int_fn)(long, long, long *),
      const char *(*float_fn)(double, double, double *), const char **why)
{
        bool af, bf;
        long ai, bi;
        double fa, fb;
        Li_Value v;

        *why = "not a number";
        if (!arith_number(a, &af, &ai, &fa) || !arith_number(b, &bf, &bi, &fb)) return (Li_Value) { .type = LI_NONE };
        if (af || bf) {
                v.type = LI_FLOAT;
                *why   = float_fn(fa, fb, &v.f);
        } else {
                v.type = LI_INT;
                *why   = int_fn(ai, bi, &v.i);
        }
        return *why ? (Li_Value) { .type = LI_NONE } : v;
}

#endif /* ARITH_H_ */

#ifndef LI_INLINE

void
module_init(Li_Node *n)
{
        /* Typed in the square when nothing is plugged in */
        li_input_value(n, "a", LI_ANY, li_int(0));
        li_input_value(n, "b", LI_ANY, li_int(0));
        /* int or float, depending on the inputs */
        li_output(n, OUTPUT, LI_ANY);
}

void
module_eval(Li_Node *n)
{
        const char *why;
        char buf[64];
        Li_Value v = arith(li_get(n, 0), li_get(n, 1), int_op, float_op, &why);

        li_set(n, 0, v);
        if (why) {
                li_text(n, "%s", why);
                return;
        }
        li_format(v, buf, sizeof buf);
        li_text(n, "%s", buf);
}

const char *
module_emit(Li_Node *n)
{
        (void) n;
        return "#define LI_INLINE\n"
               "#define int_op $_int_op\n"
               "#define float_op $_float_op\n"
               "#include \"$source\"\n"
               "#undef int_op\n"
               "#undef float_op\n"
               "#undef OUTPUT\n"
               "#undef LI_INLINE\n"
               "static Li_Value\n"
               "$_out0(void)\n"
               "{\n"
               "        const char *why;\n"
               "        return arith($_in0(), $_in1(), $_int_op, $_float_op, &why);\n"
               "}\n";
}

#endif /* LI_INLINE */

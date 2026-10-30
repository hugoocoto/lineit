/* On tick, picks a new number between min and max (both included) and
 * ticks then. Ints give an int; if any is a float, a float in [min, max).
 * The same seed (a param) always gives the same numbers, from Start. */
#include "lineit.h"
#include <stdint.h>

enum { IN_TICK,
       IN_MIN,
       IN_MAX };
enum { OUT_THEN,
       OUT_VALUE };
enum { PARAM_SEED };

static uint64_t state;

/* xorshift64*, never 0 */
static uint64_t
next(void)
{
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        return state * 0x2545F4914F6CDD1DULL;
}

static void
seed(Li_Node *n)
{
        Li_Value v = li_get_param(n, PARAM_SEED);
        state      = (uint64_t) v.i * 0x9E3779B97F4A7C15ULL;
        if (!state) state = 1;
}

void
module_init(Li_Node *n)
{
        li_input(n, "tick", LI_TICK);
        li_input_value(n, "min", LI_ANY, li_int(0));
        li_input_value(n, "max", LI_ANY, li_int(99));
        li_output(n, "then", LI_TICK);
        li_output(n, "value", LI_ANY);
        li_param(n, "seed", li_int(1));
}

void
module_param(Li_Node *n, int param)
{
        (void) param;
        seed(n);
}

void
module_start(Li_Node *n)
{
        seed(n);
}

static bool
number(Li_Value v, double *f, bool *is_float)
{
        switch (v.type) {
        case LI_NONE: *f = 0; return true;
        case LI_INT: *f = (double) v.i; return true;
        case LI_BOOL: *f = v.b; return true;
        case LI_FLOAT: *f = v.f, *is_float = true; return true;
        default: return false;
        }
}

void
module_tick(Li_Node *n, int input)
{
        (void) input;
        Li_Value a = li_get(n, IN_MIN), b = li_get(n, IN_MAX), v;
        bool is_float = false;
        double lo, hi;
        char buf[64];

        if (!number(a, &lo, &is_float) || !number(b, &hi, &is_float)) {
                li_set(n, OUT_VALUE, (Li_Value) { .type = LI_NONE });
                li_text(n, "not a number");
                li_tick(n, OUT_THEN);
                return;
        }
        if (lo > hi) {
                double t = lo;
                lo = hi, hi = t;
        }
        if (is_float) {
                v = li_float(lo + (hi - lo) * ((next() >> 11) * 0x1.0p-53));
        } else {
                long l = (long) lo, h = (long) hi;
                uint64_t range = (uint64_t) h - (uint64_t) l + 1; /* 0: every long */
                v = li_int((long) ((uint64_t) l + (range ? next() % range : next())));
        }
        li_set(n, OUT_VALUE, v);
        li_format(v, buf, sizeof buf);
        li_text(n, "%s", buf);
        li_tick(n, OUT_THEN);
}

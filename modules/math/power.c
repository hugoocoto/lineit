/* power = a to the b, int or float. With ints, b must be >= 0 (type a float
 * for a negative one). */
#define OUTPUT "power"
#include "arith.h"
#include <math.h>

static const char *
int_op(long a, long b, long *result)
{
        long r = 1;
        if (b < 0) return "b < 0";
        /* By squaring, checking every step */
        for (; b; b >>= 1) {
                if ((b & 1) && __builtin_mul_overflow(r, a, &r)) return "overflow";
                if (b > 1 && __builtin_mul_overflow(a, a, &a)) return "overflow";
        }
        *result = r;
        return NULL;
}

static const char *
float_op(double a, double b, double *result)
{
        *result = pow(a, b);
        return isnan(*result) ? "not real" : NULL;
}

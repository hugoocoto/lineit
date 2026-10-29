/* remainder = a % b, int (sign of a, like C) or float (fmod). b = 0 gives no value. */
#define OUTPUT "remainder"
#include "arith.h"
#include <limits.h>
#include <math.h>

static const char *
int_op(long a, long b, long *result)
{
        if (b == 0) return "% 0";
        if (a == LONG_MIN && b == -1) return "overflow";
        *result = a % b;
        return NULL;
}

static const char *
float_op(double a, double b, double *result)
{
        if (b == 0) return "% 0";
        *result = fmod(a, b);
        return NULL;
}

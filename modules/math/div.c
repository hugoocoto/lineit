/* quotient = a / b, int (rounded towards zero, like C) or float. b = 0 gives no value. */
#define OUTPUT "quotient"
#include "arith.h"
#include <limits.h>

static const char *
int_op(long a, long b, long *result)
{
        if (b == 0) return "/ 0";
        if (a == LONG_MIN && b == -1) return "overflow";
        *result = a / b;
        return NULL;
}

static const char *
float_op(double a, double b, double *result)
{
        if (b == 0) return "/ 0";
        *result = a / b;
        return NULL;
}

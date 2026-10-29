/* sum = a + b, int or float */
#define OUTPUT "sum"
#include "arith.h"

static const char *
int_op(long a, long b, long *result)
{
        return __builtin_add_overflow(a, b, result) ? "overflow" : NULL;
}

static const char *
float_op(double a, double b, double *result)
{
        *result = a + b;
        return NULL;
}

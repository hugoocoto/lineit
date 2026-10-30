/* min = the smaller of a and b, int or float */
#define OUTPUT "min"
#include "arith.h"

static const char *
int_op(long a, long b, long *result)
{
        *result = a < b ? a : b;
        return NULL;
}

static const char *
float_op(double a, double b, double *result)
{
        *result = a < b ? a : b;
        return NULL;
}

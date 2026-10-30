/* The value as an int: floats rounded towards zero, bools 0 or 1, and
 * strings read as a number ("42", "2.5" is 2). No value for anything else. */
#define OUTPUT "int"
#define TYPE   LI_INT
#include "convert.h"
#include <limits.h>

static bool
convert(Li_Value v, Li_Value *out)
{
        bool is_float;
        long i;
        double f;
        switch (v.type) {
        case LI_INT: *out = v; return true;
        case LI_BOOL: *out = li_int(v.b); return true;
        case LI_FLOAT: f = v.f; break;
        case LI_STRING:
                if (!parse_number(v.s, &is_float, &i, &f)) return false;
                if (!is_float) {
                        *out = li_int(i);
                        return true;
                }
                break;
        default: return false;
        }
        if (!(f > (double) LONG_MIN && f < (double) LONG_MAX)) return false; /* also NaN */
        *out = li_int((long) f);
        return true;
}

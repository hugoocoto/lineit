/* The value as a float: ints as they are, bools 0 or 1, and strings read as
 * a number. No value for anything else. */
#define OUTPUT "float"
#define TYPE   LI_FLOAT
#include "convert.h"

static bool
convert(Li_Value v, Li_Value *out)
{
        bool is_float;
        long i;
        double f;
        switch (v.type) {
        case LI_FLOAT: *out = v; return true;
        case LI_INT: *out = li_float((double) v.i); return true;
        case LI_BOOL: *out = li_float(v.b); return true;
        case LI_STRING:
                if (!parse_number(v.s, &is_float, &i, &f)) return false;
                *out = li_float(f);
                return true;
        default: return false;
        }
}

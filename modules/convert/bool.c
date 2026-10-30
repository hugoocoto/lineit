/* The value as a bool: numbers are true if they aren't 0, and strings if
 * they are "true", "yes" or "1" (false for "false", "no", "0" and ""). No
 * value for anything else. */
#define OUTPUT "bool"
#define TYPE   LI_BOOL
#include "convert.h"

static bool
convert(Li_Value v, Li_Value *out)
{
        const char *s = v.s;
        switch (v.type) {
        case LI_BOOL: *out = v; return true;
        case LI_INT: *out = li_bool(v.i != 0); return true;
        case LI_FLOAT: *out = li_bool(v.f != 0); return true;
        case LI_STRING:
                if (!s || !*s || !strcasecmp(s, "false") || !strcasecmp(s, "no") || !strcmp(s, "0"))
                        *out = li_bool(false);
                else if (!strcasecmp(s, "true") || !strcasecmp(s, "yes") || !strcmp(s, "1"))
                        *out = li_bool(true);
                else
                        return false;
                return true;
        default: return false;
        }
}

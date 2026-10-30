/* The value as text, like print shows it: "42", "2.5", "true". No value if
 * nothing is plugged in. */
#define OUTPUT "string"
#define TYPE   LI_STRING
#include "convert.h"
#include <stdio.h>

static char text[256];

static bool
convert(Li_Value v, Li_Value *out)
{
        if (v.type == LI_STRING) {
                *out = v;
                return true;
        }
        if (v.type == LI_NONE) return false;
        li_format(v, text, sizeof text);
        *out = li_string(text);
        return true;
}

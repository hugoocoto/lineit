/* A constant: "value" (typed in the square, or wired) as the type chosen in
 * "type". The output takes that type, so wires are checked against it. No
 * value if it can't be converted ("abc" as an int). */
#include "lineit.h"
#include <errno.h>
#include <stdlib.h>
#include <string.h>

enum { IN_VALUE,
       IN_TYPE };

static const Li_Type types[] = { LI_INT, LI_FLOAT, LI_BOOL, LI_STRING };

/* For the string output, alive until the next conversion */
static char text[256];

/* V as TYPE in OUT. False if it can't be. */
static bool
convert(Li_Value v, Li_Type type, Li_Value *out)
{
        char *end;

        if (v.type == LI_NONE) return false;
        if (type == LI_STRING) {
                li_format(v, text, sizeof text);
                *out = li_string(text);
                return true;
        }

        /* Numbers and bools convert to each other, strings are parsed */
        double d;
        switch (v.type) {
        case LI_INT: d = v.i; break;
        case LI_FLOAT: d = v.f; break;
        case LI_BOOL: d = v.b; break;
        case LI_STRING:
                if (type == LI_BOOL) {
                        if (!strcmp(v.s, "true") || !strcmp(v.s, "1"))
                                d = 1;
                        else if (!strcmp(v.s, "false") || !strcmp(v.s, "0"))
                                d = 0;
                        else
                                return false;
                        break;
                }
                errno = 0;
                d     = type == LI_INT ? (double) strtol(v.s, &end, 0) : strtod(v.s, &end);
                if (end == v.s || *end || errno) return false;
                break;
        default: return false;
        }

        *out = type == LI_INT ? li_int((long) d) : type == LI_FLOAT ? li_float(d) :
                                                                      li_bool(d != 0);
        return true;
}

void
module_init(Li_Node *n)
{
        li_input_value(n, "value", LI_ANY, li_int(0));
        li_input_options(n, "type", "int|float|bool|string", 0);
        li_output(n, "", LI_INT);
}

void
module_eval(Li_Node *n)
{
        Li_Type type = types[li_get(n, IN_TYPE).i];
        Li_Value v;
        char buf[64];

        li_output_type(n, 0, type);
        if (!convert(li_get(n, IN_VALUE), type, &v)) {
                li_set(n, 0, (Li_Value) { .type = LI_NONE });
                li_text(n, "not %s", li_option(n, IN_TYPE));
                return;
        }
        li_set(n, 0, v);
        li_format(v, buf, sizeof buf);
        li_text(n, "%s", buf);
}

/* Show it (and update the output type) even if nothing reads it */
void
module_draw(Li_Node *n)
{
        module_eval(n);
}

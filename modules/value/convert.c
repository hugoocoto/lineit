/* "value" as the type chosen in "to": the output takes that type, so wires
 * are checked against it. No value if it can't be converted ("abc" as an
 * int).
 *   int     floats rounded towards zero, bools 0 or 1, strings read as a
 *           number ("42", "2.5" is 2)
 *   float   ints as they are, bools 0 or 1, strings read as a number
 *   bool    numbers are true if they aren't 0, strings if they are "true",
 *           "yes" or "1" (false for "false", "no", "0" and "")
 *   string  as print shows it: "42", "2.5", "true"
 * Pure: computed when read, no ticks needed. */
#include "lineit.h"
#include <errno.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>

enum { IN_VALUE,
       IN_TO };

static const Li_Type types[] = { LI_INT, LI_FLOAT, LI_BOOL, LI_STRING };

static char text[256]; /* the string result */

void
module_init(Li_Node *n)
{
        li_input_value(n, "value", LI_ANY, li_string(""));
        li_input_options(n, "to", "int|float|bool|string", 0);
        li_output(n, "value", LI_INT);
}

static Li_Type
target(Li_Node *n)
{
        Li_Value v = li_get(n, IN_TO);
        return v.i >= 0 && v.i < (long) (sizeof types / sizeof *types) ? types[v.i] : LI_INT;
}

/* S as a number, all of it but surrounding spaces. False if it isn't one. */
static bool
parse_number(const char *s, bool *is_float, long *i, double *f)
{
        char *end;
        if (!s) return false;
        errno = 0;
        *i    = strtol(s, &end, 10);
        if (end != s && !errno) {
                while (*end == ' ')
                        end++;
                if (!*end) {
                        *f        = (double) *i;
                        *is_float = false;
                        return true;
                }
        }
        errno = 0;
        *f    = strtod(s, &end);
        if (end == s || errno) return false;
        while (*end == ' ')
                end++;
        *is_float = true;
        return !*end;
}

static bool
to_int(Li_Value v, Li_Value *out)
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

static bool
to_float(Li_Value v, Li_Value *out)
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

static bool
to_bool(Li_Value v, Li_Value *out)
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

static bool
to_string(Li_Value v, Li_Value *out)
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

void
module_eval(Li_Node *n)
{
        Li_Value in = li_get(n, IN_VALUE), v;
        Li_Type to  = target(n);
        char buf[64];
        bool ok = to == LI_INT ? to_int(in, &v) : to == LI_FLOAT ? to_float(in, &v) :
                  to == LI_BOOL ? to_bool(in, &v) : to_string(in, &v);

        li_output_type(n, 0, to);
        if (!ok) {
                li_set(n, 0, (Li_Value) { .type = LI_NONE });
                li_text(n, "can't");
                return;
        }
        li_set(n, 0, v);
        li_format(v, buf, sizeof buf);
        li_text(n, "%s", buf);
}

/* Show it, and the type it gives, even if nothing reads it */
void
module_draw(Li_Node *n)
{
        module_eval(n);
}

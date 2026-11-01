/* A variable: the one called "name" in the canvas the square is in (a
 * subgraph has its own, like a C function), or in the whole tab if
 * "global" is set. Its "var" output goes to the squares that read or
 * change it: var/get, var/set, and the list and map squares (or they type
 * its name, without a wire). It holds a
 * value of "type" (any: whatever is set), and starts as "start" when Run is
 * pressed (lists and maps start empty).
 *
 * Squares with the same name are the same variable: copies, so its wires
 * can be short. They have to say the same type and start, or the editor
 * shows them as an error and doesn't run. */
#include "lineit.h"
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

enum { IN_TYPE,
       IN_START };
enum { PARAM_NAME,
       PARAM_GLOBAL };

static const Li_Type types[] = { LI_ANY, LI_INT, LI_FLOAT, LI_BOOL, LI_STRING, LI_LIST, LI_MAP };
#define TYPES "any|int|float|bool|string|list|map"

static char ref[128];  /* its name for li_var: "x", or "::x" */
static char text[256]; /* the start, when it's a string */

/* V as TYPE in OUT. False if it can't be ("abc" as an int). */
static bool
convert(Li_Value v, Li_Type type, Li_Value *out)
{
        char *end;
        double d;

        if (type == LI_ANY || type == v.type) {
                *out = v;
                return true;
        }
        if (type == LI_LIST || type == LI_MAP) {
                *out = (Li_Value) { .type = type }; /* empty */
                return true;
        }
        if (v.type == LI_NONE) return false;
        if (type == LI_STRING) {
                li_format(v, text, sizeof text);
                *out = li_string(text);
                return true;
        }
        switch (v.type) {
        case LI_INT: d = v.i; break;
        case LI_FLOAT: d = v.f; break;
        case LI_BOOL: d = v.b; break;
        case LI_STRING:
                if (type == LI_BOOL) {
                        if (strcmp(v.s, "true") && strcmp(v.s, "false")) return false;
                        d = !strcmp(v.s, "true");
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
        li_input_options(n, "type", TYPES, 0);
        li_input_value(n, "start", LI_ANY, li_int(0));
        li_output(n, "var", LI_REF);
        li_param(n, "name", li_string("x"));
        li_param(n, "global", li_bool(false));
}

static Li_Type
type(Li_Node *n)
{
        long i = li_get(n, IN_TYPE).i;
        return i >= 0 && i < (long) (sizeof types / sizeof *types) ? types[i] : LI_ANY;
}

/* Read when a square wired to it reads the variable: says what it is, so
 * it's set to its start if it isn't yet (or the type or start changed) */
void
module_eval(Li_Node *n)
{
        Li_Type t = type(n);
        Li_Value start;

        snprintf(ref, sizeof ref, "%s%s", li_get_param(n, PARAM_GLOBAL).b ? "::" : "", li_get_param(n, PARAM_NAME).s);
        if (!convert(li_get(n, IN_START), t, &start)) start = (Li_Value) { .type = LI_NONE };
        li_var_declare(ref, t, start);
        li_set(n, 0, (Li_Value) { .type = LI_REF, .s = ref });
}

void
module_draw(Li_Node *n)
{
        char buf[64];
        module_eval(n);
        Li_Value v = li_var_get(ref);
        li_format(v, buf, sizeof buf);
        li_drop(&v);
        li_text(n, "%s = %s", ref, buf);
}

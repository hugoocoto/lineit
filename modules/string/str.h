/* Shared by the string modules. Inputs that take text take any value and
 * use it as text ("42", "true", "" when nothing is plugged in), so numbers
 * can be joined to strings. Results are strings owned by the square (each
 * square has its own copy of these globals), valid until it computes the
 * next one. Lengths and indices count bytes. */
#include "lineit.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* V as text: its string, or written in BUF */
static inline const char *
text(Li_Value v, char *buf, int size)
{
        if (v.type == LI_STRING) return v.s ? v.s : "";
        if (v.type == LI_NONE) return "";
        li_format(v, buf, size);
        return buf;
}

/* An index or length from any number, DEF if it isn't one (or nothing is
 * typed) */
static inline long
as_long(Li_Value v, long def)
{
        return v.type == LI_INT ? v.i : v.type == LI_FLOAT ? (long) v.f :
                                        v.type == LI_BOOL  ? v.b :
                                                             def;
}

static char *result;

/* Take S (malloc'd, or NULL for no value) as OUTPUT and show it */
static inline void
set_result(Li_Node *n, int output, char *s)
{
        free(result);
        result = s;
        if (!s) {
                li_set(n, output, (Li_Value) { .type = LI_NONE });
                li_text(n, "-");
                return;
        }
        li_set(n, output, li_string(s));
        li_text(n, "\"%s\"", s);
}

void
module_destroy(Li_Node *n)
{
        (void) n;
        free(result);
        result = NULL;
}

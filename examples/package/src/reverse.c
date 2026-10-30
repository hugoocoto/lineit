/* text = s backwards (bytes). A module made to be packaged: it only needs
 * lineit.h, see the Makefile of the package. Pure: computed when read, no ticks
 * needed. */
#include "lineit.h"
#include <stdlib.h>
#include <string.h>

static char *result;

void
module_init(Li_Node *n)
{
        li_input_value(n, "s", LI_ANY, li_string(""));
        li_output(n, "text", LI_STRING);
}

void
module_eval(Li_Node *n)
{
        char buf[64];
        Li_Value v    = li_get(n, 0);
        const char *s = v.type == LI_STRING ? (v.s ? v.s : "") : v.type == LI_NONE ? "" : (li_format(v, buf, sizeof buf), buf);
        size_t len    = strlen(s);

        free(result);
        result = malloc(len + 1);
        for (size_t i = 0; i < len; i++)
                result[i] = s[len - 1 - i];
        result[len] = '\0';
        li_set(n, 0, li_string(result));
        li_text(n, "\"%s\"", result);
}

void
module_draw(Li_Node *n)
{
        module_eval(n);
}

void
module_destroy(Li_Node *n)
{
        (void) n;
        free(result);
        result = NULL;
}

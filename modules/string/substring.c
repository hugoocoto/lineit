/* part = "length" bytes of s from "start". A negative start counts from
 * the end (-1 is the last byte), a negative length takes the rest. Out of
 * range parts are cut to s. Pure: computed when read, no ticks needed. */
#include "str.h"

enum { IN_S,
       IN_START,
       IN_LENGTH };

void
module_init(Li_Node *n)
{
        li_input_value(n, "s", LI_ANY, li_string(""));
        li_input_value(n, "start", LI_ANY, li_int(0));
        li_input_value(n, "length", LI_ANY, li_int(-1));
        li_output(n, "part", LI_STRING);
}

void
module_eval(Li_Node *n)
{
        char buf[64];
        const char *s = text(li_get(n, IN_S), buf, sizeof buf);
        long size     = (long) strlen(s);
        long start    = as_long(li_get(n, IN_START), 0);
        long len      = as_long(li_get(n, IN_LENGTH), -1);

        if (start < 0) start += size;
        if (start < 0) start = 0;
        if (start > size) start = size;
        if (len < 0 || len > size - start) len = size - start;
        set_result(n, 0, strndup(s + start, len));
}

void
module_draw(Li_Node *n)
{
        module_eval(n);
}

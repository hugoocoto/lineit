/* text = "template" with each {} replaced by the next of a, b and c, in
 * order: "n = {}" and a = 5 gives "n = 5". {{ is a {. Extra {} are left
 * empty. Pure: computed when read, no ticks needed. */
#include "str.h"

enum { IN_TEMPLATE,
       IN_A,
       IN_B,
       IN_C };

void
module_init(Li_Node *n)
{
        li_input_value(n, "template", LI_STRING, li_string("{}"));
        li_input_value(n, "a", LI_ANY, li_string(""));
        li_input_value(n, "b", LI_ANY, li_string(""));
        li_input_value(n, "c", LI_ANY, li_string(""));
        li_output(n, "text", LI_STRING);
}

void
module_eval(Li_Node *n)
{
        char tb[64], buf[64];
        const char *t = text(li_get(n, IN_TEMPLATE), tb, sizeof tb);
        int next      = IN_A;
        char *s;
        size_t len;
        FILE *f = open_memstream(&s, &len);

        for (; *t; t++) {
                if (t[0] == '{' && t[1] == '{') {
                        fputc('{', f), t++;
                } else if (t[0] == '{' && t[1] == '}') {
                        if (next <= IN_C) fputs(text(li_get(n, next++), buf, sizeof buf), f);
                        t++;
                } else {
                        fputc(*t, f);
                }
        }
        fclose(f);
        set_result(n, 0, s);
}

void
module_draw(Li_Node *n)
{
        module_eval(n);
}

/* decls.c - declarations: var/var squares of one variable agree */
#include "editor.h"

/* Squares of var/var with the same name in a canvas (or global anywhere in
 * the tab) are the same variable: copies, so its wires can be short. They
 * have to say the same type and start. The ones that don't are marked
 * (conflict, drawn as an error), and Run and Compile refuse until they do. */

enum { DECL_TYPE,
       DECL_START }; /* inputs of var/var */
enum { DECL_NAME,
       DECL_GLOBAL }; /* its params */

typedef struct Decl {
        Square *square;
        const void *scope; /* NULL if global */
        const char *name;
        int type;
        char start[128];
} Decl;

typedef Da(Decl) DeclList;

bool
is_decl(Square *s)
{
        return !strcmp(s->module->name, "var/var") && s->inputs.count > DECL_START && s->params.count > DECL_GLOBAL;
}

/* The var/var of LIST, a canvas whose locals are LOCALS', and the ones of
 * the subgraphs in it */
static void
decls_collect(SquareList *list, const void *locals, DeclList *out)
{
        Da_foreach(sp, *list)
        {
                Square *s = *sp;
                if (s->graph) decls_collect(&s->graph->squares, s->graph, out);
                if (!is_decl(s)) continue;
                Decl d = {
                        .square = s,
                        .scope  = s->params.items[DECL_GLOBAL].value.b ? NULL : locals,
                        .name   = s->params.items[DECL_NAME].value.s,
                        .type   = (int) s->inputs.items[DECL_TYPE].value.i,
                };
                input_text(s, DECL_START, d.start, sizeof d.start, true);
                Da_append(out, d);
        }
}

/* Mark the ones of the canvas shown (and its subgraphs) that disagree.
 * Returns the name of one of them, NULL if they all agree. */
const char *
decls_check(void)
{
        static char bad[128];
        DeclList d = { 0 };
        bool any   = false;

        decls_collect(&squares, &squares, &d);
        Da_foreach(x, d)
        {
                x->square->conflict = false;
        }
        for (int i = 0; i < d.count; i++) {
                for (int j = i + 1; j < d.count; j++) {
                        Decl *a = &d.items[i], *b = &d.items[j];
                        if (a->scope != b->scope || strcmp(a->name, b->name)) continue;
                        if (a->type == b->type && !strcmp(a->start, b->start)) continue;
                        a->square->conflict = b->square->conflict = true;
                        snprintf(bad, sizeof bad, "%s%s", a->scope ? "" : "::", a->name);
                        any = true;
                }
        }
        free(d.items);
        return any ? bad : NULL;
}

/* Run starts from nothing: the variables of the canvas, of every subgraph
 * in it and the globals are emptied, and var/var sets them to their start
 * when they are first used */
static void
locals_clear(SquareList *list)
{
        Da_foreach(sp, *list)
        {
                Graph *g = (*sp)->graph;
                if (!g) continue;
                var_list_clear(&g->vars);
                locals_clear(&g->squares);
        }
}

void
vars_reset(void)
{
        vars_clear();
        locals_clear(&squares);
}

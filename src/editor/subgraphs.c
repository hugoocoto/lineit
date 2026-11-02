/* subgraphs.c - subgraphs: canvases used as modules, and their ports */
#include "editor.h"

/* Trade the locals of G with the ones in the globals */
static void
locals_swap(Graph *g)
{
        VarList v = vars;
        vars      = g->vars;
        g->vars   = v;
}

void
graph_swap(Graph *g)
{
        SquareList s = squares;
        LinkList l   = links;
        squares      = g->squares;
        links        = g->links;
        g->squares   = s;
        g->links     = l;
        locals_swap(g);
}

/* Whose locals the thread uses: NULL for the canvas's, or a subgraph
 * square's while one of its squares runs. Each thread its own: branches run
 * in parallel. */
_Thread_local Graph *scope;

/* Use the locals of G (NULL: the canvas's). Returns whose it used, to give
 * back when done. */
Graph *
scope_enter(Graph *g)
{
        Graph *was = scope;
        scope      = g;
        value_locals(g ? &g->vars : NULL);
        return was;
}

/* Port squares: pick a type, and an input or output of that type */
const Li_Type port_types[] = { LI_TICK, LI_ANY, LI_BOOL, LI_INT, LI_FLOAT, LI_STRING };
const int port_types_count = countof(port_types);
#define PORT_OPTIONS "tick|any|bool|int|float|string"
enum { PORT_IN_TYPE }; /* inputs of subgraph/input */

static Li_Type
port_type(Square *s)
{
        Li_Value v = li_get(s, 0);
        return v.i >= 0 && v.i < countof(port_types) ? port_types[v.i] : LI_ANY;
}

static const char *
port_name(Square *s)
{
        return li_get_param(s, 0).s;
}

static void
port_in_init(Li_Node *n)
{
        li_input_options(n, "type", PORT_OPTIONS, 1);
        li_output(n, "value", LI_ANY);
        li_param(n, "name", li_string("in"));
}

static void
port_out_init(Li_Node *n)
{
        li_input_options(n, "type", PORT_OPTIONS, 1);
        li_input(n, "value", LI_ANY);
        li_param(n, "name", li_string("out"));
}

/* The type chosen goes to the pin */
static void
port_in_draw(Li_Node *n)
{
        li_output_type(n, 0, port_type(n));
        li_text(n, "%s", port_name(n));
}

static void
port_out_draw(Li_Node *n)
{
        PinInfo *p = pin_info(n, PART_INPUT, PORT_OUT_VALUE);
        if (p) p->type = port_type(n);
        li_text(n, "%s", port_name(n));
}

/* Nothing to run: the program follows the wires through them (see
 * Program). Outside of a subgraph there is nothing to read from them, and
 * ticks to them go nowhere. */
const ModuleFns port_in_fns  = { .init = port_in_init, .draw = port_in_draw };
const ModuleFns port_out_fns = { .init = port_out_init, .draw = port_out_draw };

static int
port_cmp(const void *a, const void *b)
{
        Vector2 p = (*(Square **) a)->pos, q = (*(Square **) b)->pos;
        if (p.y != q.y) return p.y < q.y ? -1 : 1;
        return (p.x > q.x) - (p.x < q.x);
}

/* A typed field to start with, by type */
static Li_Value
port_default(Li_Type type)
{
        switch (type) {
        case LI_BOOL: return li_bool(false);
        case LI_INT: return li_int(0);
        case LI_FLOAT: return li_float(0);
        case LI_STRING: return li_string("");
        default: return (Li_Value) { .type = LI_NONE };
        }
}

/* Subgraphs being made, outermost first: one can't be inside itself */
static Module *graph_making[16];
static int graph_depth;

static void
graph_init(Li_Node *n)
{
        Graph *g     = calloc(1, sizeof *g);
        Module *last = last_module;
        n->graph     = g;

        for (int i = 0; i < graph_depth; i++) {
                if (graph_making[i] == n->module) {
                        li_text(n, "inside itself");
                        return;
                }
        }
        if (graph_depth == countof(graph_making)) {
                li_text(n, "too deep");
                return;
        }

        graph_making[graph_depth++] = n->module;
        graph_swap(g);
        if (!canvas_read_file(n->module->path, READ_GRAPH)) li_text(n, "can't read it");
        Da_foreach(sp, squares)
        {
                Square *s = *sp;
                if (s->fn.draw == port_in_draw) Da_append(&g->inputs, s);
                if (s->fn.draw == port_out_draw) Da_append(&g->outputs, s);
                if (s->fn.draw == port_in_draw || s->fn.draw == port_out_draw) s->fn.draw(s);
        }
        graph_swap(g);
        graph_depth--;
        last_module = last; /* not the ones inside */

        if (g->inputs.count) qsort(g->inputs.items, g->inputs.count, sizeof *g->inputs.items, port_cmp);
        if (g->outputs.count) qsort(g->outputs.items, g->outputs.count, sizeof *g->outputs.items, port_cmp);
        for (int k = 0; k < g->inputs.count; k++) {
                Square *p    = g->inputs.items[k];
                Li_Type type = port_type(p);
                p->owner     = n;
                p->port      = k;
                if (type == LI_TICK)
                        li_input(n, port_name(p), type);
                else
                        li_input_value(n, port_name(p), type, port_default(type));
        }
        for (int k = 0; k < g->outputs.count; k++) {
                Square *p = g->outputs.items[k];
                p->owner  = n;
                p->port   = k;
                li_output(n, port_name(p), port_type(p));
        }
}

/* The squares inside update what they show too. Ticks, reads and frames
 * don't come here: the program has them inlined. */
static void
graph_draw(Li_Node *n)
{
        Graph *g = n->graph;
        Da_foreach(sp, g->squares)
        {
                Square *s = *sp;
                if (!s->fn.draw) continue;
                Graph *was = scope_enter(g);
                s->fn.draw(s);
                scope_enter(was);
        }
}

static void
graph_destroy(Li_Node *n)
{
        Graph *g = n->graph;
        graph_swap(g);
        Da_foreach(sp, squares)
        {
                square_free(*sp);
        }
        Da_destroy(&squares);
        Da_destroy(&links);
        var_list_clear(&vars);
        graph_swap(g);
        Da_destroy(&g->inputs);
        Da_destroy(&g->outputs);
        free(g);
        n->graph = NULL;
}

const ModuleFns graph_fns = {
        .init    = graph_init,
        .draw    = graph_draw,
        .destroy = graph_destroy,
};

/* subgraph/input and subgraph/output, in the menu with the modules */
void
graph_builtins(void)
{
        static const struct {
                const char *name;
                const ModuleFns *fn;
        } builtins[] = { { "subgraph/input", &port_in_fns }, { "subgraph/output", &port_out_fns } };

        for (int i = 0; i < countof(builtins); i++) {
                if (module_find(builtins[i].name)) continue;
                Module *m  = calloc(1, sizeof *m);
                m->name    = strdup(builtins[i].name);
                m->label   = strrchr(m->name, '/') + 1;
                m->builtin = builtins[i].fn;
                m->seen    = true;
                Da_append(&modules, m);
        }
}

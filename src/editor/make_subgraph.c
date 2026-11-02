/* make_subgraph.c - Make subgraph: the selection becomes a module */
#include "editor.h"

/* Like "extract function": the selected squares and the wires between them
 * go to a new subgraph (a .lua under modules.user, one of the modules
 * dirs, so it shows up now), and one square of it takes their place.
 * Each output outside that feeds the selection becomes a subgraph/input, and
 * each output inside that is used outside a subgraph/output, of the type of
 * that output, so the wires around can be plugged to the new square as
 * they were. One output feeding several squares inside is one input. */

typedef struct Port {
        int sq, pin; /* the output: outside for inputs, inside for outputs */
        char name[64];
        Li_Type type;
        float y;      /* to keep them in the order they were, top to bottom */
        Square *node; /* its subgraph/input or subgraph/output in the subgraph */
} Port;

typedef Da(Port) PortList;

/* The port of output SQ.PIN, added if new */
static Port *
port_for(PortList *l, int sq, int pin)
{
        Da_foreach(p, *l)
        {
                if (p->sq == sq && p->pin == pin) return p;
        }
        Square *s  = square_get(sq);
        PinInfo *o = pin_info(s, PART_OUTPUT, pin);
        Port p     = { .sq = sq, .pin = pin, .type = o ? o->type : LI_ANY, .y = 1e9 };

        /* Named after the output, and unique among the ports of this side */
        const char *base = o && *o->name ? o->name : "value";
        snprintf(p.name, sizeof p.name, "%s", base);
        for (int k = 2;; k++) {
                bool used = false;
                Da_foreach(q, *l)
                {
                        used = used || !strcmp(q->name, p.name);
                }
                if (!used) break;
                snprintf(p.name, sizeof p.name, "%s_%d", base, k);
        }
        Da_append(l, p);
        return &l->items[l->count - 1];
}

/* A subgraph/input or subgraph/output square for P at X, in the canvas there is */
static Square *
port_node(const char *module, Port *p, float x)
{
        Module *m = module_find(module);
        Square *s = m ? square_new(m, (Vector2) { x, p->y }) : NULL;
        if (!s) return NULL;
        param_set_text(s, 0, p->name);
        int option = 1; /* any: what the type isn't one of these */
        for (int i = 0; i < port_types_count; i++)
                if (port_types[i] == p->type) option = i;
        s->inputs.items[0].value = li_int(option);
        s->fn.draw(s); /* the pin takes the type */
        return s;
}

static int
port_y_cmp(const void *a, const void *b)
{
        float x = ((Port *) a)->y, y = ((Port *) b)->y;
        return (x > y) - (x < y);
}

/* Spread the ports of L at least a square apart, in order */
static void
ports_spread(PortList *l)
{
        if (l->count) qsort(l->items, l->count, sizeof *l->items, port_y_cmp);
        for (int i = 1; i < l->count; i++)
                l->items[i].y = fmaxf(l->items[i].y, l->items[i - 1].y + cfg.canvas.square_size * 1.5f);
}

/* Write the subgraph of the selection to PATH. INS and OUTS are its ports. */
static bool
subgraph_write(const char *path, PortList *ins, PortList *outs, Rectangle box)
{
        char *text     = canvas_text(WRITE_SELECTED | WRITE_STATE);
        Graph g        = { 0 };
        Camera2D outer = camera;
        bool ok;

        /* Made in a canvas of its own, like the ones inside subgraphs */
        graph_swap(&g);
        canvas_read_text(text, READ_GRAPH);
        free(text);
        float gap = cfg.canvas.square_size * 2.5f;
        Da_foreach(p, *ins)
        {
                p->node = port_node("subgraph/input", p, box.x - gap);
        }
        Da_foreach(p, *outs)
        {
                p->node = port_node("subgraph/output", p, box.x + box.width + gap);
        }
        /* The wires inside to and from them: the ids are the same here */
        LinkList outside = g.links; /* graph_swap put the canvas's there */
        Da_foreach(k, outside)
        {
                Square *a = square_get(k->from), *b = square_get(k->to);
                Da_foreach(p, *ins)
                {
                        if (p->node && !a && b && k->from == p->sq && k->from_pin == p->pin) link_add(p->node, 0, b, k->to_pin);
                }
                Da_foreach(p, *outs)
                {
                        if (p->node && a && !b && k->from == p->sq && k->from_pin == p->pin)
                                link_add(a, k->from_pin, p->node, PORT_OUT_VALUE);
                }
        }
        /* Ports may be linked once each, but several outside squares can use one output */
        for (int i = 0; i < links.count; i++)
                for (int j = links.count - 1; j > i; j--)
                        if (!memcmp(&links.items[i], &links.items[j], offsetof(Link, flash))) Da_remove(&links, j);

        camera  = (Camera2D) { .target = { box.x + box.width / 2, box.y + box.height / 2 }, .offset = { cfg.window.width / 2.0f, cfg.window.height / 2.0f }, .zoom = 1 };
        FILE *f = fopen(path, "w");
        ok      = f != NULL;
        if (f) {
                fprintf(f, "-- lineit canvas, a subgraph made from a selection\n");
                canvas_write(f, WRITE_CAMERA | WRITE_STATE);
                ok = fclose(f) == 0;
        }
        camera = outer;

        Da_foreach(sp, squares)
        {
                square_free(*sp);
        }
        Da_destroy(&squares);
        Da_destroy(&links);
        var_list_clear(&vars);
        graph_swap(&g);
        return ok;
}

void
subgraph_make(const char *typed)
{
        char path[PATH_MAX], name[PATH_MAX];
        const char *src = cfg.modules.user;
        size_t slen     = strlen(src);
        PortList ins = { 0 }, outs = { 0 };

        snprintf(path, sizeof path, "%s%s", typed, IsFileExtension(typed, ".lua") ? "" : ".lua");
        if (strncmp(path, src, slen) || path[slen] != '/') {
                set_status("Subgraphs go in %s/ (modules.user in the config)", src);
                return;
        }
        if (FileExists(path)) {
                set_status("%s already exists", path);
                return;
        }
        /* "modules/my/thing.lua" -> "my/thing" */
        snprintf(name, sizeof name, "%s", path + slen + 1);
        *strrchr(name, '.') = '\0';
        if (module_find(name)) {
                set_status("There is a module %s already", name);
                return;
        }

        /* Where it is, and the wires that cross it */
        Vector2 lo = { 1e9, 1e9 }, hi = { -1e9, -1e9 };
        Da_foreach(sp, squares)
        {
                if (!(*sp)->selected) continue;
                Rectangle r = square_rect(*sp);
                lo          = Vector2Min(lo, (Vector2) { r.x, r.y });
                hi          = Vector2Max(hi, (Vector2) { r.x + r.width, r.y + r.height });
        }
        Rectangle box = { lo.x, lo.y, hi.x - lo.x, hi.y - lo.y };

        Da_foreach(k, links)
        {
                Square *a = square_get(k->from), *b = square_get(k->to);
                if (!a || !b || a->selected == b->selected) continue;
                Port *p = b->selected ? port_for(&ins, k->from, k->from_pin) : port_for(&outs, k->from, k->from_pin);
                /* In the order of what they plug into inside */
                Square *in = b->selected ? b : a;
                p->y       = fminf(p->y, in->pos.y);
        }
        ports_spread(&ins);
        ports_spread(&outs);

        mkdir_parents(path);
        if (!subgraph_write(path, &ins, &outs, box)) {
                set_status("Could not write %s", path);
                goto done;
        }
        modules_scan(false); /* now, not when it settles */
        Module *m = module_find(name);
        if (!m) {
                set_status("Wrote %s, but it doesn't load", path);
                goto done;
        }

        /* Its square where the selection was, plugged as it was */
        LinkList old = { 0 };
        Da_foreach(k, links)
        {
                Da_append(&old, *k);
        }
        Vector2 center = { box.x + box.width / 2, box.y + box.height / 2 };
        each_selected(square_delete);
        Square *s = square_new(m, center);
        if (s) {
                Da_foreach(k, old)
                {
                        Square *a = square_get(k->from), *b = square_get(k->to);
                        Da_foreach(p, ins)
                        {
                                if (k->from == p->sq && k->from_pin == p->pin && a && !b) link_add(a, k->from_pin, s, pin_named(&s->inputs, p->name));
                        }
                        Da_foreach(p, outs)
                        {
                                if (k->from == p->sq && k->from_pin == p->pin && !a && b) link_add(s, pin_named(&s->outputs, p->name), b, k->to_pin);
                        }
                }
                /* Several inside squares fed by one output: one wire now */
                for (int i = 0; i < links.count; i++)
                        for (int j = links.count - 1; j > i; j--)
                                if (!memcmp(&links.items[i], &links.items[j], offsetof(Link, flash))) Da_remove(&links, j);
                select_all(false);
                s->selected = true;
        }
        Da_destroy(&old);
        set_status("Made %s, in %s", name, path);
done:
        Da_destroy(&ins);
        Da_destroy(&outs);
}

void
subgraph_ask(void)
{
        if (!selected_count()) return;
        prompt_open("Subgraph", TextFormat("%s/", cfg.modules.user), subgraph_make);
}

void
menu_subgraph(int id, void *arg)
{
        (void) id, (void) arg;
        subgraph_ask();
}

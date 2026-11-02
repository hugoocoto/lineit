/* program.c - the program: the canvas lowered, and what follows its wires (li_get, li_tick, branches) */
#include "editor.h"

Program prog; /* of the tab shown: tab_swap trades it */

void
program_free(Program *p)
{
        Da_destroy(&p->nodes);
        Da_destroy(&p->sources);
        Da_destroy(&p->outs);
        Da_destroy(&p->targets);
        Da_destroy(&p->wires);
        *p = (Program) { 0 };
}

/* S in the program, NULL if it isn't in it (a subgraph or a port square, or
 * squares changed since it was lowered) */
static Node *
prog_node(Square *s)
{
        if (prog.version != squares_version || s->ir < 0 || s->ir >= prog.nodes.count) return NULL;
        Node *x = &prog.nodes.items[s->ir];
        return x->square == s ? x : NULL;
}

LevelsDa levels;
LowerPathDa lower_path; /* wires followed so far, while lowering */

/* Following ports deeper than this is a wire from a subgraph to itself */
#define LOWER_DEPTH 64

static bool
is_port(Square *s)
{
        return s->module->builtin == &port_in_fns || s->module->builtin == &port_out_fns;
}

static Square *
level_square(Level *l, int id)
{
        Da_foreach(sp, *l->squares)
        {
                if ((*sp)->id == id) return *sp;
        }
        return NULL;
}

static int
level_of(Square *owner)
{
        for (int i = 0; i < levels.count; i++)
                if (levels.items[i].owner == owner) return i;
        return -1;
}

/* Every square of level LV that runs is a node, with the ones inside its
 * subgraphs where they are */
/* Top to bottom, then left to right: the order of the squares in the
 * program, so the one starts go in (and fan-outs, see lower_targets) is the
 * one you see, not the order they were drawn in */
static int
square_order(const void *a, const void *b)
{
        const Square *x = *(Square *const *) a, *y = *(Square *const *) b;
        if (x->pos.y != y->pos.y) return x->pos.y < y->pos.y ? -1 : 1;
        if (x->pos.x != y->pos.x) return x->pos.x < y->pos.x ? -1 : 1;
        return x->id - y->id;
}

static void
lower_squares(int lv)
{
        SquareList *list = levels.items[lv].squares;
        Graph *graph     = levels.items[lv].graph;
        assert(list->count >= 0);
        Square *sorted[list->count + 1];
        memcpy(sorted, list->items, list->count * sizeof *sorted);
        qsort(sorted, list->count, sizeof *sorted, square_order);
        for (int i = 0; i < list->count; i++) {
                Square *s = sorted[i];
                s->ir     = -1;
                if (s->graph) {
                        Da_append(&levels, ((Level) { &s->graph->squares, &s->graph->links, s->graph, s, lv }));
                        lower_squares(levels.count - 1);
                } else if (!is_port(s)) {
                        s->ir = prog.nodes.count;
                        Da_append(&prog.nodes, ((Node) { .square = s, .scope = graph, .level = lv }));
                }
        }
}

static void
path_push(LinkList *list, int link, Square *to)
{
        Da_append(&lower_path, ((Wire) { list, link, to }));
}

/* Where input INPUT of S, in level LV, gets its value. Follows wires
 * through ports, with them in `lower_path`. */
static Source
lower_source(int lv, Square *s, int input, int depth)
{
        Level *l    = &levels.items[lv];
        PinInfo *in = pin_info(s, PART_INPUT, input);
        Source none = { .kind = SRC_NONE };
        int k       = -1;

        if (!in || in->kind == PIN_OPTIONS || depth > LOWER_DEPTH) return none;
        for (int i = l->links->count - 1; i >= 0 && k < 0; i--) {
                Link *e = &l->links->items[i];
                if (e->to == s->id && e->to_pin == input) k = i;
        }
        if (k < 0) return in->kind == PIN_VALUE ? (Source) { .kind = SRC_TYPED, .square = s, .pin = input } : none;

        Link e       = l->links->items[k];
        Square *from = level_square(l, e.from);
        PinInfo *out = from ? pin_info(from, PART_OUTPUT, e.from_pin) : NULL;
        if (!out || !types_compatible(out->type, in->type)) return none;

        path_push(l->links, k, NULL);
        if (from->graph) {
                Graph *g = from->graph;
                if (e.from_pin >= g->outputs.count) return none;
                return lower_source(level_of(from), g->outputs.items[e.from_pin], PORT_OUT_VALUE, depth + 1);
        }
        if (from->module->builtin == &port_in_fns)
                return l->owner ? lower_source(l->up, l->owner, from->port, depth + 1) : none;
        return (Source) { .kind = SRC_OUTPUT, .node = from->ir, .pin = e.from_pin };
}

/* The inputs that tick output OUTPUT of S, in level LV, ticks, through
 * ports, added to prog.targets */
static void
lower_targets(int lv, Square *s, int output, int depth)
{
        Level *l     = &levels.items[lv];
        PinInfo *out = pin_info(s, PART_OUTPUT, output);
        if (!out || depth > LOWER_DEPTH) return;

        /* Its wires by where they go, top to bottom: the order of the
         * branches of a fan-out */
        assert(l->links->count >= 0);
        int order[l->links->count + 1], n = 0;
        for (int k = 0; k < l->links->count; k++) {
                Link e = l->links->items[k];
                if (e.from == s->id && e.from_pin == output) order[n++] = k;
        }
        for (int i = 1; i < n; i++) {
                for (int j = i; j > 0; j--) {
                        Link a = l->links->items[order[j - 1]], b = l->links->items[order[j]];
                        Square *sa = level_square(l, a.to), *sb = level_square(l, b.to);
                        if (!sa || !sb) break;
                        Vector2 pa = pin_pos(sa, PART_INPUT, a.to_pin), pb = pin_pos(sb, PART_INPUT, b.to_pin);
                        if (pa.y < pb.y || (pa.y == pb.y && pa.x <= pb.x)) break;
                        int t        = order[j];
                        order[j]     = order[j - 1];
                        order[j - 1] = t;
                }
        }

        for (int i = 0; i < n; i++) {
                int k       = order[i];
                Link e      = l->links->items[k];
                Square *to  = level_square(l, e.to);
                PinInfo *in = to ? pin_info(to, PART_INPUT, e.to_pin) : NULL;
                if (!in || !types_compatible(out->type, in->type)) continue;

                int mark = lower_path.count;
                path_push(l->links, k, to);
                if (to->graph) {
                        if (e.to_pin < to->graph->inputs.count)
                                lower_targets(level_of(to), to->graph->inputs.items[e.to_pin], 0, depth + 1);
                } else if (to->module->builtin == &port_out_fns) {
                        if (e.to_pin == PORT_OUT_VALUE && l->owner) lower_targets(l->up, l->owner, to->port, depth + 1);
                } else {
                        Target t = { .node = to->ir, .pin = e.to_pin, .wires = prog.wires.count, .wire_count = lower_path.count };
                        Da_foreach(w, lower_path)
                        {
                                Da_append(&prog.wires, *w);
                        }
                        Da_append(&prog.targets, t);
                }
                lower_path.count = mark;
        }
}

/* The canvas shown, to prog */
void
program_lower(void)
{
        prog.nodes.count   = 0;
        prog.sources.count = 0;
        prog.outs.count    = 0;
        prog.targets.count = 0;
        prog.wires.count   = 0;
        levels.count       = 0;
        Da_append(&levels, ((Level) { &squares, &links, NULL, NULL, -1 }));
        lower_squares(0);

        for (int i = 0; i < prog.nodes.count; i++) {
                Node *x   = &prog.nodes.items[i];
                Square *s = x->square;

                x->inputs = prog.sources.count;
                for (int k = 0; k < s->inputs.count; k++) {
                        lower_path.count = 0;
                        Source src       = lower_source(x->level, s, k, 0);
                        src.wires        = prog.wires.count;
                        src.wire_count   = lower_path.count;
                        Da_foreach(w, lower_path)
                        {
                                Da_append(&prog.wires, *w);
                        }
                        Da_append(&prog.sources, src);
                }

                x->outputs = prog.outs.count;
                for (int k = 0; k < s->outputs.count; k++) {
                        Out o            = { .first = prog.targets.count };
                        lower_path.count = 0;
                        if (s->outputs.items[k].type == LI_TICK) lower_targets(x->level, s, k, 0);
                        o.count = prog.targets.count - o.first;
                        Da_append(&prog.outs, o);
                }
        }

        /* Merges: tick inputs more than one output ticks */
        Da_foreach(x, prog.nodes)
        {
                x->merge = 0;
        }
        Da_foreach(t, prog.targets)
        {
                unsigned bit = t->pin < 32 ? 1u << t->pin : 0;
                Node *x      = &prog.nodes.items[t->node];
                for (Target *u = prog.targets.items; u < t && !(x->merge & bit); u++)
                        if (u->node == t->node && u->pin == t->pin) x->merge |= bit;
        }
        prog.version = squares_version;
}

static void
wires_glow(int first, int count, double now)
{
        static pthread_mutex_t lock = PTHREAD_MUTEX_INITIALIZER; /* branches glow them from their threads */
        pthread_mutex_lock(&lock);
        for (int i = first; i < first + count; i++) {
                Wire w = prog.wires.items[i];
                if (w.link < w.list->count) w.list->items[w.link].flash = now;
                if (w.to) w.to->flash = now;
        }
        pthread_mutex_unlock(&lock);
}

/* How deep in a tick chain we are, 0 outside of one */
static _Thread_local int tick_depth; /* of the branch the thread runs */

/* What li_get gave in this thread: copies, so a branch reading an output
 * another one changes doesn't see it go. Freed once the square that asked
 * is done (got_free with what got_mark said before it ran). */
static _Thread_local Da(Li_Value) got_values;

int
got_mark(void)
{
        return got_values.count;
}

void
got_free(int mark)
{
        while (got_values.count > mark)
                value_drop(got_values.items[--got_values.count]);
}

Li_Value
li_get(Li_Node *n, int input)
{
        Li_Value none = { .type = LI_NONE };
        PinInfo *in   = pin_info(n, PART_INPUT, input);
        if (!in) return none;
        if (in->kind == PIN_OPTIONS) return in->value;

        /* Not lowered yet (it's being made or loaded): no wires */
        Node *x = prog_node(n);
        if (!x) return in->kind == PIN_VALUE ? in->value : none;

        Source src = prog.sources.items[x->inputs + input];
        /* Data read while ticking glows too (not what's drawn every frame) */
        if (tick_depth > 0) wires_glow(src.wires, src.wire_count, GetTime());

        switch (src.kind) {
        case SRC_TYPED: {
                PinInfo *p = pin_info(src.square, PART_INPUT, src.pin);
                return p && p->kind == PIN_VALUE ? p->value : none;
        }
        case SRC_OUTPUT: {
                Node *from = &prog.nodes.items[src.node];
                Square *s  = from->square;

                /* The square is ours while we read it: it may be computing it
                 * for another branch. Pure squares compute their outputs when
                 * someone reads them. */
                sched_lock(sched_get(), src.node);
                /* Its type may have changed since it was lowered: a pure one
                 * says it again while it computes, so with the lock */
                PinInfo *out = pin_info(s, PART_OUTPUT, src.pin);
                if (!out || !types_compatible(out->type, in->type)) {
                        sched_unlock(sched_get(), src.node);
                        return none;
                }
                if (s->fn.eval && !s->evaluating) {
                        int mark      = got_mark();
                        Graph *was    = scope_enter(from->scope);
                        s->evaluating = true;
                        s->fn.eval(s);
                        s->evaluating = false;
                        scope_enter(was);
                        got_free(mark);
                }
                Li_Value v = value_dup(pin_info(s, PART_OUTPUT, src.pin)->value);
                sched_unlock(sched_get(), src.node);
                Da_append(&got_values, v);
                return value_widen(v, in->type); /* see types_compatible */
        }
        default: return none;
        }
}

/* Tick chains are plain recursion, this stops loops in the graph */
#define TICK_DEPTH_MAX 256

/* The branches of the tab shown (see branches.h): every tab has its own, so
 * one can stop stepping while others run */
Sched *sched;

/* A tick from node FROM can get to node TO, through one wire or more */
static bool
prog_reach(void *ctx, int from, int to)
{
        (void) ctx;
        static Da(char) seen;
        static Da(int) todo;
        seen.count = todo.count = 0;
        for (int i = 0; i < prog.nodes.count; i++)
                Da_append(&seen, 0);
        Da_append(&todo, from);
        while (todo.count) {
                Node *x = &prog.nodes.items[todo.items[--todo.count]];
                for (int k = 0; k < x->square->outputs.count; k++) {
                        Out o = prog.outs.items[x->outputs + k];
                        for (int i = o.first; i < o.first + o.count; i++) {
                                int t = prog.targets.items[i].node;
                                if (t == to) return true;
                                if (seen.items[t]) continue;
                                seen.items[t] = 1;
                                Da_append(&todo, t);
                        }
                }
        }
        return false;
}

Sched *
sched_get(void)
{
        if (!sched) sched = sched_new((Sched_Hooks) { .reach = prog_reach });
        return sched;
}

/* Tick target T: wait at it if it's a merge, make its square ours, run it */
static void
tick_target(Target t)
{
        Node *to  = &prog.nodes.items[t.node];
        Square *s = to->square;
        wires_glow(t.wires, t.wire_count, GetTime());
        if (!s->fn.tick) return;
        if (t.pin < 32 && (to->merge & 1u << t.pin) && !sched_merge(sched_get(), t.node, t.pin)) return;
        int at = sched_at(sched_get(), t.node);
        /* Stopped (Stop): the rest of the branches end without running */
        if (!sched_aborted(sched_get())) {
                int mark   = got_mark();
                Graph *was = scope_enter(to->scope);
                s->fn.tick(s, t.pin);
                scope_enter(was);
                got_free(mark);
        }
        sched_back(sched_get(), at);
}

/* A target of a fan-out, run as a branch: as deep as the tick that forked
 * it, on whatever thread runs it, so a loop through a fan-out stops too */
typedef struct Forked {
        Target target;
        int depth;
} Forked;

static void
tick_task(void *arg)
{
        Forked *f  = arg;
        int was    = tick_depth;
        tick_depth = f->depth;
        tick_target(f->target);
        tick_depth = was;
}

void
li_tick(Li_Node *n, int output)
{
        static atomic_bool warned; /* branches tick from their threads */

        if (output >= 0 && output < (int) countof(n->fired)) n->fired[output] = GetTime();
        if (tick_depth >= TICK_DEPTH_MAX) {
                if (!atomic_exchange(&warned, true))
                        fprintf(stderr, "tick: more than %d squares deep, is there a loop?\n", TICK_DEPTH_MAX);
                return;
        }
        Node *x = prog_node(n);
        if (!x || !pin_info(n, PART_OUTPUT, output)) return;

        Out o = prog.outs.items[x->outputs + output];
        tick_depth++;
        if (o.count == 1) {
                tick_target(prog.targets.items[o.first]);
        } else if (o.count > 1) {
                /* A fan-out: a branch each, back when they're all done */
                assert(o.count > 0);
                Forked fs[o.count];
                Sched_Task tasks[o.count];
                for (int i = 0; i < o.count; i++) {
                        fs[i]    = (Forked) { prog.targets.items[o.first + i], tick_depth };
                        tasks[i] = (Sched_Task) { tick_task, &fs[i], fs[i].target.node, fs[i].target.pin };
                }
                sched_fork(sched_get(), tasks, o.count);
        }
        tick_depth--;
}

/* A start (or a clock's frame) is a square that runs too: a step */
static void
start_task(void *arg)
{
        Node *x = &prog.nodes.items[(intptr_t) arg];
        int at  = sched_at(sched_get(), (int) (intptr_t) arg);
        if (!sched_aborted(sched_get())) {
                int mark   = got_mark();
                Graph *was = scope_enter(x->scope);
                x->square->fn.start(x->square);
                scope_enter(was);
                got_free(mark);
        }
        sched_back(sched_get(), at);
}

static void
frame_task(void *arg)
{
        Node *x = &prog.nodes.items[(intptr_t) arg];
        int at  = sched_at(sched_get(), (int) (intptr_t) arg);
        if (!sched_aborted(sched_get())) {
                int mark   = got_mark();
                Graph *was = scope_enter(x->scope);
                x->square->fn.frame(x->square);
                scope_enter(was);
                got_free(mark);
        }
        sched_back(sched_get(), at);
}

/* Its start sets it up (random seeds): it has a tick input, others start
 * it. Starts of squares nothing ticks start chains (entries). */
bool
square_is_ticked(Square *s)
{
        Da_foreach(p, s->inputs)
        {
                if (p->type == LI_TICK) return true;
        }
        return false;
}

/* module_frame of every node that has it, each a branch, in canvas order.
 * Or module_start: first the ones that set squares up, then the ones that
 * start chains, branches. True if it stopped stepping. */
bool
program_each(bool start)
{
        assert(prog.nodes.count >= 0);
        Sched_Task tasks[prog.nodes.count + 1];
        int n = 0;

        /* Variables used by name, not wired to their var/var, start as it
         * says too: every var/var says it before anything runs */
        for (int i = 0; start && i < prog.nodes.count; i++) {
                Node *x = &prog.nodes.items[i];
                if (!is_decl(x->square) || !x->square->fn.eval) continue;
                Graph *was = scope_enter(x->scope);
                x->square->fn.eval(x->square);
                scope_enter(was);
        }
        for (int i = 0; i < prog.nodes.count; i++) {
                Square *s = prog.nodes.items[i].square;
                if (start ? !s->fn.start : !s->fn.frame) continue;
                if (start && square_is_ticked(s)) {
                        Graph *was = scope_enter(prog.nodes.items[i].scope);
                        s->fn.start(s);
                        scope_enter(was);
                        continue;
                }
                tasks[n++] = (Sched_Task) { start ? start_task : frame_task, (void *) (intptr_t) i, i, -1 };
        }
        return n && sched_run(sched_get(), tasks, n);
}

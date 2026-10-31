/* rt.c - see rt.h */
#define _POSIX_C_SOURCE 200809L /* strdup, clock_gettime, nanosleep */
#include "rt.h"

#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

/* ---- Pins and params ---- */

static int
pin_add(Li_Node *n, bool input, Li_Type type)
{
        Rt_Pin p = { .type = type };
        if (input) {
                Da_append(&n->inputs, p);
                return n->inputs.count - 1;
        }
        Da_append(&n->outputs, p);
        return n->outputs.count - 1;
}

int
li_input(Li_Node *n, const char *name, Li_Type type)
{
        (void) name;
        return pin_add(n, true, type);
}

/* What's typed comes from main.c, as the function of the input */
int
li_input_value(Li_Node *n, const char *name, Li_Type type, Li_Value value)
{
        (void) name, (void) value;
        return pin_add(n, true, type);
}

int
li_input_options(Li_Node *n, const char *name, const char *options, int chosen)
{
        (void) name;
        int i      = pin_add(n, true, LI_INT);
        Rt_Pin *p  = &n->inputs.items[i];
        char *list = strdup(options ? options : ""), *save = NULL;
        for (char *o = strtok_r(list, "|", &save); o; o = strtok_r(NULL, "|", &save)) {
                p->options                    = realloc(p->options, (p->option_count + 1) * sizeof *p->options);
                p->options[p->option_count++] = strdup(o);
        }
        free(list);
        p->value = li_int(chosen);
        return i;
}

const char *
li_option(Li_Node *n, int input)
{
        if (input < 0 || input >= n->inputs.count) return "";
        Rt_Pin *p = &n->inputs.items[input];
        long i    = p->value.i;
        return i >= 0 && i < p->option_count ? p->options[i] : "";
}

int
li_output(Li_Node *n, const char *name, Li_Type type)
{
        (void) name;
        return pin_add(n, false, type);
}

void
rt_pins(Li_Node *n, int inputs, int outputs)
{
        for (int i = 0; i < inputs; i++)
                pin_add(n, true, LI_ANY);
        for (int i = 0; i < outputs; i++)
                pin_add(n, false, LI_ANY);
}

void
rt_option(Li_Node *n, int input, int chosen)
{
        if (input >= 0 && input < n->inputs.count) n->inputs.items[input].value = li_int(chosen);
}

Li_Value
li_get(Li_Node *n, int input)
{
        if (input < 0 || input >= n->inputs.count) return (Li_Value) { .type = LI_NONE };
        if (n->inputs.items[input].options) return n->inputs.items[input].value;
        if (input < n->get_count && n->get[input]) return n->get[input]();
        return (Li_Value) { .type = LI_NONE };
}

void
li_set(Li_Node *n, int output, Li_Value value)
{
        if (output >= 0 && output < n->outputs.count) n->outputs.items[output].value = value;
}

void
li_output_type(Li_Node *n, int output, Li_Type type)
{
        if (output >= 0 && output < n->outputs.count) n->outputs.items[output].type = type;
}

/* Tick chains are plain recursion, this stops loops in the graph */
#define TICK_DEPTH_MAX 256

void
li_tick(Li_Node *n, int output)
{
        static int depth;
        static bool warned;

        if (output < 0 || output >= n->tick_count || !n->tick[output]) return;
        if (depth >= TICK_DEPTH_MAX) {
                if (!warned) fprintf(stderr, "tick: more than %d squares deep, is there a loop?\n", TICK_DEPTH_MAX);
                warned = true;
                return;
        }
        depth++;
        n->tick[output]();
        depth--;
}

void *
li_data(Li_Node *n)
{
        return n->data;
}

void
li_set_data(Li_Node *n, void *data)
{
        n->data = data;
}

/* There's no square to show it in */
void
li_text(Li_Node *n, const char *fmt, ...)
{
        (void) n, (void) fmt;
}

void
li_print(Li_Node *n, const char *fmt, ...)
{
        va_list ap;
        (void) n;
        va_start(ap, fmt);
        vprintf(fmt, ap);
        va_end(ap);
        putchar('\n');
}

static bool fake_time;
static double now; /* fake_time: of this frame */
static struct timespec t0;

double
li_time(void)
{
        if (fake_time) return now;
        struct timespec t;
        clock_gettime(CLOCK_MONOTONIC, &t);
        return (double) (t.tv_sec - t0.tv_sec) + (t.tv_nsec - t0.tv_nsec) / 1e9;
}

static void
param_store(Rt_Param *p, Li_Value value)
{
        free(p->str);
        p->str   = NULL;
        p->value = value;
        if (value.type == LI_STRING) {
                p->str     = strdup(value.s ? value.s : "");
                p->value.s = p->str;
        }
}

int
li_param(Li_Node *n, const char *name, Li_Value value)
{
        if (value.type != LI_INT && value.type != LI_FLOAT && value.type != LI_BOOL && value.type != LI_STRING) {
                fprintf(stderr, "%s: param %s must be int, float, bool or string\n", n->module, name);
                return -1;
        }
        Da_append(&n->params, (Rt_Param) { 0 });
        param_store(&n->params.items[n->params.count - 1], value);
        return n->params.count - 1;
}

Li_Value
li_get_param(Li_Node *n, int param)
{
        if (param < 0 || param >= n->params.count) return (Li_Value) { .type = LI_NONE };
        return n->params.items[param].value;
}

void
rt_param(Li_Node *n, int param, Li_Value value)
{
        if (param >= 0 && param < n->params.count) param_store(&n->params.items[param], value);
}

/* ---- Scopes: the locals of each subgraph square, like the editor's ---- */

typedef struct Locals {
        VarList vars;
        BT *lists, *trees;
} Locals;

static Locals *locals; /* by scope: while it's in the globals, the canvas's (0) */
static int scope;

static void
locals_swap(int s)
{
        Locals *l = &locals[s];
        VarList v = vars;
        BT *ls = lists, *ts = trees;
        vars     = l->vars;
        lists    = l->lists;
        trees    = l->trees;
        l->vars  = v;
        l->lists = ls;
        l->trees = ts;
}

int
rt_scope(int s)
{
        int was = scope;
        if (s == scope) return was;
        if (scope) locals_swap(scope);
        if (s) locals_swap(s);
        scope = s;
        return was;
}

/* ---- Custom types ---- */

static const Rt_Type *
type_get(Li_Type t)
{
        int i = (int) t - LI_TYPE_COUNT;
        return i >= 0 && i < rt_type_count ? &rt_types[i] : NULL;
}

bool
type_is_record(Li_Type t)
{
        return type_get(t) != NULL;
}

const char *
type_field_name(Li_Type t, int field)
{
        const Rt_Type *d = type_get(t);
        return d && field >= 0 && field < d->field_count ? d->fields[field] : "?";
}

/* Like the editor's: made again only when a field changes, so what was
 * read from it lives until then */
void
rt_make_eval(Li_Node *n)
{
        const Rt_Type *d = type_get(n->type);
        if (!d) return;
        Record *r = malloc(sizeof *r + d->field_count * sizeof *r->fields);
        r->type   = n->type;
        r->count  = d->field_count;
        for (int i = 0; i < r->count; i++)
                r->fields[i] = value_dup(li_get(n, i));

        Li_Value made = { .type = n->type, .p = r };
        Li_Value old  = { .type = n->type, .p = n->data };
        if (n->data && value_equal(old, made)) {
                for (int i = 0; i < r->count; i++)
                        value_drop(r->fields[i]);
                free(r);
        } else {
                n->data = r;
                value_drop(old);
        }
        li_set(n, 0, (Li_Value) { .type = n->type, .p = n->data });
}

void
rt_break_eval(Li_Node *n)
{
        const Rt_Type *d = type_get(n->type);
        Li_Value v       = li_get(n, 0);
        Record *r        = v.type == n->type ? v.p : NULL;
        for (int i = 0; d && i < d->field_count; i++)
                li_set(n, i, r && i < r->count ? r->fields[i] : (Li_Value) { .type = LI_NONE });
}

/* ---- Running ---- */

enum { FPS = 60 };

int
rt_main(int argc, char **argv, void (*setup)(void), void (*start)(void), void (*frame)(void))
{
        long frames = -1;

        for (int i = 1; i < argc; i++) {
                if (!strcmp(argv[i], "--frames") && i + 1 < argc) {
                        frames    = strtol(argv[++i], NULL, 10);
                        fake_time = true;
                } else {
                        fprintf(stderr, "Usage: %s [--frames N]\n"
                                        "  --frames N  run N frames of fake time (%d per second) and quit\n",
                                argv[0], FPS);
                        return 2;
                }
        }

        clock_gettime(CLOCK_MONOTONIC, &t0);
        locals = calloc(rt_scope_count > 0 ? rt_scope_count : 1, sizeof *locals);
        setup();
        start();

        /* Nothing starts tick chains on its own: it's done */
        for (long i = 0; frame && (frames < 0 || i < frames); i++) {
                if (fake_time) {
                        now = (double) i / FPS;
                } else {
                        struct timespec wait = { 0, 1000000000L / FPS };
                        nanosleep(&wait, NULL);
                }
                frame();
        }
        fflush(stdout);
        return 0;
}

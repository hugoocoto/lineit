/* rt.h - what the programs Compile makes run on
 *
 * Compile (see Compile in src/main.c) writes a canvas as C: main.c, with
 * every square of it a node, N[i], and its wires as functions, and one
 * nI.c per node that has its module source in it, with the callbacks
 * renamed (module_tick is nI_tick). Each one is its own object, so each
 * square has its own globals, like the .so copies of the editor.
 *
 * This is the module API (lineit.h) for them, the same as the editor's but
 * without the editor: li_get calls the function main.c has for that input,
 * li_tick the one for that output. Values, variables, lists and trees are
 * the editor's own code (src/value.c). `make` builds it to build/rt, with
 * these headers. */

#ifndef RT_H_
#define RT_H_

#include "value.h"

typedef struct Rt_Pin {
        Li_Type type;
        Li_Value value;  /* outputs: the last value set. Options: .i, the chosen one */
        char **options;  /* li_input_options */
        int option_count;
} Rt_Pin;

typedef struct Rt_Param {
        Li_Value value; /* for LI_STRING, .s is str */
        char *str;
} Rt_Param;

struct Li_Node {
        /* Given by main.c */
        const char *module; /* "math/add" */
        int square;         /* its id in the canvas */
        int scope;          /* whose locals it uses: 0 the canvas's, then one per subgraph square */
        Li_Type type;       /* custom type of make_ and break_ squares */
        Li_Value (*const *get)(void); /* per input, its value (NULL: options) */
        int get_count;
        void (*const *tick)(void); /* per output, what it ticks (NULL: nothing) */
        int tick_count;

        /* Made by the module */
        Da(Rt_Pin) inputs, outputs;
        Da(Rt_Param) params;
        void *data;
        bool evaluating;
};

/* A custom type: its name and the names of its fields */
typedef struct Rt_Type {
        const char *name;
        int field_count;
        const char *const *fields;
} Rt_Type;

/* Given by main.c */
extern const Rt_Type rt_types[];
extern const int rt_type_count;
extern const int rt_scope_count;

/* Put the locals of scope S in the globals. Returns whose were there. */
int rt_scope(int s);

/* Output PIN of N, computed first if it's pure (EVAL), the way the editor
 * does when a wire is read */
static inline Li_Value
rt_pull(Li_Node *n, void (*eval)(Li_Node *), int pin)
{
        if (eval && !n->evaluating) {
                int was       = rt_scope(n->scope);
                n->evaluating = true;
                eval(n);
                n->evaluating = false;
                rt_scope(was);
        }
        return pin < n->outputs.count ? n->outputs.items[pin].value : (Li_Value) { .type = LI_NONE };
}

/* Call FN (module_tick, module_start...) of N in its scope */
static inline void
rt_call(Li_Node *n, void (*fn)(Li_Node *))
{
        int was = rt_scope(n->scope);
        fn(n);
        rt_scope(was);
}

static inline void
rt_call_tick(Li_Node *n, void (*tick)(Li_Node *, int), int input)
{
        int was = rt_scope(n->scope);
        tick(n, input);
        rt_scope(was);
}

/* Setting a node up, after its module_init */
void rt_pins(Li_Node *n, int inputs, int outputs); /* for built-in modules, that have no init */
void rt_option(Li_Node *n, int input, int chosen);
void rt_param(Li_Node *n, int param, Li_Value value);

/* make_ and break_ of custom types */
void rt_make_eval(Li_Node *n);
void rt_break_eval(Li_Node *n);

/* Runs the program: SETUP, START, then FRAME (if there is one) every frame
 * until killed. With --frames N, only N frames, of fake time at 60 per
 * second, as fast as it can (what `make test` does). */
int rt_main(int argc, char **argv, void (*setup)(void), void (*start)(void), void (*frame)(void));

#endif /* RT_H_ */

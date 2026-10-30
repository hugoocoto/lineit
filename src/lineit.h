/* lineit.h - module API

   A module is a C file in the modules/ directory. `make` builds each one to a
   shared object (.so) in build/modules/, with the same folders. Every .so
   found there is loaded at startup and shows up in the canvas menu, under
   "Add module", with the same folder structure.

   Each square on the canvas is one instance of a module, a Li_Node. Every
   square loads its own copy of the .so, so globals are per square: they can
   hold its state instead of li_set_data.

   A module defines some of these callbacks, all optional except module_init:

     void module_init(Li_Node *n);
        A new square was placed. Declare its pins with li_input and
        li_output, and allocate state with li_set_data if needed.

     void module_tick(Li_Node *n, int input);
        A tick reached tick input INPUT. Read inputs with li_get, set
        outputs with li_set, and activate the next squares with li_tick.

     void module_eval(Li_Node *n);
        Compute the outputs from the inputs. Called right before another
        square reads one of this square's outputs, so pure squares (math,
        constants...) don't need ticks.

     void module_frame(Li_Node *n);
        Called once per frame while running (Start in the bar). Use it to
        start tick chains (a clock).

     void module_start(Li_Node *n);
        Start was pressed in the bar. Called once, before the first
        module_frame. Use it to set things up or start tick chains (entry).

     void module_draw(Li_Node *n);
        Called once per frame, even when stopped, before drawing. Use it to
        update what the square shows (li_text), like the value of an input.

     void module_destroy(Li_Node *n);
        The square is being removed. Free its state.

     void module_param(Li_Node *n, int param);
        PARAM (see li_param) changed: the user set it in the settings panel
        (double click the square, or Configure in its menu), or a canvas was
        loaded. Also called for every param right after module_init, so
        defaults go through the same path. Read it with li_get_param.

     const char *module_save(Li_Node *n);
        The canvas is being saved. Return the square's state as a string in
        any format (NULL to save nothing). It's copied right away, so a
        static buffer is fine. Use it for state the user can't see in the
        pins or the text: counts, accumulators...

     void module_load(Li_Node *n, const char *state);
        The square was loaded from a saved canvas with STATE, what
        module_save returned. Called after module_init and after its params
        are restored (module_param).

   Wires go from an output to an input of the same type. LI_ANY matches any
   type except LI_TICK. A data input takes one wire, a tick input can take
   many. Build modules with `make`, see the Makefile.

   To give a module to others, with its source, license and builds for
   several platforms in one file, make a package (.li) with tools/lineit-pack.
   It goes in the modules dir like a .so.

   Minimal example, modules/io/print.c:

     #include "lineit.h"
     #include <stdio.h>

     void module_init(Li_Node *n) {
             li_input(n, "tick", LI_TICK);
             li_input(n, "value", LI_INT);
     }

     void module_tick(Li_Node *n, int input) {
             Li_Value v = li_get(n, 1);
             if (v.type == LI_INT) printf("%ld\n", v.i);
     }
*/

#ifndef LINEIT_H_
#define LINEIT_H_

#include <stdbool.h>

/* Changes when this API changes in a way that breaks modules built for an
 * older one. Packages (.li) say which one they were built for, and lineit
 * skips the ones that don't match. See tools/lineit-pack --howto. */
#define LI_API_VERSION 1

#ifdef __cplusplus
extern "C" {
#endif

typedef enum Li_Type {
        LI_NONE,   /* no value: unconnected input, or output never set */
        LI_TICK,   /* activation, carries no value */
        LI_ANY,    /* pin accepts any data type */
        LI_BOOL,   /* .b */
        LI_INT,    /* .i */
        LI_FLOAT,  /* .f */
        LI_STRING, /* .s, owned by the module that set it */
        LI_PTR,    /* .p, anything else, owned by the module that set it */
        LI_TYPE_COUNT,
        /* Custom types, records made in the editor (File > New type), come
           after these: LI_TYPE_COUNT and up. Their .p is the record, owned
           by the square that made it. Modules can pass them on (LI_ANY
           pins), keep them in variables, lists and maps (they're copied
           there) and show them with li_format: "{x: 1, y: 2}". */
} Li_Type;

/* What travels through a wire */
typedef struct Li_Value {
        Li_Type type;
        union {
                bool b;
                long i;
                double f;
                const char *s;
                void *p;
        };
} Li_Value;

/* A square on the canvas. Opaque, and only valid during the callback that
 * received it: don't keep it around. */
typedef struct Li_Node Li_Node;

/* Declare a pin. Returns its index, counting inputs and outputs separately
 * from 0 in the order they are declared. */
int li_input(Li_Node *n, const char *name, Li_Type type);
int li_output(Li_Node *n, const char *name, Li_Type type);

/* Inputs can be of three kinds, like Blueprint pins:
 *
 *   li_input          only a wire.
 *   li_input_value    a wire, or, while nothing is plugged in, a value the
 *                     user types in a field next to it (bools are a
 *                     checkbox). VALUE is the default. For LI_ANY inputs the
 *                     type comes from what is typed: 2 is an int, 2.5 a
 *                     float, true a bool, the rest a string.
 *   li_input_options  no wire: the user picks one of OPTIONS ("a|b|c") from
 *                     a list. li_get gives its index as an LI_INT, starting
 *                     with CHOSEN, and li_option its name.
 *
 * li_get works the same for all of them. Typed values and choices are saved
 * with the canvas. */
int li_input_value(Li_Node *n, const char *name, Li_Type type, Li_Value value);
int li_input_options(Li_Node *n, const char *name, const char *options, int chosen);
const char *li_option(Li_Node *n, int input);

/* Value on the wire plugged into INPUT, or what is typed or chosen in it.
 * LI_NONE if there is nothing. */
Li_Value li_get(Li_Node *n, int input);

/* Set the value of OUTPUT, read by the squares wired to it */
void li_set(Li_Node *n, int output, Li_Value value);

/* Change the type of OUTPUT, like a const whose type is chosen in the
 * square. Wires from it are checked against the new type. */
void li_output_type(Li_Node *n, int output, Li_Type type);

/* Activate every square wired to tick output OUTPUT */
void li_tick(Li_Node *n, int output);

/* Per square state */
void *li_data(Li_Node *n);
void li_set_data(Li_Node *n, void *data);

/* Text drawn inside the square, printf style */
void li_text(Li_Node *n, const char *fmt, ...);

/* A line in the output panel of the editor (and stdout), printf style.
   '\n' in it starts new lines. */
void li_print(Li_Node *n, const char *fmt, ...);

/* Seconds since the program started */
double li_time(void);

/* Declare a setting the user can change from the editor: double click the
   square, or Configure in its menu. VALUE is the default, and its type is
   the param type: LI_INT, LI_FLOAT, LI_BOOL or LI_STRING. Params are saved
   with the canvas. Returns its index, -1 for other types. See module_param. */
int li_param(Li_Node *n, const char *name, Li_Value value);

/* Current value of PARAM. A string is valid until the param changes. */
Li_Value li_get_param(Li_Node *n, int param);

/* Variables, by name (module globals are per square). Like in C, they are
   local to the canvas the square is in: each subgraph square has its own,
   which the canvas around it can't see, and it can't see the ones around
   it. li_global_* are shared by every canvas of the tab, subgraphs
   included. Strings are copied. A variable never set gives LI_NONE, a
   string is valid until the variable is set again. They start empty: they
   are not saved, and Clear all / Load empty them. */
void li_var_set(const char *name, Li_Value value);
Li_Value li_var_get(const char *name);
void li_global_set(const char *name, Li_Value value);
Li_Value li_global_get(const char *name);

/* Lists, by name, local to the canvas like variables. They grow as
   needed and start empty (not saved, Clear all / Load empty them). Strings
   are copied in. Negative indices count from the end: -1 is the last. */
long li_list_len(const char *list);
void li_list_push(const char *list, Li_Value value);            /* append */
Li_Value li_list_pop(const char *list);                         /* remove the last, LI_NONE if empty */
Li_Value li_list_get(const char *list, long index);             /* LI_NONE if out of range */
bool li_list_set(const char *list, long index, Li_Value value); /* false if out of range */
bool li_list_remove(const char *list, long index);              /* false if out of range */
void li_list_clear(const char *list);

/* Trees: string keys to values, by name, in key order (a red-black tree,
   bt.h). Local, copied and emptied like lists. */
void li_tree_set(const char *tree, const char *key, Li_Value value); /* add or replace */
Li_Value li_tree_get(const char *tree, const char *key);             /* LI_NONE if missing */
bool li_tree_has(const char *tree, const char *key);
void li_tree_del(const char *tree, const char *key);
long li_tree_len(const char *tree);
void li_tree_keys(const char *tree, const char *list); /* LIST = the keys, in order */

/* Strings that li_list_get, li_tree_get or li_var_get return live while the
   item does; the one li_list_pop returns until the next pop of that list. */

/* Write V as text in BUF ("42", "true", "-" for LI_NONE). Returns what
 * snprintf returns. */
int li_format(Li_Value v, char *buf, int size);

static inline Li_Value
li_bool(bool b)
{
        return (Li_Value) { .type = LI_BOOL, .b = b };
}
static inline Li_Value
li_int(long i)
{
        return (Li_Value) { .type = LI_INT, .i = i };
}
static inline Li_Value
li_float(double f)
{
        return (Li_Value) { .type = LI_FLOAT, .f = f };
}
static inline Li_Value
li_string(const char *s)
{
        return (Li_Value) { .type = LI_STRING, .s = s };
}
static inline Li_Value
li_ptr(void *p)
{
        return (Li_Value) { .type = LI_PTR, .p = p };
}

#ifdef __cplusplus
}
#endif

#endif /* LINEIT_H_ */

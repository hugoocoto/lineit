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
        Called once per frame while running (Run in the bar). Use it to
        start tick chains (a clock). Ticks run to the end at once, so a
        canvas with no square that has module_frame stops right after
        module_start; with one it runs until Stop.

     void module_start(Li_Node *n);
        Run was pressed in the bar. Called once, before the first
        module_frame. Use it to set things up or start tick chains (entry).
        A square with a tick input is set up: its start runs first, before
        any branch. One with none starts a branch (see src/branches.h).

     void module_draw(Li_Node *n);
        Called once per frame, even when stopped, before drawing. Use it to
        update what the square shows (li_text), like the value of an input.

     void module_destroy(Li_Node *n);
        The square is being removed. Free its state.

     void module_param(Li_Node *n, int param);
        PARAM (see li_param) changed: the user set it in its field, at the
        bottom of the square (or in the settings panel: double click the
        square, or Configure in its menu), or a canvas was loaded. Also called for every param right after module_init, so
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

     const char *module_emit(Li_Node *n);
        Only for pure squares (module_eval, no ticks, no state, no
        variables: inline code runs in no scope of its own). Compile
        (see below) calls it to put the square inline in the program,
        instead of calling its module_eval through li_get: return C that
        defines, for each output K,

            static Li_Value $_outK(void)

        computing it like module_eval, with $_inK() for the value of input
        K (instead of li_get). $_ is replaced by a name for the square, so
        name everything you define $_something; $source by the path of the
        module's C file, to #include it again for its functions (with
        LI_INLINE defined, so it can leave its callbacks out). It's called
        in the editor, so it can read params and options and write C just
        for them. NULL: compiled as usual. See modules/math/arith.h.

   Wires go from an output to an input of the same type. LI_ANY matches any
   type except LI_TICK. A data input takes one wire, a tick input can take
   many. Build modules with `make`, see the Makefile.

   Compile (in the bar, or lineit --compile FILE) makes a program of the
   canvas, with the C of each module in it once per square (or inline, see
   module_emit), and the li_* functions of src/rt. Keep everything in a
   module but these callbacks static, or the copies clash.

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
#include <stddef.h>

/* The version of lineit, MAJOR.MINOR.PATCH. There's only this one: it's
 * also the version of this API, and of the canvases and type files lineit
 * writes. Something made with version V (a canvas, a package built against
 * this lineit.h) works with a lineit of version H if they have the same
 * major and V's minor isn't newer than H's. So the minor goes up when
 * something is added (what older ones made still works), the major when
 * something changes or goes away (it doesn't). The patch is never checked.
 * See tools/lineit-pack --howto for packages. */
#define LI_VERSION "0.0.1"

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
        LI_STRING, /* .s; li_set copies it */
        LI_PTR,    /* .p, anything else, owned by the module that set it */
        LI_LIST,   /* .p, values in order, see li_list_* */
        LI_MAP,    /* .p, string keys to values, in key order, see li_map_* */
        LI_REF,    /* .s, a variable: its name, "::name" for a global (var/var) */
        LI_TYPE_COUNT,
        /* Custom types, records made in the editor (File > New type), come
           after these: LI_TYPE_COUNT and up. Their .p is the record, owned
           by the square that made it. Modules can pass them on (LI_ANY
           pins), keep them in variables, lists and maps (they're copied
           there) and show them with li_format: "{x: 1, y: 2}". */
} Li_Type;

/* printf style functions: the compiler checks their formats */
#if defined(__GNUC__)
#define LI_PRINTF(fmt, args) __attribute__((format(printf, fmt, args)))
#else
#define LI_PRINTF(fmt, args)
#endif

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
 * LI_NONE if there is nothing. It's a copy, valid until the callback that
 * asked returns: branches run in parallel (src/branches.h), and the square
 * it comes from may make the next one meanwhile. li_copy it to keep it. */
Li_Value li_get(Li_Node *n, int input);

/* Set the value of OUTPUT, read by the squares wired to it. The output
 * keeps a copy (strings, lists...), so VALUE can be freed right after. */
void li_set(Li_Node *n, int output, Li_Value value);

/* Change the type of OUTPUT, like a const whose type is chosen in the
 * square, or of INPUT. Wires are checked against the new type. */
void li_output_type(Li_Node *n, int output, Li_Type type);
void li_input_type(Li_Node *n, int input, Li_Type type);

/* Activate every square wired to tick output OUTPUT. Returns when they're
 * done: with more than one, each is a branch, and they run in parallel (see
 * src/branches.h). */
void li_tick(Li_Node *n, int output);

/* Per square state */
void *li_data(Li_Node *n);
void li_set_data(Li_Node *n, void *data);

/* Text drawn inside the square, printf style */
void li_text(Li_Node *n, const char *fmt, ...) LI_PRINTF(2, 3);

/* A line in the output panel of the editor (and stdout), printf style.
   '\n' in it starts new lines. */
void li_print(Li_Node *n, const char *fmt, ...) LI_PRINTF(2, 3);

/* Seconds since the program started */
double li_time(void);

/* Declare a setting the user can change from the editor: a field at the
   bottom of the square, a checkbox for bools (also in its settings panel,
   double click it or Configure in its menu). VALUE is the default, and its type is
   the param type: LI_INT, LI_FLOAT, LI_BOOL or LI_STRING. Params are saved
   with the canvas. Returns its index, -1 for other types. See module_param. */
int li_param(Li_Node *n, const char *name, Li_Value value);

/* Current value of PARAM. A string is valid until the param changes. */
Li_Value li_get_param(Li_Node *n, int param);

/* Variables, by name (module globals are per square). Squares use them
   through var/var, which declares one and gives a LI_REF to it (its name)
   to the squares that read or change it. Like in C, they are local to the
   canvas the square is in: each subgraph square has its own, which the
   canvas around it can't see, and it can't see the ones around it. A name
   that starts with "::" is a global: one for every canvas of the tab,
   subgraphs included. Run empties them all, and the editor and programs
   start with them empty: they are not saved.

   Branches run in parallel, so each of these is one at a time: whole, never
   half done by another branch. li_var_get gives a copy, yours (li_drop it),
   li_var_set copies VALUE in. li_var_update runs FN on the value kept in the
   variable, to change it in place (a list push) or read part of it, with
   nobody else touching it meanwhile; one never set is LI_NONE. li_var_has
   says if it was ever set. */
Li_Value li_var_get(const char *name);
void li_var_set(const char *name, Li_Value value);
void li_var_update(const char *name, void (*fn)(Li_Value *value, void *ctx), void *ctx);
bool li_var_has(const char *name);

/* What var/var says of a variable: its TYPE (LI_ANY for any) and the
   value it starts with. It's set to START if it isn't set yet, or if the
   type or the start changed. li_var_type is LI_ANY until it's declared. */
void li_var_declare(const char *name, Li_Type type, Li_Value start);
Li_Type li_var_type(const char *name);

/* Lists are values (LI_LIST): copied, like strings, when they go into a
   variable, a list or a map. Change the one in a variable with
   li_var_update.
   A list of a module's own starts as (Li_Value) { 0 }, li_list_push makes
   it a list, and li_drop frees it. Negative indices count from the end:
   -1 is the last. Values given back belong to the list: copy them
   (li_copy) to keep them after it changes. */
long li_list_len(Li_Value list);
Li_Value li_list_get(Li_Value list, long index);              /* LI_NONE if out of range */
void li_list_push(Li_Value *list, Li_Value value);            /* append; makes *LIST a list if it isn't */
Li_Value li_list_pop(Li_Value *list);                         /* the last, taken out: li_drop it. LI_NONE if empty */
bool li_list_set(Li_Value *list, long index, Li_Value value); /* false if out of range */
bool li_list_remove(Li_Value *list, long index);              /* false if out of range */
void li_list_clear(Li_Value *list);                           /* makes *LIST an empty list */

/* Maps (LI_MAP): string keys to values, in key order (a red-black tree,
   bt.h). Values, like lists. */
long li_map_len(Li_Value map);
Li_Value li_map_get(Li_Value map, const char *key); /* LI_NONE if missing */
bool li_map_has(Li_Value map, const char *key);
const char *li_map_key(Li_Value map, long index);                /* the INDEXth key, in order, NULL if out of range */
void li_map_set(Li_Value *map, const char *key, Li_Value value); /* add or replace; makes *MAP a map */
void li_map_del(Li_Value *map, const char *key);
void li_map_clear(Li_Value *map); /* makes *MAP an empty map */

/* A copy of V that owns what it points to (strings, lists, maps, records),
   and freeing one: what li_copy, li_list_pop or a module's own list gave.
   li_drop leaves *V as LI_NONE. */
Li_Value li_copy(Li_Value v);
void li_drop(Li_Value *v);

/* Write V as text in BUF ("42", "true", "[1, 2]", "{a: 1}", "-" for
 * LI_NONE). Returns what snprintf returns. */
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

/* A variable by name: what a LI_REF input has typed in its field, and its
 * default (li_input_value(n, "list", LI_REF, li_var_ref(""))) */
static inline Li_Value
li_var_ref(const char *name)
{
        return (Li_Value) { .type = LI_REF, .s = name };
}

/* The variable of a LI_REF input: wired from a var/var, or its name typed
 * in the field. Its name for li_var and the rest, NULL if there's none. */
static inline const char *
li_ref(Li_Node *n, int input)
{
        Li_Value v = li_get(n, input);
        return v.type == LI_REF && v.s && *v.s ? v.s : NULL;
}

#ifdef __cplusplus
}
#endif

#endif /* LINEIT_H_ */

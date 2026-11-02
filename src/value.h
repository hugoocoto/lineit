/* value.h - what values need at run time: records, lists, maps, variables
 * and li_format. Shared by the editor (src/editor/) and the programs
 * Compile makes (src/rt/rt.c), so both run canvases the same way.
 *
 * Whoever links it says what the custom types are: type_is_record and
 * type_field_name below. */

#ifndef VALUE_H_
#define VALUE_H_

#include "bt.h"
#include "cum.h"
#include "lineit.h"

/* A value of a custom type: .p of a Li_Value of it */
typedef struct Record {
        Li_Type type;
        int count;
        Li_Value fields[];
} Record;

/* Given by whoever links this */
bool type_is_record(Li_Type t);
const char *type_field_name(Li_Type t, int field); /* "?" if there's none */
int type_field_count(Li_Type t);
const char *type_field_type(Li_Type t, int field); /* its type, by name: "int32", "geo/point" */
Li_Type type_named(const char *name);              /* by name or last part, LI_NONE if none (or two) */

/* A copy of V that owns what it points to (strings, lists, maps, records),
 * for whoever keeps it longer than the module that made it: free it with
 * value_drop. li_copy and li_drop for modules. */
Li_Value value_dup(Li_Value v);
void value_drop(Li_Value v);
bool value_equal(Li_Value a, Li_Value b);
Li_Value value_widen(Li_Value v, Li_Type t); /* for an input of type T: int to float, bool to int... */

/* Variables, by name: li_var_* */
typedef struct Var {
        char *name;
        Li_Value value; /* a copy it owns (value_dup) */
        bool set;       /* ever set: li_var_has */
        /* What li_var_declare says (var/var): its type, LI_ANY for any */
        bool declared;
        Li_Type type;
        Li_Value start;
} Var;

typedef Da(Var) VarList;

/* The locals of the canvas running, and the globals ("::name"). Whoever
 * runs canvases trades them to change which canvas is running: subgraphs
 * have their own locals, like C functions. */
extern VarList vars;
extern VarList globals;

/* The thread runs a square of the subgraph square whose locals are L, or
 * of the canvas (NULL) */
void value_locals(VarList *l);

void var_list_clear(VarList *l);
void vars_clear(void); /* vars and globals */

#endif /* VALUE_H_ */

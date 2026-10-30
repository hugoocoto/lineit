/* value.h - what values need at run time: records, variables, lists and
 * trees, and li_format. Shared by the editor (src/main.c) and the programs
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

/* A copy of V that owns what it points to (strings, records), for whoever
 * keeps it longer than the module that made it: free it with value_drop */
Li_Value value_dup(Li_Value v);
void value_drop(Li_Value v);
bool value_equal(Li_Value a, Li_Value b);

/* Variables, by name: li_var_* and li_global_* */
typedef struct Var {
        char *name;
        Li_Value value; /* a copy it owns (value_dup) */
} Var;

typedef Da(Var) VarList;

/* The locals of the canvas running (its variables, lists and trees), and
 * the globals. Whoever runs canvases trades them to change which canvas is
 * running: subgraphs have their own locals, like C functions. */
extern VarList vars;
extern VarList globals;
extern BT *lists; /* name -> List * */
extern BT *trees; /* name -> Tree * */

void var_list_clear(VarList *l);
void vars_clear(void);       /* vars and globals */
void containers_clear(void); /* lists and trees */

#endif /* VALUE_H_ */

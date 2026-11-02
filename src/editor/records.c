/* records.c - custom types: records, and the names of the types */
#include "editor.h"

/* Names used in the config (colors.types.<name>) */
const char *type_names[LI_TYPE_COUNT] = {
        [LI_NONE]   = "none",
        [LI_TICK]   = "tick",
        [LI_ANY]    = "any",
        [LI_BOOL]   = "bool",
        [LI_INT]    = "int",
        [LI_FLOAT]  = "float",
        [LI_STRING] = "string",
        [LI_PTR]    = "ptr",
        [LI_LIST]   = "list",
        [LI_MAP]    = "map",
        [LI_REF]    = "ref",
        [LI_BYTES]  = "bytes",
};

TypesDa types;

TypeDef *
type_def(Li_Type t)
{
        int i = (int) t - LI_TYPE_COUNT;
        return i >= 0 && i < types.count ? types.items[i] : NULL;
}

/* For value.c */
bool
type_is_record(Li_Type t)
{
        return type_def(t) != NULL;
}

const char *
type_field_name(Li_Type t, int field)
{
        TypeDef *d = type_def(t);
        return d && field >= 0 && field < d->fields.count ? d->fields.items[field].name : "?";
}

int
type_field_count(Li_Type t)
{
        TypeDef *d = type_def(t);
        return d ? d->fields.count : 0;
}

const char *
type_field_type(Li_Type t, int field)
{
        TypeDef *d = type_def(t);
        return d && field >= 0 && field < d->fields.count ? d->fields.items[field].type : "";
}

Li_Type
type_named(const char *name)
{
        int t = type_from_name(name);
        return type_def(t) ? (Li_Type) t : LI_NONE;
}

/* For people: "int", "point" */
const char *
type_name(Li_Type t)
{
        if ((int) t >= 0 && t < LI_TYPE_COUNT) return type_names[t];
        TypeDef *d = type_def(t);
        return d ? d->label : "?";
}

/* NAME as a type: a built-in one, or a custom one by its name ("geo/point")
 * or, if only one has it, its last part ("point"). -1 if there's none. */
int
type_from_name(const char *name)
{
        int found = -1;
        for (int t = 0; t < LI_TYPE_COUNT; t++)
                if (!strcmp(type_names[t], name)) return t;
        for (int i = 0; i < types.count; i++)
                if (!strcmp(types.items[i]->name, name)) return LI_TYPE_COUNT + i;
        for (int i = 0; i < types.count; i++) {
                if (strcmp(types.items[i]->label, name)) continue;
                if (found >= 0) return -1; /* two: say which */
                found = LI_TYPE_COUNT + i;
        }
        return found;
}

/* Of pins and wires */
Color
type_color(Li_Type t)
{
        if ((int) t >= 0 && t < LI_TYPE_COUNT) return cfg.colors.types[t];
        TypeDef *d = type_def(t);
        return d ? d->color : cfg.colors.types[LI_ANY];
}

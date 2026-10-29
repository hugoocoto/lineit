/* Shared by the list modules: the list name is their first param, and they
 * show it with its length. */
#include "lineit.h"

#define LIST(n) (li_get_param((n), 0).s)

static inline void
list_param(Li_Node *n)
{
        li_param(n, "list", li_string("l"));
}

/* An index from any number, 0 if it isn't one (or nothing is typed) */
static inline long
as_index(Li_Value v)
{
        return v.type == LI_INT ? v.i : v.type == LI_FLOAT ? (long) v.f :
                                        v.type == LI_BOOL  ? v.b :
                                                             0;
}

void
module_draw(Li_Node *n)
{
        li_text(n, "%s [%ld]", LIST(n), li_list_len(LIST(n)));
}

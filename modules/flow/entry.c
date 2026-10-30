/* Ticks once when Start is pressed, before anything else runs. Wire it to
 * what has to be set up first, like clock.reset or var/set. */
#include "lineit.h"

void
module_init(Li_Node *n)
{
        li_output(n, "start", LI_TICK);
}

void
module_start(Li_Node *n)
{
        li_tick(n, 0);
}

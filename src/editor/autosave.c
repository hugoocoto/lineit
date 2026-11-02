/* autosave.c - autosave */
#include "editor.h"

/* While it's on (File > Autosave, or save.autosave in the config), every
 * save.autosave_interval seconds the tabs with changes are saved. Untitled
 * ones aren't: they don't have a file yet. */

void
autosave_tick(void)
{
        static double last;
        char names[200] = "";
        int n           = 0;

        if (!autosave || GetTime() - last < cfg.save.autosave_interval) return;
        /* Not in the middle of something: it's tried again next frame */
        if (drag.kind != DRAG_NONE || editing_text() || ask.open) return;
        last = GetTime();
        for (int i = 0; i < tabs.count; i++) {
                const char *path = i == tab_cur ? canvas_path : tabs.items[i]->path;
                if (!path || !tab_dirty(i) || !tab_save(i)) continue;
                size_t used = strlen(names);
                snprintf(names + used, sizeof names - used, "%s%s", n++ ? ", " : "", tab_name(i));
        }
        if (n) set_status("Autosaved %s", names);
}

/* shortcuts.c - keyboard shortcuts */
#include "editor.h"

/* Ctrl+key for the tabs, in every tab */
static void
shortcuts_tabs(bool shift)
{
        if (key_pressed(KEY_O)) file_open();
        if (key_pressed(KEY_T)) shift ? tab_reopen() : file_new();
        if (key_pressed(KEY_W)) file_close();
        if (key_pressed(KEY_COMMA)) settings_open();
        if (key_pressed(KEY_TAB) || key_pressed(KEY_PAGE_DOWN) || key_pressed(KEY_PAGE_UP))
                file_next_tab(shift || key_pressed(KEY_PAGE_UP) ? -1 : 1);
}

/* Keys that act on the whole canvas. Only when no menu is open and nothing
 * is being typed. */
void
update_shortcuts(void)
{
        bool ctrl  = IsKeyDown(KEY_LEFT_CONTROL) || IsKeyDown(KEY_RIGHT_CONTROL);
        bool shift = IsKeyDown(KEY_LEFT_SHIFT) || IsKeyDown(KEY_RIGHT_SHIFT);

        /* The settings tab types in its search box: only the keys of the tabs */
        if (settings_shown()) {
                if (ctrl) shortcuts_tabs(shift);
                return;
        }
        if (editing_text()) return;

        if (!ctrl) {
                if (key_pressed(KEY_DELETE) || key_pressed(KEY_BACKSPACE)) each_selected(square_delete);
                if (key_pressed(KEY_ESCAPE)) select_all(false);
                return;
        }

        if (key_pressed(KEY_S) && !shift) bar_save();
        if (key_pressed(KEY_S) && shift) file_save_as();
        if (key_pressed(KEY_G)) subgraph_ask();
        shortcuts_tabs(shift);
        if (key_pressed(KEY_Z) && !shift) history_step(true);
        if (key_pressed(KEY_Y) || (key_pressed(KEY_Z) && shift)) history_step(false);
        if (key_pressed(KEY_A)) select_all(true);
        if (key_pressed(KEY_C)) selection_copy();
        if (key_pressed(KEY_X)) {
                selection_copy();
                each_selected(square_delete);
        }
        if (key_pressed(KEY_V)) selection_paste(mouse_world());
        if (key_pressed(KEY_D)) selection_duplicate();
}

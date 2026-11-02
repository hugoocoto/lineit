/* selection.c - the selection: copy, paste, duplicate */
#include "editor.h"

void
select_all(bool on)
{
        Da_foreach(sp, squares)
        {
                (*sp)->selected = on;
        }
}

/* Run FN on every selected square. FN may remove squares. */
void
each_selected(void (*fn)(int id, void *arg))
{
        Da(int) ids = { 0 };
        Da_foreach(sp, squares)
        {
                if ((*sp)->selected) Da_append(&ids, (*sp)->id);
        }
        Da_foreach(id, ids)
        {
                fn(*id, NULL);
        }
        Da_destroy(&ids);
}

int
selected_count(void)
{
        int n = 0;
        Da_foreach(sp, squares)
        {
                n += (*sp)->selected;
        }
        return n;
}

/* Top-left corner of the selection */
static Vector2
selection_corner(void)
{
        Vector2 lo = { INFINITY, INFINITY };
        Da_foreach(sp, squares)
        {
                if ((*sp)->selected) lo = Vector2Min(lo, (*sp)->pos);
        }
        return lo;
}

/* Copies only live here: pasting runs the text as Lua, so the system
 * clipboard (anything copied from anywhere) is never read */
char *clipboard;

void
selection_copy(void)
{
        if (!selected_count()) return;
        free(clipboard);
        clipboard = canvas_text(WRITE_SELECTED);
}

void
selection_paste(Vector2 at)
{
        if (!clipboard) return;
        paste_at = at;
        canvas_read_text(clipboard, READ_PASTE);
}

/* Copy and paste next to it, wires between them included */
void
selection_duplicate(void)
{
        if (!selected_count()) return;
        char *text = canvas_text(WRITE_SELECTED);
        paste_at   = Vector2AddValue(selection_corner(), 2 * cfg.canvas.grid_step);
        canvas_read_text(text, READ_PASTE);
        free(text);
}

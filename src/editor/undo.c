/* undo.c - undo and redo */
#include "editor.h"

#define UNDO_MAX 200

/* Snapshots (canvas_text without camera or state), oldest first */
struct HistoryState history;

void
history_free_list(void *list)
{
        Da(char *) *l = list;
        Da_foreach(t, *l)
        {
                free(*t);
        }
        l->count = 0;
}

/* Forget everything: a new canvas */
void
history_reset(void)
{
        history_free_list(&history.undo);
        history_free_list(&history.redo);
        free(history.current);
        history.current = canvas_text(0);
}

bool
editing_text(void)
{
        return panel.field >= 0 || field_edit.square || (menu.depth && menu.searchable) || prompt_is_open();
}

/* Called every frame: if the canvas changed since the last snapshot (and
 * nothing is half done: a drag, typing), the last one goes to undo */
void
history_check(void)
{
        if (drag.kind != DRAG_NONE || editing_text()) return;

        char *now = canvas_text(0);
        if (history.current && strcmp(now, history.current) == 0) {
                free(now);
                return;
        }
        /* A step run is in the middle of the squares as they were */
        if (history.current && stepping) {
                run_stop();
                set_status("Stopped stepping: the canvas changed");
        }
        if (history.current) {
                if (history.undo.count >= UNDO_MAX) {
                        free(history.undo.items[0]);
                        Da_remove(&history.undo, 0);
                }
                Da_append(&history.undo, history.current);
        }
        history_free_list(&history.redo);
        history.current = now;
}

void
history_step(bool undo)
{
        history_check();
        void *from    = undo ? (void *) &history.undo : (void *) &history.redo;
        void *to      = undo ? (void *) &history.redo : (void *) &history.undo;
        Da(char *) *f = from;
        Da(char *) *t = to;

        if (!f->count) return;
        Da_append(t, history.current);
        history.current = f->items[--f->count];
        canvas_read_text(history.current, READ_RESTORE);
}

void
history_destroy(void)
{
        history_free_list(&history.undo);
        history_free_list(&history.redo);
        Da_destroy(&history.undo);
        Da_destroy(&history.redo);
        free(history.current);
        free(clipboard);
}

/* tabs.c - tabs: a canvas each */
#include "editor.h"

TabsDa tabs;
int tab_cur;      /* the one shown */
bool handed_over; /* a rebuilt editor took over: quit (see reload.c) */
bool quitting;    /* the editor closes after this frame: the window or the last tab was closed */

#define SWAP(a, b)                       \
        do {                             \
                Typeof(a) swap_ = a;     \
                a               = b;     \
                b               = swap_; \
        } while (0)

static void
tab_swap(Tab *t)
{
        SWAP(squares, t->squares);
        SWAP(links, t->links);
        SWAP(camera, t->camera);
        SWAP(vars, t->vars);
        SWAP(globals, t->globals);
        SWAP(history, t->history);
        SWAP(prog, t->prog);
        SWAP(running, t->running);
        SWAP(stepping, t->stepping);
        SWAP(sched, t->sched);
        SWAP(canvas_path, t->path);
}

/* Bring tab I to the globals if it isn't there, or take it back: call it
 * twice around what has to be done on it */
void
tab_visit(int i)
{
        if (i != tab_cur) tab_swap(tabs.items[i]);
}

const char *
tab_name(int i)
{
        if (tabs.items[i]->settings) return "Settings";
        const char *path = i == tab_cur ? canvas_path : tabs.items[i]->path;
        return path ? GetFileName(path) : "untitled";
}

/* Changed since it was loaded or saved */
bool
tab_dirty(int i)
{
        Tab *t          = tabs.items[i];
        const char *now = i == tab_cur ? history.current : t->history.current;
        return !t->settings && t->saved && now && strcmp(t->saved, now) != 0;
}

/* What the tab shown has now is what's saved */
void
tab_mark_saved(void)
{
        if (!tabs.count) return;
        Tab *t = tabs.items[tab_cur];
        free(history.current);
        history.current = canvas_text(0);
        free(t->saved);
        t->saved = strdup(history.current);
}

void
tab_title(void)
{
        SetWindowTitle(TextFormat("%s - %s", tab_name(tab_cur), cfg.window.title));
}

void
tab_switch(int i)
{
        if (i == tab_cur || i < 0 || i >= tabs.count) return;
        settings_edit_end();
        panel_close();
        field_edit.square = NULL;
        drag.kind         = DRAG_NONE;
        menu.depth        = 0;
        tab_swap(tabs.items[tab_cur]);
        tab_swap(tabs.items[i]);
        tab_cur = i;
        /* Sizes may be old: the config or the fonts may have changed */
        Da_foreach(sp, squares)
        {
                (*sp)->side = 0;
        }
        tab_title();
}

/* A new canvas starts with an entry in the middle of the view: it's where
 * everything starts from */
static void
canvas_start(void)
{
        Module *m    = module_find("flow/entry");
        Module *last = last_module; /* "Add ..." stays what the user added */
        Rectangle r  = canvas_rect();
        camera       = (Camera2D) { .offset = { r.x + r.width / 2, r.y + r.height / 2 }, .zoom = 1.0f };
        if (m) square_new(m, (Vector2) { 0, 0 });
        last_module = last;
}

/* A new tab, shown. Its path is PATH (copied), and what's there is loaded
 * if there is something; if not, it's a new canvas (canvas_start). Or the
 * settings tab, if SETTINGS. */
void
tab_add(const char *path, bool is_settings)
{
        Tab *t      = calloc(1, sizeof *t);
        t->camera   = (Camera2D) { .zoom = 1.0f };
        t->settings = is_settings;
        Da_append(&tabs, t);
        /* The first one is the globals already */
        if (tabs.count > 1) tab_switch(tabs.count - 1);
        char why[128];
        canvas_path = path ? strdup(path) : NULL;
        if (path && FileExists(path) && !canvas_check(path, why, sizeof why)) {
                /* Not adopted: saving would overwrite it */
                set_status("Can't open %s: %s", path, why);
                free((char *) canvas_path);
                canvas_path = NULL;
        } else if (path && FileExists(path) && GetFileLength(path) > 0 && !canvas_read_file(path, READ_LOAD)) {
                set_status("Could not load %s", path);
        }
        if (!squares.count && !is_settings) canvas_start();
        history_reset();
        tab_mark_saved(); /* with the entry: a new canvas has no changes yet */
        tab_title();
}

void
tab_new(const char *path)
{
        tab_add(path, false);
}

/* Tabs closed, last last, for Ctrl+Shift+T: everything to bring one back as
 * it was, unsaved changes included (they're still marked unsaved). Tabs
 * never touched and never saved aren't kept. */
typedef struct ClosedTab {
        char *path;   /* NULL if untitled */
        char *canvas; /* canvas_text with camera and state */
        char *saved;
        Typeof(history.undo) undo;
        Typeof(history.redo) redo;
} ClosedTab;

static Da(ClosedTab) closed_tabs;
#define CLOSED_TABS_MAX 20

static void
closed_tab_free(ClosedTab *c)
{
        free(c->path);
        free(c->canvas);
        free(c->saved);
        history_free_list(&c->undo);
        history_free_list(&c->redo);
        Da_destroy(&c->undo);
        Da_destroy(&c->redo);
}

void
closed_tabs_free(void)
{
        Da_foreach(c, closed_tabs)
        {
                closed_tab_free(c);
        }
        Da_destroy(&closed_tabs);
}

/* Close tab I. With unsaved changes it asks first, unless FORCE. */
void
tab_close(int i, bool force)
{
        Tab *t = tabs.items[i];
        if (!force && tab_dirty(i)) {
                ask_close_tab(i);
                return;
        }

        bool keep = tab_dirty(i) || (i == tab_cur ? canvas_path : t->path);
        tab_visit(i);
        if (keep) {
                /* Remembered for Ctrl+Shift+T: its undo and redo go with it */
                ClosedTab c = {
                        .path   = canvas_path ? strdup(canvas_path) : NULL,
                        .canvas = canvas_text(WRITE_CAMERA | WRITE_STATE),
                        .saved  = t->saved ? strdup(t->saved) : NULL,
                        .undo   = history.undo,
                        .redo   = history.redo,
                };
                history.undo = (Typeof(history.undo)) { 0 };
                history.redo = (Typeof(history.redo)) { 0 };
                if (closed_tabs.count == CLOSED_TABS_MAX) {
                        closed_tab_free(&closed_tabs.items[0]);
                        Da_remove(&closed_tabs, 0);
                }
                Da_append(&closed_tabs, c);
        }
        running  = false;
        stepping = false;
        sched_free(sched);
        sched = NULL;
        canvas_clear(-1, NULL);
        Da_destroy(&squares);
        Da_destroy(&links);
        program_free(&prog);
        history_free_list(&history.undo);
        history_free_list(&history.redo);
        Da_destroy(&history.undo);
        Da_destroy(&history.redo);
        free(history.current);
        history.current = NULL;
        free((char *) canvas_path);
        canvas_path = NULL;
        camera      = (Camera2D) { .zoom = 1.0f };
        tab_visit(i);
        /* Now the globals (if it was shown) or T have empty ones */

        free(t->saved);
        free(t);
        Da_remove(&tabs, i);
        if (i < tab_cur) {
                tab_cur--;
        } else if (i == tab_cur) {
                if (!tabs.count) {
                        /* The last one: closing it closes the editor */
                        tab_cur  = 0;
                        quitting = true;
                        return;
                }
                /* The globals are empty: bring the next one, or the last */
                tab_cur = Min(i, tabs.count - 1);
                tab_swap(tabs.items[tab_cur]);
                Da_foreach(sp, squares)
                {
                        (*sp)->side = 0;
                }
        }
        tab_title();
}

/* Show PATH: its tab if it's open, or a new one. An untitled tab shown that
 * wasn't touched goes away, it was only there to have one. */
void
tab_open(const char *path)
{
        char want[PATH_MAX], have[PATH_MAX], why[128];
        bool known = realpath(path, want) != NULL;

        /* A new file is fine, an existing one has to be a canvas */
        if ((FileExists(path) || DirectoryExists(path)) && !canvas_check(path, why, sizeof why)) {
                set_status("Can't open %s: %s", path, why);
                return;
        }

        for (int i = 0; known && i < tabs.count; i++) {
                const char *p = i == tab_cur ? canvas_path : tabs.items[i]->path;
                if (p && realpath(p, have) && !strcmp(want, have)) {
                        tab_switch(i);
                        return;
                }
        }

        Tab *blank = tabs.count && !canvas_path && !tab_dirty(tab_cur) && !settings_shown() ? tabs.items[tab_cur] : NULL;
        tab_new(known ? want : path);
        for (int i = 0; blank && i < tabs.count; i++) {
                if (tabs.items[i] == blank) tab_close(i, true);
        }
        set_status(FileExists(path) ? "Opened %s" : "New file %s", path);
}

/* Ctrl+Shift+T: the last tab closed, back as it was. If its file is open
 * again already, that tab. An untitled tab shown that wasn't touched goes
 * away, like with Open. */
void
tab_reopen(void)
{
        char want[PATH_MAX], have[PATH_MAX];
        if (!closed_tabs.count) {
                set_status("No closed tabs to reopen");
                return;
        }
        ClosedTab c = closed_tabs.items[--closed_tabs.count];

        for (int i = 0; c.path && i < tabs.count; i++) {
                const char *p = i == tab_cur ? canvas_path : tabs.items[i]->path;
                if (p && realpath(p, have) && realpath(c.path, want) && !strcmp(want, have)) {
                        tab_switch(i);
                        set_status("%s is open already", tab_name(i));
                        closed_tab_free(&c);
                        return;
                }
        }

        Tab *blank = tabs.count && !canvas_path && !tab_dirty(tab_cur) && !settings_shown() ? tabs.items[tab_cur] : NULL;
        tab_new(NULL);
        canvas_read_text(c.canvas, READ_LOAD);
        free((char *) canvas_path);
        canvas_path = c.path; /* its strings are the tab's now */
        history_reset();
        Da_destroy(&history.undo);
        Da_destroy(&history.redo);
        history.undo = c.undo;
        history.redo = c.redo;
        Tab *t       = tabs.items[tab_cur];
        free(t->saved);
        t->saved = c.saved;
        free(c.canvas);
        tab_title();
        for (int i = 0; blank && i < tabs.count; i++) {
                if (tabs.items[i] == blank) tab_close(i, true);
        }
        set_status("Reopened %s", tab_name(tab_cur));
}

/* Running tabs that aren't shown go on running */
void
tabs_run_background(void)
{
        for (int t = 0; t < tabs.count; t++) {
                if (t == tab_cur || !tabs.items[t]->running) continue;
                tab_visit(t);
                program_lower();
                program_each(false);
                tab_visit(t);
        }
}

/* "Open in a new tab" of a subgraph: the canvas it's made of. Under
 * build/modules it's a link to the one in modules/, realpath finds that. */
void
square_open_tab(int id, void *arg)
{
        (void) arg;
        char path[PATH_MAX];
        Square *s = square_get(id);
        if (!s || !s->module->path) return;
        tab_open(realpath(s->module->path, path) ? path : s->module->path);
}

float
tabs_height(void)
{
        return cfg.bar.height;
}

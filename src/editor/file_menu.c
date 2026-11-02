/* file_menu.c - the File menu, and the menu of the run button */
#include "editor.h"

static void
file_open_done(const char *path)
{
        tab_open(path);
}

void
file_save_as_done(const char *path)
{
        free((char *) canvas_path);
        canvas_path = strdup(path);
        bar_save();
        tab_title();
}

/* Where Open starts: the folder of the file shown */
const char *
file_dir(void)
{
        static char dir[PATH_MAX];
        if (!canvas_path || !strchr(canvas_path, '/')) return "";
        snprintf(dir, sizeof dir, "%s/", GetDirectoryPath(canvas_path));
        return dir;
}

void
file_new(void)
{
        tab_new(NULL);
}

void
file_open(void)
{
        prompt_open("Open", file_dir(), file_open_done);
}

void
file_save_as(void)
{
        if (not_canvas()) return;
        prompt_open("Save as", canvas_path ? canvas_path : file_dir(), file_save_as_done);
}

void
file_close(void)
{
        tab_close(tab_cur, false);
}

void
file_next_tab(int step)
{
        if (!tabs.count) return; /* the last was just closed */
        tab_switch((tab_cur + step + tabs.count) % tabs.count);
}

#define FILE_ITEM(fn)                            \
        static void fn##_item(int id, void *arg) \
        {                                        \
                (void) id, (void) arg, fn();     \
        }
FILE_ITEM(file_new)
FILE_ITEM(type_new_ask)
FILE_ITEM(tab_reopen)
FILE_ITEM(file_open)
FILE_ITEM(bar_save)
FILE_ITEM(file_save_as)
FILE_ITEM(bar_load)
FILE_ITEM(file_close)
FILE_ITEM(settings_open)
#undef FILE_ITEM

static Menu file_menu;

static void
autosave_toggle(int id, void *arg)
{
        (void) id, (void) arg;
        autosave = !autosave;
        set_status(autosave ? "Autosave on, every %gs" : "Autosave off", cfg.save.autosave_interval);
}

/* The bar button: the menu, above it */
void
file_menu_open(void)
{
        const MenuItem items[] = {
                { .label = "New tab", .hint = "Ctrl+T", .fn = file_new_item },
                { .label = "Reopen closed tab", .hint = "Ctrl+Shift+T", .fn = tab_reopen_item },
                { .label = "Open...", .hint = "Ctrl+O", .fn = file_open_item },
                { .label = "Save", .hint = "Ctrl+S", .fn = bar_save_item },
                { .label = "Save as...", .hint = "Ctrl+Shift+S", .fn = file_save_as_item },
                { .label = "Revert to saved", .fn = bar_load_item },
                { .label = "New type...", .fn = type_new_ask_item },
                { .label = "Autosave", .hint = autosave ? "on" : "off", .fn = autosave_toggle },
                { .label = "Settings", .hint = "Ctrl+,", .fn = settings_open_item },
                { .label = "Close tab", .hint = "Ctrl+W", .fn = file_close_item },
        };
        file_menu.count = 0;
        for (int i = 0; i < countof(items); i++)
                Da_append(&file_menu, items[i]);
        Rectangle b = bar_button_rect(BAR_FILE);
        menu_open(&file_menu, -1, (Vector2) { b.x, b.y - countof(items) * menu_item_height() });
}

/* The arrow of the run button: what it does, chosen and done at once */
static Menu act_menu;

static void
act_choose(int id, void *arg)
{
        (void) id;
        bar_act = (int) (intptr_t) arg;
        bar_act_do();
}

static void
act_stop(int id, void *arg)
{
        (void) id, (void) arg;
        run_stop();
}

void
act_menu_open(void)
{
        act_menu.count = 0;
        Da_append(&act_menu, ((MenuItem) { .label = "Step", .hint = "one square", .fn = act_choose, .arg = (void *) ACT_STEP }));
        Da_append(&act_menu, ((MenuItem) { .label = "Run", .hint = stepping ? "to the end" : NULL, .fn = act_choose, .arg = (void *) ACT_RUN }));
        Da_append(&act_menu, ((MenuItem) { .label = "Compile", .fn = act_choose, .arg = (void *) ACT_COMPILE }));
        if (running) Da_append(&act_menu, ((MenuItem) { .label = "Stop", .fn = act_stop }));
        Rectangle b = bar_button_rect(BAR_ACT);
        menu_open(&act_menu, -1, (Vector2) { b.x, b.y - act_menu.count * menu_item_height() });
}

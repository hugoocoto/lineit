/* context_menu.c - the menus of a square and of the canvas */
#include "editor.h"

/* The square menu acts on the selection (the right clicked square is
 * always in it) */
static void
menu_duplicate(int id, void *arg)
{
        (void) id, (void) arg;
        selection_duplicate();
}

static void
menu_disconnect_inputs(int id, void *arg)
{
        (void) id, (void) arg;
        each_selected(square_disconnect_inputs);
}

static void
menu_disconnect_outputs(int id, void *arg)
{
        (void) id, (void) arg;
        each_selected(square_disconnect_outputs);
}

static void
menu_disconnect_all(int id, void *arg)
{
        (void) id, (void) arg;
        each_selected(square_disconnect_all);
}

static void
menu_delete(int id, void *arg)
{
        (void) id, (void) arg;
        each_selected(square_delete);
}

/* Options shown on right click over a square. They act on every selected
 * square. Add new entries here. */
static const MenuItem square_items[] = {
        { .label = "Duplicate", .fn = menu_duplicate },
        { .label = "Disconnect inputs", .fn = menu_disconnect_inputs },
        { .label = "Disconnect outputs", .fn = menu_disconnect_outputs },
        { .label = "Disconnect all", .fn = menu_disconnect_all },
        { .label = "Delete", .fn = menu_delete },
};
Menu square_menu; /* rebuilt each time it opens */

void
square_menu_build(Square *s)
{
        square_menu.count = 0;
        if (s->params.count) Da_append(&square_menu, (MenuItem) { .label = "Configure", .fn = square_configure });
        if (s->module->builtin == &graph_fns) Da_append(&square_menu, (MenuItem) { .label = "Open in a new tab", .fn = square_open_tab });
        Da_append(&square_menu, (MenuItem) { .label = "Make subgraph...", .hint = "Ctrl+G", .fn = menu_subgraph });
        if (type_def(s->module->type)) Da_append(&square_menu, (MenuItem) { .label = "Edit type...", .fn = type_edit });
        for (int i = 0; i < countof(square_items); i++)
                Da_append(&square_menu, square_items[i]);
}

/* Options shown on right click over the empty canvas. Add new entries here. */
void
canvas_menu_build(void)
{
        static char add_last[128];

        canvas_menu.count = 0;
        if (last_module) {
                snprintf(add_last, sizeof add_last, "Add %s", last_module->label);
                Da_append(&canvas_menu, (MenuItem) { .label = add_last, .fn = canvas_add_module, .arg = last_module });
        }
        if (module_menu.count) Da_append(&canvas_menu, (MenuItem) { .label = "Add module", .sub = &module_menu });
        Da_append(&canvas_menu, (MenuItem) { .label = "Reset view", .fn = canvas_reset_view });
        Da_append(&canvas_menu, (MenuItem) { .label = "Clear all", .fn = canvas_clear });
}

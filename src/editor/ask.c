/* ask.c - unsaved changes: asked before they are lost */
#include "editor.h"

/* Closing a tab or the window with unsaved changes asks: Save or Don't
 * save. The x at its top right (or Esc) leaves everything as it was. Enter
 * presses the button chosen, Save at first; Tab chooses the next one.
 * Saving an untitled one asks where first (Save as). */

struct AskState ask;

enum { ASK_SAVE,
       ASK_DONT_SAVE,
       ASK_BUTTONS,
       ASK_DISMISS }; /* the x */
static const char *ask_labels[ASK_BUTTONS] = { "Save", "Don't save" };

static int
tab_index(Tab *t)
{
        for (int i = 0; i < tabs.count; i++)
                if (tabs.items[i] == t) return i;
        return -1;
}

/* Save tab I where it was, false if it has no path or it can't */
bool
tab_save(int i)
{
        Tab *t = tabs.items[i];
        tab_visit(i);
        bool ok = canvas_path && canvas_save(canvas_path);
        if (ok) {
                free(history.current);
                history.current = canvas_text(0);
                free(t->saved);
                t->saved = strdup(history.current);
        }
        if (!ok && canvas_path) set_status("Could not save %s", canvas_path);
        tab_visit(i);
        return ok;
}

void
ask_close_tab(int i)
{
        tab_switch(i); /* show what it's about */
        ask.open   = true;
        ask.tab    = tabs.items[i];
        ask.choice = 0; /* Save */
        snprintf(ask.text, sizeof ask.text, "%s has unsaved changes", tab_name(i));
}

static void
save_as_then_close(const char *path)
{
        file_save_as_done(path);
        if (!tab_dirty(tab_cur)) tab_close(tab_cur, true);
}

static void
save_as_then_quit(const char *path)
{
        file_save_as_done(path);
        if (!tab_dirty(tab_cur)) quit_request();
}

/* The window is closing: ask if something isn't saved */
void
quit_request(void)
{
        char names[200] = "";
        int n           = 0;
        for (int i = 0; i < tabs.count; i++) {
                if (!tab_dirty(i)) continue;
                size_t used = strlen(names);
                snprintf(names + used, sizeof names - used, "%s%s", n++ ? ", " : "", tab_name(i));
        }
        if (!n) {
                quitting = true;
                return;
        }
        ask.open   = true;
        ask.tab    = NULL;
        ask.choice = 0; /* Save */
        snprintf(ask.text, sizeof ask.text, "Unsaved changes in %s", names);
}

static void
ask_answer(int button)
{
        ask.open = false;
        if (button == ASK_DISMISS) return;

        if (ask.tab) {
                int i = tab_index(ask.tab);
                if (i < 0) return;
                if (button == ASK_DONT_SAVE) {
                        tab_close(i, true);
                } else if (!canvas_path) {
                        prompt_open("Save as", file_dir(), save_as_then_close);
                } else if (tab_save(i)) {
                        tab_close(i, true);
                }
                return;
        }

        if (button == ASK_DONT_SAVE) {
                quitting = true;
                return;
        }
        /* Save all that have a path, then ask where for the first untitled */
        for (int i = 0; i < tabs.count; i++) {
                const char *path = i == tab_cur ? canvas_path : tabs.items[i]->path;
                if (tab_dirty(i) && path && !tab_save(i)) return;
        }
        for (int i = 0; i < tabs.count; i++) {
                if (!tab_dirty(i)) continue;
                tab_switch(i);
                prompt_open("Save as", file_dir(), save_as_then_quit);
                return;
        }
        quitting = true;
}

static Rectangle
ask_rect(void)
{
        float w = fminf(fmaxf(measure_text(ask.text, cfg.menu.font_size) + 6 * cfg.menu.padding, 460), GetScreenWidth() - 32);
        float h = 3 * menu_item_height() + 2 * cfg.menu.padding;
        return (Rectangle) { (GetScreenWidth() - w) / 2, (GetScreenHeight() - h) / 2, w, h };
}

/* Right to left, at the bottom of it */
static Rectangle
ask_button_rect(int b)
{
        Rectangle r = ask_rect();
        float pad   = cfg.menu.padding;
        float x     = r.x + r.width - pad;
        float w     = 0;
        for (int k = ASK_BUTTONS - 1; k >= b; k--) {
                w = measure_text(ask_labels[k], cfg.menu.font_size) + 4 * pad;
                x -= w + pad;
        }
        return (Rectangle) { x + pad, r.y + r.height - menu_item_height() - pad, w, menu_item_height() };
}

/* The x, at the top right */
static Rectangle
ask_dismiss_rect(void)
{
        Rectangle r = ask_rect();
        float side  = menu_item_height();
        return (Rectangle) { r.x + r.width - side, r.y, side, side };
}

/* Enter presses the button chosen, Tab (Shift+Tab back) chooses another,
 * Esc is the x */
bool
update_ask(void)
{
        if (!ask.open) return false;
        Vector2 mouse = GetMousePosition();
        bool shift    = IsKeyDown(KEY_LEFT_SHIFT) || IsKeyDown(KEY_RIGHT_SHIFT);
        if (key_pressed(KEY_TAB)) {
                ask.choice = (ask.choice + (shift ? ASK_BUTTONS - 1 : 1)) % ASK_BUTTONS;
        } else if (key_pressed(KEY_ENTER) || key_pressed(KEY_KP_ENTER)) {
                ask_answer(ask.choice);
        } else if (key_pressed(KEY_ESCAPE)) {
                ask_answer(ASK_DISMISS);
        } else if (IsMouseButtonPressed(MOUSE_BUTTON_LEFT)) {
                if (CheckCollisionPointRec(mouse, ask_dismiss_rect())) ask_answer(ASK_DISMISS);
                for (int b = 0; ask.open && b < ASK_BUTTONS; b++)
                        if (CheckCollisionPointRec(mouse, ask_button_rect(b))) ask_answer(b);
        }
        return true; /* nothing else while it's open */
}

void
draw_ask(void)
{
        if (!ask.open) return;
        Rectangle r = ask_rect();
        float pad   = cfg.menu.padding;
        float fs    = cfg.menu.font_size;

        DrawRectangle(0, 0, GetScreenWidth(), GetScreenHeight(), Fade(BLACK, 0.4f));
        DrawRectangleRec(r, cfg.colors.menu);
        DrawRectangleLinesEx(r, 1.0f, cfg.colors.menu_border);
        draw_text(ask.text, r.x + 3 * pad, r.y + 2 * pad, fs, cfg.colors.menu_text);
        draw_text(ask.tab ? "Save it before closing it?" : "Save before closing lineit?", r.x + 3 * pad,
                  r.y + 2 * pad + menu_item_height() * 0.8f, fs, cfg.colors.name);
        for (int b = 0; b < ASK_BUTTONS; b++) {
                Rectangle br = ask_button_rect(b);
                bool hover   = CheckCollisionPointRec(GetMousePosition(), br);
                DrawRectangleRec(br, hover || b == ask.choice ? cfg.colors.menu_hover : cfg.colors.bar);
                DrawRectangleLinesEx(br, 1.0f, cfg.colors.menu_border);
                /* The one Enter presses */
                if (b == ask.choice) DrawRectangleLinesEx(br, 2.0f, cfg.colors.selection);
                draw_text(ask_labels[b], br.x + (br.width - measure_text(ask_labels[b], fs)) / 2, br.y + pad, fs,
                          cfg.colors.menu_text);
        }
        Rectangle x = ask_dismiss_rect();
        if (CheckCollisionPointRec(GetMousePosition(), x)) DrawRectangleRec(x, cfg.colors.menu_hover);
        draw_text(TAB_CLOSE, x.x + (x.width - measure_text(TAB_CLOSE, fs)) / 2, x.y + pad, fs, cfg.colors.name);
}

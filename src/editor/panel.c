/* panel.c - the settings panel of a square: its params */
#include "editor.h"

/* Settings panel of a square, next to it */
struct PanelState panel = { .field = -1 };

void
panel_open(Square *s)
{
        panel.square = s;
        panel.field  = -1;
        panel.error  = false;
}

void
panel_close(void)
{
        panel.square = NULL;
        panel.field  = -1;
}

#define PANEL_WIDTH 260

static float
panel_row_height(void)
{
        return cfg.menu.font_size + 2 * cfg.menu.padding;
}

/* Screen space, at the right of the square, inside the canvas. First row is
 * the title, then one per param. */
static Rectangle
panel_rect(void)
{
        Rectangle sr = square_rect(panel.square);
        Vector2 p    = GetWorldToScreen2D((Vector2) { sr.x + sr.width, sr.y }, camera);
        Rectangle c  = canvas_rect();
        Rectangle r  = { p.x + 16, p.y, PANEL_WIDTH, (panel.square->params.count + 1) * panel_row_height() };

        r.x = Clamp(r.x, 0, fmaxf(0, c.width - r.width));
        r.y = Clamp(r.y, 0, fmaxf(0, c.height - r.height));
        return r;
}

static Rectangle
panel_field_rect(int i)
{
        Rectangle r = panel_rect();
        float h     = panel_row_height();
        float pad   = cfg.menu.padding;
        return (Rectangle) { r.x + r.width * 0.45f, r.y + (i + 1) * h + pad / 2, r.width * 0.55f - pad, h - pad };
}

static Rectangle
panel_close_rect(void)
{
        Rectangle r = panel_rect();
        float h     = panel_row_height();
        return (Rectangle) { r.x + r.width - h, r.y, h, h };
}

/* Apply what is typed. On error the field stays open. */
static bool
panel_apply(void)
{
        if (panel.field < 0) return true;
        if (!param_set_text(panel.square, panel.field, panel.text.buf)) {
                panel.error = true;
                return false;
        }
        panel.field = -1;
        panel.error = false;
        return true;
}

static void
panel_edit(int i)
{
        char buf[sizeof panel.text.buf];
        param_text(&panel.square->params.items[i], buf, sizeof buf, true); /* all of a float */
        text_input_set(&panel.text, buf);
        panel.field = i;
        panel.error = false;
}

/* Returns true if the panel consumed the input this frame. */
bool
update_panel(void)
{
        bool changed = false;

        if (!panel.square) return false;

        Vector2 mouse = GetMousePosition();
        bool inside   = CheckCollisionPointRec(mouse, panel_rect());
        int count     = panel.square->params.count;

        if (panel.field >= 0) {
                switch (text_input_update(&panel.text, &changed)) {
                case TEXT_DONE: panel_apply(); break;
                case TEXT_NEXT: {
                        int next = (panel.field + 1) % count;
                        if (panel_apply()) panel_edit(next);
                        break;
                }
                case TEXT_CANCEL: panel.field = -1; return true; /* keep the old value */
                case TEXT_TYPING: break;
                }
                if (changed) panel.error = false;
        } else if (key_pressed(KEY_ESCAPE)) {
                panel_close();
                return true;
        }

        if (IsMouseButtonPressed(MOUSE_BUTTON_LEFT)) {
                if (!inside) {
                        /* Clicking away applies what's valid and closes it */
                        panel_apply();
                        panel_close();
                        return false;
                }
                if (CheckCollisionPointRec(mouse, panel_close_rect())) {
                        panel_apply();
                        panel_close();
                        return true;
                }
                for (int i = 0; i < count; i++) {
                        if (!CheckCollisionPointRec(mouse, panel_field_rect(i))) continue;
                        if (panel.field != i && !panel_apply()) break;
                        Param *p = &panel.square->params.items[i];
                        if (p->value.type == LI_BOOL)
                                param_set_text(panel.square, i, p->value.b ? "false" : "true");
                        else if (panel.field != i)
                                panel_edit(i);
                        break;
                }
                return true;
        }

        if (IsMouseButtonPressed(MOUSE_BUTTON_RIGHT) && !inside) panel_close();
        return false;
}

void
draw_panel(void)
{
        if (!panel.square) return;

        Rectangle r = panel_rect();
        float h     = panel_row_height();
        float fs    = cfg.menu.font_size;
        float pad   = cfg.menu.padding;

        DrawRectangleRec(r, cfg.colors.menu);
        DrawRectangleLinesEx(r, 1.0f, cfg.colors.menu_border);
        draw_text(panel.square->module->name, r.x + pad, r.y + pad, fs, cfg.colors.menu_text);

        Rectangle x = panel_close_rect();
        if (CheckCollisionPointRec(GetMousePosition(), x)) DrawRectangleRec(x, cfg.colors.menu_hover);
        draw_text("x", x.x + (x.width - measure_text("x", fs)) / 2, r.y + pad, fs, cfg.colors.menu_text);

        for (int i = 0; i < panel.square->params.count; i++) {
                Param *p    = &panel.square->params.items[i];
                Rectangle f = panel_field_rect(i);
                bool typing = panel.field == i;
                char buf[256];

                draw_text(p->name, r.x + pad, r.y + (i + 1) * h + pad, fs, cfg.colors.menu_text);

                if (!typing) param_text(p, buf, sizeof buf, false);

                Color border = panel.error && typing ? cfg.colors.error :
                               typing                ? cfg.colors.pin_hover :
                                                       cfg.colors.menu_border;
                DrawRectangleRec(f, cfg.colors.field);
                DrawRectangleLinesEx(f, typing ? 2.0f : 1.0f, border);

                /* Long values are cut at the field; typed, it shows the cursor */
                Rectangle in = { f.x + 4, f.y, f.width - 8, f.height };
                if (typing) {
                        text_input_draw(&panel.text, in, fs, cfg.colors.square_text, false);
                } else {
                        BeginScissorMode(f.x, f.y, f.width, f.height);
                        draw_text(buf, in.x, f.y + (f.height - fs) / 2, fs, cfg.colors.square_text);
                        EndScissorMode();
                }
        }
}

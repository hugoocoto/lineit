/* tab_strip.c - the tab strip, above the canvas */
#include "editor.h"
#define TAB_NEW "+"

static float
tab_pad(void)
{
        return cfg.bar.font_size / 2.0f;
}

/* Tab I, or the + after the last one for I == tabs.count. They shrink to
 * fit when there are many. */
static Rectangle
tab_rect(int i)
{
        float fs    = cfg.bar.font_size;
        float pad   = tab_pad();
        float close = measure_text(TAB_CLOSE, fs) + 2 * pad;
        float plus  = measure_text(TAB_NEW, fs) + 4 * pad;
        float total = plus, x = 0;
        int count = tabs.count;
        assert(count >= 0);
        float w[count + 1];

        for (int k = 0; k < count; k++) {
                w[k] = measure_text(TextFormat("%s*", tab_name(k)), fs) + 3 * pad + close;
                total += w[k];
        }
        float scale = total > GetScreenWidth() ? (GetScreenWidth() - plus) / (total - plus) : 1;
        for (int k = 0; k < i && k < count; k++)
                x += w[k] * scale;
        return (Rectangle) { x, 0, i >= 0 && i < count ? w[i] * scale : plus, tabs_height() };
}

static Rectangle
tab_close_rect(int i)
{
        Rectangle r = tab_rect(i);
        float cw    = measure_text(TAB_CLOSE, cfg.bar.font_size) + 2 * tab_pad();
        return (Rectangle) { r.x + r.width - cw - tab_pad() / 2, r.y, cw, r.height };
}

/* Click a tab to show it, its x or the middle button to close it, + for a
 * new one */
bool
update_tabs(void)
{
        Vector2 mouse = GetMousePosition();
        bool left     = IsMouseButtonPressed(MOUSE_BUTTON_LEFT);
        bool middle   = IsMouseButtonPressed(MOUSE_BUTTON_MIDDLE);

        if (mouse.y >= tabs_height() || (!left && !middle)) return false;
        for (int i = 0; i < tabs.count; i++) {
                if (!CheckCollisionPointRec(mouse, tab_rect(i))) continue;
                if (middle || CheckCollisionPointRec(mouse, tab_close_rect(i)))
                        tab_close(i, false);
                else
                        tab_switch(i);
                return true;
        }
        if (left && CheckCollisionPointRec(mouse, tab_rect(tabs.count))) tab_new(NULL);
        menu.depth = 0;
        return true;
}

void
draw_tabs(void)
{
        float fs      = cfg.bar.font_size;
        float pad     = tab_pad();
        Vector2 mouse = GetMousePosition();

        DrawRectangle(0, 0, GetScreenWidth(), tabs_height(), cfg.colors.bar);
        DrawLine(0, tabs_height(), GetScreenWidth(), tabs_height(), cfg.colors.bar_border);
        for (int i = 0; i < tabs.count; i++) {
                Rectangle r = tab_rect(i);
                Rectangle c = tab_close_rect(i);
                float ty    = r.y + (r.height - fs) / 2;

                /* The one shown joins the canvas below it */
                if (i == tab_cur)
                        DrawRectangleRec((Rectangle) { r.x, r.y, r.width, r.height + 1 }, cfg.colors.bg);
                else if (CheckCollisionPointRec(mouse, r))
                        DrawRectangleRec(r, cfg.colors.menu);
                DrawLine(r.x + r.width, r.y, r.x + r.width, r.y + r.height, cfg.colors.bar_border);

                BeginScissorMode(r.x, r.y, c.x - r.x, r.height);
                draw_text(TextFormat("%s%s", tab_name(i), tab_dirty(i) ? "*" : ""), r.x + pad, ty, fs,
                          i == tab_cur ? cfg.colors.menu_text : cfg.colors.bar_text);
                EndScissorMode();
                if (CheckCollisionPointRec(mouse, c)) DrawRectangleRec(c, cfg.colors.menu_hover);
                draw_text(TAB_CLOSE, c.x + pad, ty, fs, cfg.colors.name);
        }
        Rectangle p = tab_rect(tabs.count);
        if (CheckCollisionPointRec(mouse, p)) DrawRectangleRec(p, cfg.colors.menu);
        draw_text(TAB_NEW, p.x + 2 * pad, p.y + (p.height - fs) / 2, fs, cfg.colors.bar_text);
}

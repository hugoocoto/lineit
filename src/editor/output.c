/* output.c - the output panel: what print writes */
#include "editor.h"

struct OutputPanelState output_panel;

/* Branches print from their threads: one at a time */
pthread_mutex_t output_lock = PTHREAD_MUTEX_INITIALIZER;

/* Add TEXT, one line per '\n' in it. With output_lock. */
void
output_append(const char *text, bool info)
{
        const char *p = text;
        do {
                const char *nl = strchr(p, '\n');
                int len        = nl ? (int) (nl - p) : (int) strlen(p);
                Da_append(&output_panel.lines, (OutputLine) { .text = strndup(p, len), .info = info });
                if (!output_panel.open) output_panel.unseen++;
                if (output_panel.scroll) output_panel.scroll++; /* scrolled up: keep what's shown */
                p += len + (nl != NULL);
        } while (*p);

        /* Oldest first out. Remove in one go: the list can be long. */
        int extra = output_panel.lines.count - cfg.output.lines;
        if (extra > 0) {
                for (int i = 0; i < extra; i++)
                        free(output_panel.lines.items[i].text);
                memmove(output_panel.lines.items, output_panel.lines.items + extra, (output_panel.lines.count - extra) * sizeof *output_panel.lines.items);
                output_panel.lines.count -= extra;
        }
}

void
output_add(const char *text, bool info)
{
        pthread_mutex_lock(&output_lock);
        output_append(text, info);
        pthread_mutex_unlock(&output_lock);
}

void
output_clear(void)
{
        Da_foreach(l, output_panel.lines)
        {
                free(l->text);
        }
        output_panel.lines.count = 0;
        output_panel.scroll      = 0;
        output_panel.unseen      = 0;
}

/* The panel, above the bar, while it's open */
Rectangle
output_rect(void)
{
        float h = output_panel.open ? Min(cfg.output.height, GetScreenHeight() - cfg.bar.height - 40) : 0;
        return (Rectangle) { 0, GetScreenHeight() - cfg.bar.height - h, GetScreenWidth(), h };
}

static float
output_line_height(void)
{
        return cfg.output.font_size + 4;
}

static Rectangle
output_header_rect(void)
{
        Rectangle r = output_rect();
        return (Rectangle) { r.x, r.y, r.width, cfg.output.font_size + 12 };
}

static Rectangle
output_clear_rect(void)
{
        Rectangle h = output_header_rect();
        float w     = measure_text("Clear", cfg.output.font_size) + 16;
        return (Rectangle) { h.x + h.width - w - 4, h.y + 3, w, h.height - 6 };
}

static int
output_visible_lines(void)
{
        Rectangle r = output_rect();
        return Max(1, (int) ((r.height - output_header_rect().height - 4) / output_line_height()));
}

/* Returns true if the panel consumed the input this frame. */
bool
update_output(void)
{
        if (!output_panel.open) return false;

        Vector2 mouse = GetMousePosition();
        if (!CheckCollisionPointRec(mouse, output_rect())) return false;

        /* The wheel scrolls it (the canvas doesn't zoom from here) */
        float wheel = GetMouseWheelMove();
        if (wheel) {
                int max             = Max(0, output_panel.lines.count - output_visible_lines());
                output_panel.scroll = Clamp(output_panel.scroll + (int) (wheel * 3), 0, max);
        }
        if (IsMouseButtonPressed(MOUSE_BUTTON_LEFT)) {
                if (CheckCollisionPointRec(mouse, output_clear_rect())) output_clear();
                return true;
        }
        return IsMouseButtonPressed(MOUSE_BUTTON_RIGHT);
}

void
draw_output(void)
{
        if (!output_panel.open) return;

        Rectangle r = output_rect();
        Rectangle h = output_header_rect();
        Rectangle c = output_clear_rect();
        float fs    = cfg.output.font_size;
        float lh    = output_line_height();
        int n       = output_visible_lines();

        DrawRectangleRec(r, cfg.colors.output);
        DrawLine(r.x, r.y, r.x + r.width, r.y, cfg.colors.bar_border);

        const char *title = output_panel.scroll ? TextFormat("Output (%d newer below)", output_panel.scroll) : "Output";
        draw_text(title, h.x + 10, h.y + (h.height - fs) / 2, fs, cfg.colors.bar_text);
        DrawRectangleRec(c, CheckCollisionPointRec(GetMousePosition(), c) ? cfg.colors.menu_hover : cfg.colors.menu);
        DrawRectangleLinesEx(c, 1.0f, cfg.colors.menu_border);
        draw_text("Clear", c.x + 8, c.y + (c.height - fs) / 2, fs, cfg.colors.bar_text);
        DrawLine(r.x, h.y + h.height, r.x + r.width, h.y + h.height, cfg.colors.bar_border);

        /* From the top, first line first. When they don't all fit, the
         * newest ones (or as far up as it's scrolled) */
        BeginScissorMode(r.x, h.y + h.height, r.width, r.height - h.height);
        int last  = output_panel.lines.count - 1 - output_panel.scroll;
        int first = Max(0, last - n + 1);
        for (int k = first; k <= last; k++) {
                OutputLine *l = &output_panel.lines.items[k];
                float y       = h.y + h.height + 2 + (k - first) * lh;
                draw_text(l->text, r.x + 10, y + 2, fs, l->info ? cfg.colors.name : cfg.colors.output_text);
        }
        EndScissorMode();
}

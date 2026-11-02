/* tooltips.c - tooltips of pins */
#include "editor.h"

/* Value of output OUT of S as text, for the tooltip */
static void
output_tooltip(Square *s, int out, char *buf, int size)
{
        PinInfo *p = pin_info(s, PART_OUTPUT, out);
        char v[128];

        if (!p) return;
        if (p->type == LI_TICK) {
                double t = out < (int) countof(s->fired) ? s->fired[out] : 0;
                if (t > 0)
                        snprintf(buf, size, "tick, %.1f s ago", GetTime() - t);
                else
                        snprintf(buf, size, "tick, not fired yet");
                return;
        }
        /* Pure squares compute it when read */
        if (s->fn.eval && !s->evaluating) {
                s->evaluating = true;
                s->fn.eval(s);
                s->evaluating = false;
        }
        if (p->value.type == LI_NONE) {
                snprintf(buf, size, "no value");
                return;
        }
        li_format(p->value, v, sizeof v);
        snprintf(buf, size, "%s %s", type_name(p->value.type), v);
}

/* The type input IN of S expects, and what it gets: from its wire, or
 * typed in its field */
static void
input_tooltip(Square *s, int in, char *buf, int size)
{
        PinInfo *p    = pin_info(s, PART_INPUT, in);
        char got[160] = "", v[128];
        int l         = link_to(s->id, in);

        if (!p) return;
        if (l >= 0) {
                Square *a = square_get(links.items[l].from);
                if (!link_valid(links.items[l]))
                        snprintf(got, sizeof got, "its wire carries nothing");
                else if (a)
                        output_tooltip(a, links.items[l].from_pin, got, sizeof got);
        } else if (p->kind == PIN_VALUE && p->value.type != LI_NONE) {
                li_format(p->value, v, sizeof v);
                snprintf(got, sizeof got, "typed %s", v);
        }
        snprintf(buf, size, "expects %s%s%s", p->type == LI_REF ? "a variable (ref)" : type_name(p->type), *got ? ", gets " : "",
                 got);
}

/* What's on the wire or pin under the mouse */
void
draw_tooltip(void)
{
        char buf[1024] = "";
        Vector2 mouse  = GetMousePosition();
        Vector2 world  = mouse_world();

        if (drag.kind != DRAG_NONE || menu.depth || !CheckCollisionPointRec(mouse, canvas_rect())) return;

        Hit h = square_hit(world);
        if (h.part == PART_OUTPUT) {
                output_tooltip(h.square, h.pin, buf, sizeof buf);
        } else if (h.part == PART_INPUT) {
                input_tooltip(h.square, h.pin, buf, sizeof buf);
        } else if ((h.part == PART_FIELD || h.part == PART_PARAM) && !field_typing(h.square, h.pin, h.part == PART_PARAM)) {
                /* A value longer than its field: all of it */
                char v[sizeof buf];
                bool param  = h.part == PART_PARAM;
                Rectangle f = param ? param_rect(h.square, h.pin) : field_rect(h.square, h.pin);
                bool opts   = !param && h.square->inputs.items[h.pin].kind == PIN_OPTIONS;
                if (param)
                        param_text(&h.square->params.items[h.pin], v, sizeof v, false);
                else
                        input_text(h.square, h.pin, v, sizeof v, false);
                if (measure_canvas_text(v, cfg.canvas.pin_font_size) > field_text_rect(f, opts).width)
                        snprintf(buf, sizeof buf, "%s", v);
        } else if (h.part == PART_NONE) {
                int l = link_hit(world);
                if (l < 0) return;
                Square *a = square_get(links.items[l].from);
                Square *b = square_get(links.items[l].to);
                if (!link_valid(links.items[l]))
                        snprintf(buf, sizeof buf, "%s -> %s: carries nothing",
                                 type_name(pin_info(a, PART_OUTPUT, links.items[l].from_pin)->type),
                                 type_name(pin_info(b, PART_INPUT, links.items[l].to_pin)->type));
                else
                        output_tooltip(a, links.items[l].from_pin, buf, sizeof buf);
        }
        if (!*buf) return;

        float fs        = cfg.menu.font_size;
        float pad       = cfg.menu.padding;
        const char *tip = text_fit(buf, fs, GetScreenWidth() - 2 * pad); /* cut to the window */
        Rectangle r     = { mouse.x + 16, mouse.y + 16, measure_text(tip, fs) + 2 * pad, fs + 2 * pad };
        r.x             = fminf(r.x, GetScreenWidth() - r.width);
        r.y             = fminf(r.y, GetScreenHeight() - r.height);
        DrawRectangleRec(r, cfg.colors.menu);
        DrawRectangleLinesEx(r, 1.0f, cfg.colors.menu_border);
        draw_text(tip, r.x + pad, r.y + pad, fs, cfg.colors.menu_text);
}

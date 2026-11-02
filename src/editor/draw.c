/* draw.c - drawing the canvas: the grid, wires and squares */
#include "editor.h"

static void
draw_grid(void)
{
        Rectangle r = canvas_rect();
        Vector2 tl  = GetScreenToWorld2D((Vector2) { r.x, r.y }, camera);
        Vector2 br  = GetScreenToWorld2D((Vector2) { r.x + r.width, r.y + r.height }, camera);

        float step  = cfg.canvas.grid_step;
        int major   = cfg.canvas.grid_major;
        float x0    = floorf(tl.x / step) * step;
        float y0    = floorf(tl.y / step) * step;
        float thick = 1.0f / camera.zoom;

        for (float x = x0; x <= br.x; x += step) {
                Color c = ((int) roundf(x / step) % major == 0) ? cfg.colors.grid_major : cfg.colors.grid;
                DrawLineEx((Vector2) { x, tl.y }, (Vector2) { x, br.y }, thick, c);
        }
        for (float y = y0; y <= br.y; y += step) {
                Color c = ((int) roundf(y / step) % major == 0) ? cfg.colors.grid_major : cfg.colors.grid;
                DrawLineEx((Vector2) { tl.x, y }, (Vector2) { br.x, y }, thick, c);
        }
}

/* Bezier control points of the wire from A to B */
static void
wire_controls(Vector2 a, Vector2 b, Vector2 *c1, Vector2 *c2)
{
        float dx = fmaxf(fabsf(b.x - a.x), fabsf(b.y - a.y)) * 0.5;
        *c1      = (Vector2) { a.x + dx, a.y };
        *c2      = (Vector2) { b.x - dx, b.y };
}

static void
draw_wire_ex(Vector2 a, Vector2 b, Color color, float thick)
{
        Vector2 c1, c2;
        wire_controls(a, b, &c1, &c2);
        DrawSplineSegmentBezierCubic(a, c1, c2, b, thick, color);
}

static void
draw_wire(Vector2 a, Vector2 b, Color color)
{
        draw_wire_ex(a, b, color, cfg.canvas.wire_thickness);
}

/* 1 right after T, fading to 0 after flash_time */
float
glow(double t)
{
        if (t <= 0 || cfg.canvas.flash_time <= 0) return 0;
        float f = 1 - (GetTime() - t) / cfg.canvas.flash_time;
        return Clamp(f, 0, 1);
}

/* Distance from P to the segment AB */
static float
segment_distance(Vector2 p, Vector2 a, Vector2 b)
{
        Vector2 ab = Vector2Subtract(b, a);
        float len  = Vector2LengthSqr(ab);
        float t    = len > 0 ? Clamp(Vector2DotProduct(Vector2Subtract(p, a), ab) / len, 0, 1) : 0;
        return Vector2Distance(p, Vector2Add(a, Vector2Scale(ab, t)));
}

/* Index of the link whose wire passes under world point P, -1 if none */
int
link_hit(Vector2 p)
{
        float near = cfg.canvas.wire_thickness / 2 + 5 / camera.zoom;

        for (int i = links.count - 1; i >= 0; i--) {
                Square *a = square_get(links.items[i].from), *b = square_get(links.items[i].to);
                if (!a || !b) continue;
                Vector2 p1 = pin_pos(a, PART_OUTPUT, links.items[i].from_pin);
                Vector2 p2 = pin_pos(b, PART_INPUT, links.items[i].to_pin);
                Vector2 c1, c2, prev = p1;
                wire_controls(p1, p2, &c1, &c2);
                for (int k = 1; k <= 24; k++) {
                        Vector2 q = GetSplinePointBezierCubic(p1, c1, c2, p2, k / 24.0f);
                        if (segment_distance(p, prev, q) < near) return i;
                        prev = q;
                }
        }
        return -1;
}

/* "int -> tick" above the middle of a wire with incompatible types */
static void
draw_wire_error(Vector2 a, Vector2 b, Li_Type out, Li_Type in)
{
        Vector2 c1, c2;
        wire_controls(a, b, &c1, &c2);
        Vector2 mid = GetSplinePointBezierCubic(a, c1, c2, b, 0.5f);

        const char *msg = TextFormat("%s -> %s", type_name(out), type_name(in));
        float fs        = cfg.canvas.pin_font_size;
        float pad       = 4;
        float tw        = measure_canvas_text(msg, fs);
        Rectangle r     = { mid.x - tw / 2 - pad, mid.y - fs - 2 * pad - cfg.canvas.wire_thickness - 4,
                            tw + 2 * pad, fs + 2 * pad };

        DrawRectangleRec(r, cfg.colors.error);
        draw_canvas_text(msg, r.x + pad, r.y + pad, fs, cfg.colors.error_text);
}

/* TINT overrides the type color (hover), NULL for none */
static void
draw_pin(Vector2 p, Li_Type type, bool connected, const Color *tint)
{
        float r = cfg.canvas.pin_radius;
        Color c = tint ? *tint : type_color(type);

        DrawCircleV(p, r, cfg.colors.square_border);
        if (connected)
                DrawCircleV(p, r * 0.7f, c);
        else
                DrawRing(p, r * 0.45f, r * 0.7f, 0, 360, 24, c);
}

/* A field at F showing VALUE (what's typed, if TYPING), with an arrow if
 * it's a list of OPTIONS */
static void
draw_value_field(Rectangle f, const char *value, bool typing, bool options)
{
        float fs     = cfg.canvas.pin_font_size;
        Color border = typing && field_edit.error ? cfg.colors.error :
                       typing                     ? cfg.colors.pin_hover :
                                                    cfg.colors.square_border;
        char buf[256], text[256];

        DrawRectangleRec(f, cfg.colors.field);
        DrawRectangleLinesEx(f, typing ? 2.0f : 1.0f, border);

        float arrow = options ? measure_canvas_text(FIELD_ARROW, fs) + 6 : 0;
        if (typing)
                snprintf(buf, sizeof buf, "%s%s", field_edit.text.buf, fmod(GetTime(), 1.0) < 0.5 ? "_" : "");
        else
                snprintf(buf, sizeof buf, "%s", value);
        fit_canvas_text(buf, text, sizeof text, f.width - 8 - arrow, fs);
        draw_canvas_text(text, f.x + 4, f.y + (f.height - fs) / 2, fs, cfg.colors.square_text);
        if (arrow) draw_canvas_text(FIELD_ARROW, f.x + f.width - arrow, f.y + (f.height - fs) / 2, fs, cfg.colors.square_text);
}

/* Bools: a round checkbox, filled when ON (clicked in the same box) */
static void
draw_checkbox(Rectangle f, bool on)
{
        Vector2 c = { f.x + f.width / 2, f.y + f.height / 2 };
        float r   = fminf(f.width, f.height) / 2;
        DrawCircleV(c, r, cfg.colors.square_border); /* the border: a circle under the field */
        DrawCircleV(c, r - 1, cfg.colors.field);
        if (on) DrawCircleV(c, r - 4, cfg.colors.square_text);
}

/* The typed value or chosen option of input I */
static void
draw_field(Square *s, int i)
{
        PinInfo *p = &s->inputs.items[i];
        char buf[256];

        if (p->kind == PIN_VALUE && p->type == LI_BOOL) {
                draw_checkbox(field_rect(s, i), p->value.type == LI_BOOL && p->value.b);
                return;
        }
        input_text(s, i, buf, sizeof buf, false);
        draw_value_field(field_rect(s, i), buf, field_typing(s, i, false), p->kind == PIN_OPTIONS);
}

/* The params, in rows at the bottom, under a line */
static void
draw_params(Square *s)
{
        float fs = cfg.canvas.pin_font_size;
        char buf[256];

        if (!s->params.count) return;
        Rectangle r = square_rect(s);
        float top   = r.y + r.height - params_height(s);
        DrawLineEx((Vector2) { r.x + 4, top }, (Vector2) { r.x + r.width - 4, top }, 1.0f, Fade(cfg.colors.square_border, 0.4f));
        for (int i = 0; i < s->params.count; i++) {
                Param *p = &s->params.items[i];
                draw_canvas_text(p->name, r.x + pin_label_gap(), param_y(s, i) - fs / 2, fs, cfg.colors.square_text);
                if (p->value.type == LI_BOOL) {
                        draw_checkbox(param_rect(s, i), p->value.b);
                        continue;
                }
                param_text(p, buf, sizeof buf, false);
                draw_value_field(param_rect(s, i), buf, field_typing(s, i, true), false);
        }
}

/* HOVER_TINT is the color of the hovered pin, if it's one of S */
static void
draw_square(Square *s, Hit hover, Color hover_tint)
{
        Rectangle r = square_rect(s);
        float fs    = cfg.canvas.pin_font_size;
        float gap   = pin_label_gap();

        DrawRectangleRec(r, cfg.colors.square);
        /* Glows when a tick reaches it */
        float g = glow(s->flash);
        if (g > 0) DrawRectangleLinesEx((Rectangle) { r.x - 5, r.y - 5, r.width + 10, r.height + 10 }, 4.0f, Fade(cfg.colors.flash, g));
        if (s->selected)
                DrawRectangleLinesEx((Rectangle) { r.x - 2, r.y - 2, r.width + 4, r.height + 4 }, 3.0f, cfg.colors.selection);
        else
                DrawRectangleLinesEx(r, 2.0f, cfg.colors.square_border);
        if (s->conflict) DrawRectangleLinesEx((Rectangle) { r.x - 4, r.y - 4, r.width + 8, r.height + 8 }, 3.0f, cfg.colors.error);
        /* Stepping: the one it runs next */
        if (step_next(s)) DrawRectangleLinesEx((Rectangle) { r.x - 7, r.y - 7, r.width + 14, r.height + 14 }, 4.0f, cfg.colors.pin_hover);

        for (int i = 0; i < s->inputs.count; i++) {
                PinInfo *in = &s->inputs.items[i];
                Vector2 p   = pin_pos(s, PART_INPUT, i);
                bool hov    = hover.square == s && hover.part == PART_INPUT && hover.pin == i;
                /* Options can't be wired: no pin */
                if (in->kind != PIN_OPTIONS) draw_pin(p, in->type, pin_connected(s, PART_INPUT, i), hov ? &hover_tint : NULL);
                draw_canvas_text(in->name, p.x + gap, p.y - fs / 2, fs, cfg.colors.square_text);
                if (field_visible(s, i)) draw_field(s, i);
        }
        for (int i = 0; i < s->outputs.count; i++) {
                PinInfo *out = &s->outputs.items[i];
                Vector2 p    = pin_pos(s, PART_OUTPUT, i);
                bool hov     = hover.square == s && hover.part == PART_OUTPUT && hover.pin == i;
                draw_pin(p, out->type, pin_connected(s, PART_OUTPUT, i), hov ? &hover_tint : NULL);
                draw_canvas_text(out->name, p.x - gap - measure_canvas_text(out->name, fs), p.y - fs / 2, fs,
                                 cfg.colors.square_text);
        }

        draw_params(s);

        /* Text in the band at the top. Cut to fit, so a long value doesn't
         * cover the pins or resize the square. */
        if (*s->text) {
                char text[sizeof s->text];
                fit_canvas_text(s->text, text, sizeof text, r.width - 8, fs);
                float tw = measure_canvas_text(text, fs);
                draw_canvas_text(text, r.x + (r.width - tw) / 2, r.y + (square_text_band() - fs) / 2 + 2, fs,
                                 cfg.colors.square_text);
        }

        /* Module name below, or what's wrong with it */
        float nfs         = cfg.canvas.name_font_size;
        const char *label = s->conflict ? "says another type or start than its copies" : s->module->label;
        float nw          = measure_canvas_text(label, nfs);
        draw_canvas_text(label, r.x + (r.width - nw) / 2, r.y + r.height + 4, nfs, s->conflict ? cfg.colors.error : cfg.colors.name);
}

void
draw_canvas(void)
{
        Rectangle r = canvas_rect();
        BeginScissorMode(r.x, r.y, r.width, r.height);
        BeginMode2D(camera);

        Vector2 world = GetScreenToWorld2D(GetMousePosition(), camera);
        Hit hover     = square_hit(world);
        Color tint    = cfg.colors.pin_hover;

        /* While wiring, inputs of another type light up as an error */
        if (drag.kind == DRAG_WIRE && hover.part == PART_INPUT) {
                if (!link_allowed(drag.square, drag.pin, hover.square, hover.pin))
                        hover.part = PART_NONE;
                else if (!types_compatible(pin_info(drag.square, PART_OUTPUT, drag.pin)->type,
                                           pin_info(hover.square, PART_INPUT, hover.pin)->type))
                        tint = cfg.colors.error;
        }

        draw_grid();

        /* Wires go below the squares. Their type is the output type. They
         * glow when something goes through them, and get thicker hovered. */
        int hovered = hover.part == PART_NONE && drag.kind == DRAG_NONE ? link_hit(world) : -1;
        for (int i = 0; i < links.count; i++) {
                Link *l   = &links.items[i];
                Square *a = square_get(l->from);
                Square *b = square_get(l->to);
                if (!a || !b) continue;
                Color c     = a->conflict ? cfg.colors.error : type_color(pin_info(a, PART_OUTPUT, l->from_pin)->type);
                float g     = glow(l->flash);
                float thick = cfg.canvas.wire_thickness * (1 + g + (i == hovered ? 0.6f : 0));
                draw_wire_ex(pin_pos(a, PART_OUTPUT, l->from_pin), pin_pos(b, PART_INPUT, l->to_pin),
                             ColorLerp(c, cfg.colors.flash, g), thick);
        }

        Da_foreach(s, squares)
        {
                draw_square(*s, hover, tint);
        }

        if (drag.kind == DRAG_BOX) {
                Rectangle b = drag_box();
                DrawRectangleRec(b, Fade(cfg.colors.selection, 0.12f));
                DrawRectangleLinesEx(b, 1.0f / camera.zoom, cfg.colors.selection);
        }

        /* Errors go above everything */
        Da_foreach(l, links)
        {
                if (link_valid(*l)) continue;
                Square *a = square_get(l->from);
                Square *b = square_get(l->to);
                if (!a || !b) continue;
                draw_wire_error(pin_pos(a, PART_OUTPUT, l->from_pin), pin_pos(b, PART_INPUT, l->to_pin),
                                pin_info(a, PART_OUTPUT, l->from_pin)->type, pin_info(b, PART_INPUT, l->to_pin)->type);
        }

        if (drag.kind == DRAG_WIRE) {
                Color c = type_color(pin_info(drag.square, PART_OUTPUT, drag.pin)->type);
                draw_wire(pin_pos(drag.square, PART_OUTPUT, drag.pin), world, c);
        }

        /* The wire the module search was opened with, until it's closed */
        Square *from = menu.depth && menu.wire_from >= 0 ? square_get(menu.wire_from) : NULL;
        if (from) {
                Color c = type_color(pin_info(from, PART_OUTPUT, menu.wire_pin)->type);
                draw_wire(pin_pos(from, PART_OUTPUT, menu.wire_pin), menu.world, c);
        }

        EndMode2D();
        EndScissorMode();
}

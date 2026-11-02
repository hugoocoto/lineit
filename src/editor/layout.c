/* layout.c - where the parts of a square are: its pins, fields and params */
#include "editor.h"

/* Height of the strip at the top where the text goes, above the pins */
float
square_text_band(void)
{
        return cfg.canvas.pin_font_size + 8;
}

/* Space between a pin and its name */
float
pin_label_gap(void)
{
        return cfg.canvas.pin_radius + 4;
}

static float
pins_label_width(PinList *l)
{
        float w = 0;
        Da_foreach(p, *l)
        {
                w = fmaxf(w, measure_canvas_text(p->name, cfg.canvas.pin_font_size));
        }
        return w;
}

/* Width of the field of input P, 0 if it has none */
float
field_width(PinInfo *p)
{
        float fs = cfg.canvas.pin_font_size;
        float w  = cfg.canvas.field_width;

        if (p->kind == PIN_WIRE) return 0;
        if (p->kind == PIN_VALUE && p->type == LI_BOOL) return fs + 6; /* a checkbox */
        for (int i = 0; i < p->option_count; i++)
                w = fmaxf(w, measure_canvas_text(p->options[i], fs) + measure_canvas_text(FIELD_ARROW, fs) + 16);
        return w;
}

/* Width of the field of a param: a checkbox for bools */
static float
param_field_width(Param *p)
{
        return p->value.type == LI_BOOL ? cfg.canvas.pin_font_size + 6 : cfg.canvas.field_width;
}

/* Height of the rows of the params, at the bottom */
float
params_height(Square *s)
{
        return s->params.count * cfg.canvas.pin_spacing;
}

/* Always a square: as tall as the text, the pins and the params need, and
 * as wide as the longest input or param name, its field and the longest
 * output name side by side */
float
square_side(Square *s)
{
        if (s->side) return s->side;

        s->label_w = pins_label_width(&s->inputs);
        s->field_w = 0;
        Da_foreach(p, s->inputs)
        {
                s->field_w = fmaxf(s->field_w, field_width(p));
        }
        Da_foreach(p, s->params)
        {
                s->label_w = fmaxf(s->label_w, measure_canvas_text(p->name, cfg.canvas.pin_font_size));
                s->field_w = fmaxf(s->field_w, param_field_width(p));
        }

        int pins     = Max(s->inputs.count, s->outputs.count);
        float height = square_text_band() + (pins + 1) * cfg.canvas.pin_spacing + params_height(s);
        float width  = s->label_w + (s->field_w ? s->field_w + 6 : 0) + pins_label_width(&s->outputs) +
                       2 * pin_label_gap() + 12;
        s->side      = fmaxf(fmaxf(height, width), cfg.canvas.square_size);
        return s->side;
}

Rectangle
square_rect(Square *s)
{
        float side = square_side(s);
        return (Rectangle) { s->pos.x, s->pos.y, side, side };
}

PinInfo *
pin_info(Square *s, Part part, int i)
{
        PinList *l = part == PART_INPUT ? &s->inputs : &s->outputs;
        return i >= 0 && i < l->count ? &l->items[i] : NULL;
}

/* Inputs on the left side, outputs on the right, centered vertically
 * between the text band and the params */
Vector2
pin_pos(Square *s, Part part, int i)
{
        float side = square_side(s);
        float band = square_text_band();
        float room = side - band - params_height(s);
        int count  = part == PART_INPUT ? s->inputs.count : s->outputs.count;
        float y    = s->pos.y + band + room / 2 + (i - (count - 1) / 2.0f) * cfg.canvas.pin_spacing;
        float x    = part == PART_INPUT ? s->pos.x : s->pos.x + side;
        return (Vector2) { x, y };
}

/* Field where the value of input I is typed or chosen, after the names */
Rectangle
field_rect(Square *s, int i)
{
        Vector2 p = pin_pos(s, PART_INPUT, i);
        float h   = cfg.canvas.pin_font_size + 6;
        square_side(s);
        return (Rectangle) { p.x + pin_label_gap() + s->label_w + 6, p.y - h / 2, field_width(&s->inputs.items[i]), h };
}

/* Where the text of field F goes: inside its padding, before the arrow of
 * a list of OPTIONS */
Rectangle
field_text_rect(Rectangle f, bool options)
{
        float fs    = cfg.canvas.pin_font_size;
        float arrow = options ? measure_canvas_text(FIELD_ARROW, fs) + 6 : 0;
        return (Rectangle) { f.x + 4, f.y, fmaxf(f.width - 8 - arrow, 0), f.height };
}

/* Middle of the row of param I, at the bottom of the square */
float
param_y(Square *s, int i)
{
        return s->pos.y + square_side(s) - params_height(s) + (i + 0.5f) * cfg.canvas.pin_spacing;
}

/* Field of param I, below the input fields */
Rectangle
param_rect(Square *s, int i)
{
        float h = cfg.canvas.pin_font_size + 6;
        square_side(s);
        return (Rectangle) { s->pos.x + pin_label_gap() + s->label_w + 6, param_y(s, i) - h / 2,
                             param_field_width(&s->params.items[i]), h };
}

/* Options are always shown, typed values only while nothing is plugged in */
bool
field_visible(Square *s, int i)
{
        PinInfo *p = &s->inputs.items[i];
        return p->kind == PIN_OPTIONS || (p->kind == PIN_VALUE && link_to(s->id, i) < 0);
}

/* Topmost square under world point P and which part of it was hit. */
Hit
square_hit(Vector2 p)
{
        /* A bit bigger than the drawn circle so pins are easy to grab */
        float r = cfg.canvas.pin_radius * 1.5f;

        for (int i = squares.count - 1; i >= 0; i--) {
                Square *s = squares.items[i];
                for (int j = 0; j < s->outputs.count; j++) {
                        if (CheckCollisionPointCircle(p, pin_pos(s, PART_OUTPUT, j), r))
                                return (Hit) { s, PART_OUTPUT, j };
                }
                for (int j = 0; j < s->inputs.count; j++) {
                        if (s->inputs.items[j].kind != PIN_OPTIONS && CheckCollisionPointCircle(p, pin_pos(s, PART_INPUT, j), r))
                                return (Hit) { s, PART_INPUT, j };
                        if (field_visible(s, j) && CheckCollisionPointRec(p, field_rect(s, j)))
                                return (Hit) { s, PART_FIELD, j };
                }
                for (int j = 0; j < s->params.count; j++) {
                        if (CheckCollisionPointRec(p, param_rect(s, j))) return (Hit) { s, PART_PARAM, j };
                }
                if (CheckCollisionPointRec(p, square_rect(s))) return (Hit) { s, PART_BODY, -1 };
        }
        return (Hit) { NULL, PART_NONE, -1 };
}

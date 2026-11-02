/* fields.c - typed values: params and input fields, and typing in them */
#include "editor.h"

/* Values (records, variables, lists, trees and li_format) are in value.c,
 * shared with the programs Compile makes */

/* V as text. EXACT keeps every digit of floats, for saving. */
static void
value_text(Li_Value v, char *buf, int size, bool exact)
{
        if (v.type == LI_FLOAT && exact) {
                /* The shortest that reads back the same: 0.1, not 0.10000000000000001 */
                for (int digits = 6; digits <= 17; digits++) {
                        snprintf(buf, size, "%.*g", digits, v.f);
                        if (strtod(buf, NULL) == v.f) break;
                }
        } else if (v.type == LI_NONE) {
                *buf = '\0';
        } else {
                li_format(v, buf, size);
        }
}

/* TEXT as TYPE in OUT (strings are not copied). False if it isn't one. */
static bool
value_parse_as(Li_Type type, const char *text, Li_Value *out)
{
        char *end;

        errno = 0;
        switch (type) {
        case LI_INT: {
                long v = strtol(text, &end, 0);
                if (end == text || *end || errno) return false;
                *out = li_int(v);
                return true;
        }
        case LI_FLOAT: {
                double v = strtod(text, &end);
                if (end == text || *end || errno) return false;
                *out = li_float(v);
                return true;
        }
        case LI_BOOL:
                if (!strcasecmp(text, "true") || !strcmp(text, "1") || !strcasecmp(text, "yes"))
                        *out = li_bool(true);
                else if (!strcasecmp(text, "false") || !strcmp(text, "0") || !strcasecmp(text, "no"))
                        *out = li_bool(false);
                else
                        return false;
                return true;
        case LI_STRING: *out = li_string(text); return true;
        case LI_REF: *out = li_var_ref(text); return true; /* a variable's name */
        case LI_NONE: *out = (Li_Value) { .type = LI_NONE }; return true;
        case LI_ANY:
                /* Whatever it looks like: 2 is an int, 2.5 a float, true a
                 * bool, nothing is no value, the rest a string */
                if (!*text) return value_parse_as(LI_NONE, text, out);
                if (value_parse_as(LI_INT, text, out) || value_parse_as(LI_FLOAT, text, out)) return true;
                if (!strcmp(text, "true") || !strcmp(text, "false")) return value_parse_as(LI_BOOL, text, out);
                return value_parse_as(LI_STRING, text, out);
        default: return false;
        }
}

void
param_text(Param *p, char *buf, int size, bool exact)
{
        value_text(p->value, buf, size, exact);
}

/* Parse TEXT as the type of param I of S and tell the module. Returns false
 * (and changes nothing) if it isn't valid. */
bool
param_set_text(Square *s, int i, const char *text)
{
        Param *p = &s->params.items[i];
        Li_Value v;

        if (!value_parse_as(p->value.type, text, &v)) return false;
        if (v.type == LI_STRING) {
                free(p->str);
                p->str = strdup(v.s);
                v.s    = p->str;
        }
        p->value = v;

        if (s->fn.param) s->fn.param(s, i);
        links_check(s);
        return true;
}

/* Text shown in the field of input I */
void
input_text(Square *s, int i, char *buf, int size, bool exact)
{
        PinInfo *p = &s->inputs.items[i];
        if (p->kind == PIN_OPTIONS)
                snprintf(buf, size, "%s", li_option(s, i));
        else
                value_text(p->value, buf, size, exact);
}

/* Set input I from TEXT: an option name, or a value of its type (any type
 * for LI_ANY inputs, TYPE if not LI_NONE). False if it isn't valid. */
bool
input_set_text(Square *s, int i, const char *text, Li_Type type)
{
        PinInfo *p = &s->inputs.items[i];
        Li_Value v;

        if (p->kind == PIN_OPTIONS) {
                for (int k = 0; k < p->option_count; k++) {
                        if (strcmp(p->options[k], text) == 0) {
                                p->value = li_int(k);
                                return true;
                        }
                }
                return false;
        }
        if (p->kind != PIN_VALUE) return false;
        if (type == LI_NONE) type = p->type;
        /* Typed strings are fine for string inputs, but an empty int is not */
        if (!value_parse_as(type, text, &v) || (v.type == LI_NONE && p->type != LI_ANY)) return false;
        pin_store(p, v);
        return true;
}

int
input_find(Square *s, const char *name)
{
        for (int i = 0; i < s->inputs.count; i++) {
                if (s->inputs.items[i].kind != PIN_WIRE && strcmp(s->inputs.items[i].name, name) == 0) return i;
        }
        return -1;
}

/* Input field, or param field, being typed in the square */
struct FieldEditState field_edit;

void
field_edit_start(Square *s, int i, bool param)
{
        /* Exact: a float shown as 3.14159 is edited as all of it, and stays it */
        char buf[sizeof field_edit.text.buf];
        if (param)
                param_text(&s->params.items[i], buf, sizeof buf, true);
        else
                input_text(s, i, buf, sizeof buf, true);
        text_input_set(&field_edit.text, buf);
        field_edit.square = s;
        field_edit.input  = i;
        field_edit.param  = param;
        field_edit.error  = false;
}

/* Field I of S (a param if PARAM) is being typed */
bool
field_typing(Square *s, int i, bool param)
{
        return field_edit.square == s && field_edit.input == i && field_edit.param == param;
}

/* Apply what is typed. On error it stays open. */
static bool
field_edit_apply(void)
{
        Square *s = field_edit.square;
        if (!s) return true;
        bool ok = field_edit.param ? param_set_text(s, field_edit.input, field_edit.text.buf) :
                                     input_set_text(s, field_edit.input, field_edit.text.buf, LI_NONE);
        if (!ok) {
                field_edit.error = true;
                return false;
        }
        field_edit.square = NULL;
        return true;
}

/* ---- The editor: the field being typed, all of it, at the bottom ---- */

/* A field is as wide as its square lets it be: what's typed in it is also
 * in a strip above the bar (and the output, if open), wrapped over as many
 * lines as it takes, with the square and the input it's for. */

static float
editor_pad(void)
{
        return cfg.bar.font_size * 0.5f;
}

static float
editor_line(void)
{
        return cfg.bar.font_size + 4;
}

/* Where its text wraps, given the editor's rect R */
static Rectangle
editor_text_rect(Rectangle r)
{
        float pad = editor_pad();
        return (Rectangle) { r.x + 2 * pad, r.y + pad + editor_line() + pad / 2, r.width - 4 * pad,
                             r.height - 2 * pad - editor_line() - pad / 2 };
}

/* The whole strip, empty if nothing's typed */
Rectangle
field_editor_rect(void)
{
        if (!field_edit.square) return (Rectangle) { 0 };
        float pad    = editor_pad();
        float bottom = output_rect().y;
        float w      = GetScreenWidth();
        float text   = text_input_wrapped_height(&field_edit.text, w - 4 * pad, cfg.bar.font_size, editor_line());
        /* At most half of what's above it: long text wraps in that */
        float h = fminf(2 * pad + editor_line() + pad / 2 + text, bottom / 2);
        return (Rectangle) { 0, bottom - h, w, h };
}

/* What it's for: the square, the input or param, and its type */
static const char *
editor_title(void)
{
        Square *s = field_edit.square;
        int i     = field_edit.input;
        if (field_edit.param) {
                Param *p = &s->params.items[i];
                return TextFormat("%s  >  %s (param, %s)", s->module->name, p->name, type_name(p->value.type));
        }
        PinInfo *p = &s->inputs.items[i];
        return TextFormat("%s  >  %s (%s)", s->module->name, p->name, type_name(p->type));
}

void
draw_field_editor(void)
{
        if (!field_edit.square) return;
        Rectangle r = field_editor_rect();
        float fs    = cfg.bar.font_size;
        float pad   = editor_pad();
        Color edge  = field_edit.error ? cfg.colors.error : cfg.colors.pin_hover;

        DrawRectangleRec(r, cfg.colors.bar);
        DrawRectangleLinesEx(r, 2.0f, edge);
        const char *keys = field_edit.error ? "not valid: fix it, or Esc" : "Enter applies, Esc cancels";
        float kw         = measure_text(keys, fs);
        draw_text(text_fit(editor_title(), fs, r.width - kw - 6 * pad), r.x + 2 * pad, r.y + pad, fs, cfg.colors.name);
        draw_text(keys, r.x + r.width - kw - 2 * pad, r.y + pad, fs, field_edit.error ? cfg.colors.error : cfg.colors.name);

        Rectangle t = editor_text_rect(r);
        BeginScissorMode(t.x, t.y, t.width + pad, t.height);
        text_input_draw_wrapped(&field_edit.text, t, fs, editor_line(), cfg.colors.bar_text);
        EndScissorMode();
}

/* A press being dragged, and where it was: the cursor follows the mouse there */
static enum { SELECT_NONE,
              SELECT_EDITOR,
              SELECT_FIELD } selecting;

/* Returns true if the field consumed the input this frame. */
bool
update_field(void)
{
        bool changed = false;

        if (!field_edit.square) return false;

        switch (text_input_update(&field_edit.text, &changed)) {
        case TEXT_DONE:
        case TEXT_NEXT: field_edit_apply(); break;
        case TEXT_CANCEL: field_edit.square = NULL; return true;
        case TEXT_TYPING: break;
        }
        if (changed) field_edit.error = false;

        /* Applied above (Enter, Tab): there's no field to click away from */
        if (!field_edit.square) {
                selecting = SELECT_NONE;
                return false;
        }
        Vector2 mouse  = GetMousePosition();
        Vector2 world  = GetScreenToWorld2D(mouse, camera);
        Square *s      = field_edit.square;
        bool options   = !field_edit.param && s->inputs.items[field_edit.input].kind == PIN_OPTIONS;
        Rectangle f    = field_edit.param ? param_rect(s, field_edit.input) : field_rect(s, field_edit.input);
        Rectangle ed   = field_editor_rect();
        Rectangle et   = editor_text_rect(ed);
        Rectangle ft   = field_text_rect(f, options);
        float fs       = cfg.bar.font_size;
        float canvasfs = cfg.canvas.pin_font_size;

        /* In the editor or the field: the cursor goes there, a drag selects */
        if (selecting && IsMouseButtonDown(MOUSE_BUTTON_LEFT)) {
                if (selecting == SELECT_EDITOR)
                        text_input_drag(&field_edit.text, text_input_hit_wrapped(&field_edit.text, et, fs, editor_line(), mouse));
                else
                        text_input_drag(&field_edit.text, text_input_hit(&field_edit.text, ft, canvasfs, true, world.x));
                return true;
        }
        selecting = SELECT_NONE;
        if (IsMouseButtonPressed(MOUSE_BUTTON_LEFT) || IsMouseButtonPressed(MOUSE_BUTTON_RIGHT)) {
                bool left = IsMouseButtonPressed(MOUSE_BUTTON_LEFT);
                if (CheckCollisionPointRec(mouse, ed)) {
                        if (left) text_input_click(&field_edit.text, text_input_hit_wrapped(&field_edit.text, et, fs, editor_line(), mouse));
                        selecting = left ? SELECT_EDITOR : SELECT_NONE;
                        return true;
                }
                if (CheckCollisionPointRec(world, f)) {
                        if (left) text_input_click(&field_edit.text, text_input_hit(&field_edit.text, ft, canvasfs, true, world.x));
                        selecting = left ? SELECT_FIELD : SELECT_NONE;
                        return true;
                }
                /* Clicking away applies what's valid, the click goes on */
                if (!field_edit_apply()) field_edit.square = NULL;
        }
        return false;
}

/* The list of an options input, opened by clicking its field */
Menu options_menu;
int options_input;

void
option_choose(int id, void *arg)
{
        Square *s  = square_get(id);
        PinInfo *p = s ? pin_info(s, PART_INPUT, options_input) : NULL;
        if (p && p->kind == PIN_OPTIONS) p->value = li_int((int) (intptr_t) arg);
}

int
param_find(Square *s, const char *name)
{
        for (int i = 0; i < s->params.count; i++) {
                if (strcmp(s->params.items[i].name, name) == 0) return i;
        }
        return -1;
}

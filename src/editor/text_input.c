/* text_input.c - typing text: one line, with a cursor and a selection,
 * drawn in a field (scrolled to the cursor) or wrapped in a box */
#include "editor.h"

void
text_input_set(TextInput *t, const char *text)
{
        t->len = snprintf(t->buf, sizeof t->buf, "%s", text);
        if (t->len >= (int) sizeof t->buf) t->len = sizeof t->buf - 1;
        t->cursor = t->anchor = t->len;
}

/* After T's buf was changed by hand: the cursor at its end */
void
text_input_sync(TextInput *t)
{
        t->len    = (int) strlen(t->buf);
        t->cursor = t->anchor = t->len;
}

static int
sel_start(const TextInput *t)
{
        return Min(t->cursor, t->anchor);
}

static int
sel_end(const TextInput *t)
{
        return Max(t->cursor, t->anchor);
}

/* The selection goes: what's typed replaces it */
static void
delete_range(TextInput *t, int from, int to)
{
        if (from >= to) return;
        memmove(t->buf + from, t->buf + to, t->len - to + 1);
        t->len -= to - from;
        t->cursor = t->anchor = from;
}

static bool
delete_selection(TextInput *t)
{
        if (t->cursor == t->anchor) return false;
        delete_range(t, sel_start(t), sel_end(t));
        return true;
}

/* TEXT at the cursor, as much as fits; only what a field can show (the
 * font only has ASCII), a new line as a space */
static bool
insert(TextInput *t, const char *text)
{
        bool any = delete_selection(t);
        for (const char *p = text; *p && t->len < (int) sizeof t->buf - 1; p++) {
                char c = *p == '\n' || *p == '\t' ? ' ' : *p;
                if (c == '\r' || (unsigned char) c < ' ' || (unsigned char) c >= 127) continue;
                memmove(t->buf + t->cursor + 1, t->buf + t->cursor, t->len - t->cursor + 1);
                t->buf[t->cursor++] = c;
                t->len++;
                any = true;
        }
        t->anchor = t->cursor;
        return any;
}

static bool
is_word(char c)
{
        return isalnum((unsigned char) c) || c == '_';
}

/* Where a word move from AT goes: back to the start of the word, or on to
 * its end */
static int
word_from(const TextInput *t, int at, int dir)
{
        if (dir < 0) {
                while (at > 0 && !is_word(t->buf[at - 1]))
                        at--;
                while (at > 0 && is_word(t->buf[at - 1]))
                        at--;
        } else {
                while (at < t->len && !is_word(t->buf[at]))
                        at++;
                while (at < t->len && is_word(t->buf[at]))
                        at++;
        }
        return at;
}

static bool
pressed(int key)
{
        return key_pressed(key) || IsKeyPressedRepeat(key);
}

/* The cursor to AT; with SELECT the selection grows to it */
static void
move_to(TextInput *t, int at, bool select)
{
        t->cursor = Clamp(at, 0, t->len);
        if (!select) t->anchor = t->cursor;
}

static void
copy_selection(const TextInput *t)
{
        if (t->cursor == t->anchor) return;
        char *s = strndup(t->buf + sel_start(t), sel_end(t) - sel_start(t));
        SetClipboardText(s);
        free(s);
}

/* Keys typed this frame. CHANGED is set when the text changes. */
TextResult
text_input_update(TextInput *t, bool *changed)
{
        int c;

        /* Ctrl+key is a shortcut, not a letter (but AltGr can come as
         * Ctrl+Alt, and that one types) */
        bool alt   = IsKeyDown(KEY_LEFT_ALT) || IsKeyDown(KEY_RIGHT_ALT);
        bool ctrl  = (IsKeyDown(KEY_LEFT_CONTROL) || IsKeyDown(KEY_RIGHT_CONTROL)) && !alt;
        bool shift = IsKeyDown(KEY_LEFT_SHIFT) || IsKeyDown(KEY_RIGHT_SHIFT);
        t->cursor  = Clamp(t->cursor, 0, t->len);
        t->anchor  = Clamp(t->anchor, 0, t->len);

        while ((c = GetCharPressed())) {
                if (ctrl || c < ' ' || c >= 127) continue;
                char s[2] = { (char) c, '\0' };
                *changed  = insert(t, s) || *changed;
        }

        /* With Ctrl, a word */
        if (pressed(KEY_BACKSPACE)) {
                if (delete_selection(t)) {
                        *changed = true;
                } else if (t->cursor > 0) {
                        delete_range(t, ctrl ? word_from(t, t->cursor, -1) : t->cursor - 1, t->cursor);
                        *changed = true;
                }
        }
        if (pressed(KEY_DELETE)) {
                if (delete_selection(t)) {
                        *changed = true;
                } else if (t->cursor < t->len) {
                        delete_range(t, t->cursor, ctrl ? word_from(t, t->cursor, 1) : t->cursor + 1);
                        *changed = true;
                }
        }

        if (pressed(KEY_LEFT)) {
                if (t->cursor != t->anchor && !shift && !ctrl)
                        move_to(t, sel_start(t), false);
                else
                        move_to(t, ctrl ? word_from(t, t->cursor, -1) : t->cursor - 1, shift);
        }
        if (pressed(KEY_RIGHT)) {
                if (t->cursor != t->anchor && !shift && !ctrl)
                        move_to(t, sel_end(t), false);
                else
                        move_to(t, ctrl ? word_from(t, t->cursor, 1) : t->cursor + 1, shift);
        }
        if (key_pressed(KEY_HOME)) move_to(t, 0, shift);
        if (key_pressed(KEY_END)) move_to(t, t->len, shift);

        if (ctrl && key_pressed(KEY_A)) {
                t->anchor = 0;
                t->cursor = t->len;
        }
        if (ctrl && (key_pressed(KEY_C) || key_pressed(KEY_X))) copy_selection(t);
        if (ctrl && key_pressed(KEY_X)) *changed = delete_selection(t) || *changed;
        if (ctrl && key_pressed(KEY_V)) {
                const char *clip = GetClipboardText();
                if (clip) *changed = insert(t, clip) || *changed;
        }

        if (key_pressed(KEY_ENTER) || key_pressed(KEY_KP_ENTER)) return TEXT_DONE;
        if (key_pressed(KEY_TAB)) return TEXT_NEXT;
        if (key_pressed(KEY_ESCAPE)) return TEXT_CANCEL;
        return TEXT_TYPING;
}

/* ---- Drawing ---- */

/* Width of N chars of S, in the font of the screen or of the canvas */
static float
width_of(const char *s, int n, float fs, bool canvas)
{
        char buf[sizeof((TextInput *) 0)->buf];
        n = Clamp(n, 0, (int) sizeof buf - 1);
        memcpy(buf, s, n);
        buf[n] = '\0';
        return canvas ? measure_canvas_text(buf, fs) : measure_text(buf, fs);
}

static void
text_at(const char *s, int n, float x, float y, float fs, Color color, bool canvas)
{
        char buf[sizeof((TextInput *) 0)->buf];
        n = Clamp(n, 0, (int) sizeof buf - 1);
        memcpy(buf, s, n);
        buf[n] = '\0';
        if (canvas)
                draw_canvas_text(buf, x, y, fs, color);
        else
                draw_text(buf, x, y, fs, color);
}

/* The first char shown in a field W wide: from the start while the cursor
 * fits, else as much before the cursor as fits */
static int
field_start(const TextInput *t, float w, float fs, bool canvas)
{
        /* The fewer chars, the narrower: the first start that fits */
        int lo = 0, hi = t->cursor;
        while (lo < hi) {
                int mid = (lo + hi) / 2;
                if (width_of(t->buf + mid, t->cursor - mid, fs, canvas) > w)
                        lo = mid + 1;
                else
                        hi = mid;
        }
        return lo;
}

/* How many chars from START fit in W */
static int
fit_from(const TextInput *t, int start, float w, float fs, bool canvas)
{
        int lo = 0, hi = t->len - start;
        while (lo < hi) {
                int mid = (lo + hi + 1) / 2;
                if (width_of(t->buf + start, mid, fs, canvas) <= w)
                        lo = mid;
                else
                        hi = mid - 1;
        }
        return lo;
}

static bool
caret_on(void)
{
        return fmod(GetTime(), 1.0) < 0.5;
}

/* Chars [FROM, FROM + N) of T at X, Y, with the part of the selection in
 * them behind them and the cursor if it's there */
static void
draw_run(const TextInput *t, int from, int n, float x, float y, float fs, Color color, bool canvas, bool caret)
{
        int a = Max(sel_start(t), from), b = Min(sel_end(t), from + n);
        if (a < b) {
                float sx = x + width_of(t->buf + from, a - from, fs, canvas);
                float sw = width_of(t->buf + a, b - a, fs, canvas);
                DrawRectangleRec((Rectangle) { sx, y, sw, fs }, Fade(cfg.colors.selection, 0.45f));
        }
        text_at(t->buf + from, n, x, y, fs, color, canvas);
        if (caret && t->cursor >= from && t->cursor <= from + n && caret_on()) {
                float cx = x + width_of(t->buf + from, t->cursor - from, fs, canvas);
                DrawLineEx((Vector2) { cx, y }, (Vector2) { cx, y + fs }, fmaxf(1.0f, fs / 12), color);
        }
}

/* T in field R, on one line: scrolled so the cursor shows */
void
text_input_draw(const TextInput *t, Rectangle r, float fs, Color color, bool canvas)
{
        float w   = r.width - fs / 6; /* room for the cursor */
        int start = field_start(t, w, fs, canvas);
        int n     = fit_from(t, start, w, fs, canvas);
        draw_run(t, start, n, r.x, r.y + (r.height - fs) / 2, fs, color, canvas, true);
}

/* Where a click at X in field R puts the cursor */
int
text_input_hit(const TextInput *t, Rectangle r, float fs, bool canvas, float x)
{
        float w   = r.width - fs / 6;
        int start = field_start(t, w, fs, canvas);
        int n     = fit_from(t, start, w, fs, canvas);
        for (int i = 0; i < n; i++) {
                float mid = r.x + (width_of(t->buf + start, i, fs, canvas) + width_of(t->buf + start, i + 1, fs, canvas)) / 2;
                if (x < mid) return start + i;
        }
        return start + n;
}

/* Where the lines of T wrapped in W start, in STARTS (MAX at most), with
 * the screen's font: at a space if a word fits, else where it's full.
 * How many lines. */
static int
wrap(const TextInput *t, float w, float fs, int *starts, int max)
{
        int lines = 0, at = 0;
        do {
                starts[lines++] = at;
                int n           = fit_from(t, at, w, fs, false);
                if (n == 0 && at < t->len) n = 1; /* narrower than a char */
                if (at + n < t->len) {
                        int space = n;
                        while (space > 0 && t->buf[at + space - 1] != ' ')
                                space--;
                        if (space > 0) n = space;
                }
                at += n;
        } while (at < t->len && lines < max);
        return lines;
}

#define WRAP_MAX 64

/* How tall T is wrapped in W, with lines LH apart */
float
text_input_wrapped_height(const TextInput *t, float w, float fs, float lh)
{
        int starts[WRAP_MAX];
        return wrap(t, w, fs, starts, WRAP_MAX) * lh;
}

/* T wrapped in box R, lines LH apart, from its top */
void
text_input_draw_wrapped(const TextInput *t, Rectangle r, float fs, float lh, Color color)
{
        int starts[WRAP_MAX];
        int lines = wrap(t, r.width, fs, starts, WRAP_MAX);
        for (int i = 0; i < lines; i++) {
                int end = i + 1 < lines ? starts[i + 1] : t->len;
                /* The cursor at a line break is at the start of the next */
                bool caret = t->cursor < end || i + 1 == lines;
                draw_run(t, starts[i], end - starts[i], r.x, r.y + i * lh, fs, color, false, caret);
        }
}

/* Where a click at P in box R (as text_input_draw_wrapped) puts the cursor */
int
text_input_hit_wrapped(const TextInput *t, Rectangle r, float fs, float lh, Vector2 p)
{
        int starts[WRAP_MAX];
        int lines = wrap(t, r.width, fs, starts, WRAP_MAX);
        int line  = Clamp((int) ((p.y - r.y) / lh), 0, lines - 1);
        int end   = line + 1 < lines ? starts[line + 1] : t->len;
        int from  = starts[line];
        for (int i = from; i < end; i++) {
                float mid = r.x + (width_of(t->buf + from, i - from, fs, false) + width_of(t->buf + from, i - from + 1, fs, false)) / 2;
                if (p.x < mid) return i;
        }
        /* After the last char of a wrapped line: before the next one's first */
        return line + 1 < lines && end > from ? end - 1 : end;
}

/* A press at the cursor's place AT: with Shift, the selection grows to
 * it; a drag goes on selecting (text_input_drag) */
void
text_input_click(TextInput *t, int at)
{
        bool shift = IsKeyDown(KEY_LEFT_SHIFT) || IsKeyDown(KEY_RIGHT_SHIFT);
        move_to(t, at, shift);
}

void
text_input_drag(TextInput *t, int at)
{
        move_to(t, at, true);
}

/* text_input.c - typing text: one line, with a cursor */
#include "editor.h"

void
text_input_set(TextInput *t, const char *text)
{
        t->len = snprintf(t->buf, sizeof t->buf, "%s", text);
        if (t->len >= (int) sizeof t->buf) t->len = sizeof t->buf - 1;
}

/* Keys typed this frame. CHANGED is set when the text changes. */
TextResult
text_input_update(TextInput *t, bool *changed)
{
        int c;

        /* The font only has ASCII. Ctrl+key is a shortcut, not a letter (but
         * AltGr can come as Ctrl+Alt, and that one types). */
        bool ctrl = (IsKeyDown(KEY_LEFT_CONTROL) || IsKeyDown(KEY_RIGHT_CONTROL)) &&
                    !IsKeyDown(KEY_LEFT_ALT) && !IsKeyDown(KEY_RIGHT_ALT);
        while ((c = GetCharPressed())) {
                if (!ctrl && c >= ' ' && c < 127 && t->len < (int) sizeof t->buf - 1) {
                        t->buf[t->len++] = c;
                        t->buf[t->len]   = '\0';
                        *changed         = true;
                }
        }
        if ((key_pressed(KEY_BACKSPACE) || IsKeyPressedRepeat(KEY_BACKSPACE)) && t->len > 0) {
                t->buf[--t->len] = '\0';
                *changed         = true;
        }

        if (key_pressed(KEY_ENTER) || key_pressed(KEY_KP_ENTER)) return TEXT_DONE;
        if (key_pressed(KEY_TAB)) return TEXT_NEXT;
        if (key_pressed(KEY_ESCAPE)) return TEXT_CANCEL;
        return TEXT_TYPING;
}

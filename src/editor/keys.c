/* keys.c - the keys pressed this frame */
#include "editor.h"

/* Keys pressed this frame, from raylib's queue of presses: IsKeyPressed
 * compares the key now with the last frame, so a key that goes down and up
 * between two frames is lost. Tap-hold keys (Esc on tap, Ctrl on hold) do
 * exactly that: on release they send the tap's down and up together. */
static bool keys_pressed[512];

/* Once per frame, before anything reads the keyboard */
void
keys_poll(void)
{
        int key;
        memset(keys_pressed, 0, sizeof keys_pressed);
        while ((key = GetKeyPressed()))
                if (key > 0 && key < countof(keys_pressed)) keys_pressed[key] = true;
}

/* Like IsKeyPressed, but it doesn't miss quick taps */
bool
key_pressed(int key)
{
        return key > 0 && key < countof(keys_pressed) && keys_pressed[key];
}

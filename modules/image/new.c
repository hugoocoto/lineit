/* image = a new one, "width" x "height" pixels, all of "color" (hex,
 * "#ff8800", or with alpha "#ff880080"). Pure: computed when read, no
 * ticks needed. */
#define IMG_KEEP
#include "image.h"

enum { IN_WIDTH,
       IN_HEIGHT,
       IN_COLOR };

void
module_init(Li_Node *n)
{
        li_input_value(n, "width", LI_INT, li_int(256));
        li_input_value(n, "height", LI_INT, li_int(256));
        li_input_value(n, "color", LI_STRING, li_string("#ffffff"));
        img_out(n, "image");
}

static void
make(Li_Node *n)
{
        long w = img_long(li_get(n, IN_WIDTH), 0);
        long h = img_long(li_get(n, IN_HEIGHT), 0);
        Color c;

        if (img_unchanged(n)) return;
        if (w < 1 || h < 1 || w > 16384 || h > 16384 || !img_color(li_get(n, IN_COLOR), &c)) {
                img_none(n, 0);
                li_image(n, NULL, 0, 0);
                li_text(n, "no image");
                return;
        }
        img_set(n, 0, GenImageColor((int) w, (int) h, c));
        li_text(n, "%ldx%ld", w, h);
}

void
module_eval(Li_Node *n)
{
        pthread_mutex_lock(&img_lock);
        make(n);
        pthread_mutex_unlock(&img_lock);
}

/* Shown before Run too */
void
module_draw(Li_Node *n)
{
        module_eval(n);
}

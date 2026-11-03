/* color = the pixel of image at "x", "y" (its top left corner is 0, 0),
 * in hex: "#rrggbbaa". Nothing if it's outside. Pure: computed when read,
 * no ticks needed. */
#define IMG_KEEP
#include "image.h"
#include <stdio.h>

enum { IN_IMAGE,
       IN_X,
       IN_Y };

void
module_init(Li_Node *n)
{
        img_in(n, "image");
        li_input_value(n, "x", LI_INT, li_int(0));
        li_input_value(n, "y", LI_INT, li_int(0));
        li_output(n, "color", LI_STRING);
}

void
module_eval(Li_Node *n)
{
        long x = img_long(li_get(n, IN_X), -1);
        long y = img_long(li_get(n, IN_Y), -1);
        char hex[16];
        Image img;

        if (img_unchanged(n)) return;
        if (!img_get(n, IN_IMAGE, &img)) {
                img_none(n, 0);
                li_text(n, "no image");
                return;
        }
        if (x < 0 || y < 0 || x >= img.width || y >= img.height) {
                img_none(n, 0);
                li_text(n, "outside");
        } else {
                const unsigned char *p = (unsigned char *) img.data + (y * img.width + x) * 4;
                snprintf(hex, sizeof hex, "#%02x%02x%02x%02x", p[0], p[1], p[2], p[3]);
                li_set(n, 0, li_string(hex));
                li_text(n, "%s", hex);
        }
        UnloadImage(img);
}

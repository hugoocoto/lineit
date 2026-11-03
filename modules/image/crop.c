/* The part of image from "x", "y" (its top left corner is 0, 0) of
 * "width" x "height" pixels. 0 in width or height goes to the edge. Cut to
 * the image. */
#define IMG_FILTER
#include "image.h"

enum { IN_IMAGE,
       IN_X,
       IN_Y,
       IN_WIDTH,
       IN_HEIGHT };

void
module_init(Li_Node *n)
{
        img_in(n, "image");
        li_input_value(n, "x", LI_INT, li_int(0));
        li_input_value(n, "y", LI_INT, li_int(0));
        li_input_value(n, "width", LI_INT, li_int(0));
        li_input_value(n, "height", LI_INT, li_int(0));
        img_out(n, "image");
}

static long
clamp(long v, long lo, long hi)
{
        return v < lo ? lo : v > hi ? hi :
                                      v;
}

static void
edit(Li_Node *n, Image *img)
{
        long x = clamp(img_long(li_get(n, IN_X), 0), 0, img->width - 1);
        long y = clamp(img_long(li_get(n, IN_Y), 0), 0, img->height - 1);
        long w = img_long(li_get(n, IN_WIDTH), 0);
        long h = img_long(li_get(n, IN_HEIGHT), 0);

        w = w <= 0 ? img->width - x : clamp(w, 1, img->width - x);
        h = h <= 0 ? img->height - y : clamp(h, 1, img->height - y);
        ImageCrop(img, (Rectangle) { (float) x, (float) y, (float) w, (float) h });
}

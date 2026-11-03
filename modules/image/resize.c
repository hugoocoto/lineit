/* image, scaled to "width" x "height". 0 in one of them keeps the image's
 * proportions, 0 in both keeps its size. "smooth" blends pixels (bicubic),
 * "pixelated" keeps them sharp (nearest), for pixel art. */
#define IMG_FILTER
#include "image.h"

enum { IN_IMAGE,
       IN_WIDTH,
       IN_HEIGHT,
       IN_HOW };

void
module_init(Li_Node *n)
{
        img_in(n, "image");
        li_input_value(n, "width", LI_INT, li_int(0));
        li_input_value(n, "height", LI_INT, li_int(0));
        li_input_options(n, "how", "smooth|pixelated", 0);
        img_out(n, "image");
}

static void
edit(Li_Node *n, Image *img)
{
        long w = img_long(li_get(n, IN_WIDTH), 0);
        long h = img_long(li_get(n, IN_HEIGHT), 0);

        if (w <= 0 && h <= 0) return;
        if (w <= 0) w = (long) img->width * h / img->height;
        if (h <= 0) h = (long) img->height * w / img->width;
        if (w < 1) w = 1;
        if (h < 1) h = 1;
        if (w > 16384 || h > 16384) return;
        if (img_long(li_get(n, IN_HOW), 0) == 1)
                ImageResizeNN(img, (int) w, (int) h);
        else
                ImageResize(img, (int) w, (int) h);
}

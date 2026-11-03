/* width and height of "image", in pixels: 0 if it isn't one. Pure:
 * computed when read, no ticks needed. */
#define IMG_KEEP
#include "image.h"

void
module_init(Li_Node *n)
{
        img_in(n, "image");
        li_output(n, "width", LI_INT);
        li_output(n, "height", LI_INT);
}

void
module_eval(Li_Node *n)
{
        Image img = { 0 };
        if (img_unchanged(n)) return;
        if (img_get(n, 0, &img)) UnloadImage(img);
        li_set(n, 0, li_int(img.width));
        li_set(n, 1, li_int(img.height));
        li_text(n, "%dx%d", img.width, img.height);
}

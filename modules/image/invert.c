/* image, with its colors inverted, like a negative. Alpha stays. */
#define IMG_FILTER
#include "image.h"

void
module_init(Li_Node *n)
{
        img_in(n, "image");
        img_out(n, "image");
}

static void
edit(Li_Node *n, Image *img)
{
        (void) n;
        ImageColorInvert(img);
}

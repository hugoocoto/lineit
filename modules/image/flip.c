/* image, mirrored: left to right (horizontal) or top to bottom
 * (vertical) */
#define IMG_FILTER
#include "image.h"

enum { IN_IMAGE,
       IN_WAY };

void
module_init(Li_Node *n)
{
        img_in(n, "image");
        li_input_options(n, "way", "horizontal|vertical", 0);
        img_out(n, "image");
}

static void
edit(Li_Node *n, Image *img)
{
        if (img_long(li_get(n, IN_WAY), 0) == 1)
                ImageFlipVertical(img);
        else
                ImageFlipHorizontal(img);
}

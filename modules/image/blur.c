/* image, blurred (gaussian) by "radius" pixels */
#define IMG_FILTER
#include "image.h"

enum { IN_IMAGE,
       IN_RADIUS };

void
module_init(Li_Node *n)
{
        img_in(n, "image");
        li_input_value(n, "radius", LI_INT, li_int(4));
        img_out(n, "image");
}

static void
edit(Li_Node *n, Image *img)
{
        long r = img_long(li_get(n, IN_RADIUS), 0);
        if (r > 0) ImageBlurGaussian(img, r > 256 ? 256 : (int) r);
}

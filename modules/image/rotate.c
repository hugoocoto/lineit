/* image, turned "degrees" clockwise. Other than 90, 180 or 270 it grows
 * to fit, with transparent corners. */
#define IMG_FILTER
#include "image.h"
#include <math.h>

enum { IN_IMAGE,
       IN_DEGREES };

void
module_init(Li_Node *n)
{
        img_in(n, "image");
        li_input_value(n, "degrees", LI_FLOAT, li_float(90));
        img_out(n, "image");
}

static void
edit(Li_Node *n, Image *img)
{
        double d = fmod(img_double(li_get(n, IN_DEGREES), 0), 360);
        if (d < 0) d += 360;
        /* Exact, without the blur of ImageRotate */
        if (d == 90)
                ImageRotateCW(img);
        else if (d == 270)
                ImageRotateCCW(img);
        else if (d == 180) {
                ImageFlipVertical(img);
                ImageFlipHorizontal(img);
        } else if (d != 0)
                ImageRotate(img, (int) lround(d));
}

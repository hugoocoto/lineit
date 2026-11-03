/* image, with more contrast ("amount" up to 100) or less (down to -100) */
#define IMG_FILTER
#include "image.h"

enum { IN_IMAGE,
       IN_AMOUNT };

void
module_init(Li_Node *n)
{
        img_in(n, "image");
        li_input_value(n, "amount", LI_FLOAT, li_float(30));
        img_out(n, "image");
}

static void
edit(Li_Node *n, Image *img)
{
        double a = img_double(li_get(n, IN_AMOUNT), 0);
        ImageColorContrast(img, (float) (a < -100 ? -100 : a > 100 ? 100 :
                                                                     a));
}

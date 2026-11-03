/* image, lighter ("amount" up to 255) or darker (down to -255) */
#define IMG_FILTER
#include "image.h"

enum { IN_IMAGE,
       IN_AMOUNT };

void
module_init(Li_Node *n)
{
        img_in(n, "image");
        li_input_value(n, "amount", LI_INT, li_int(40));
        img_out(n, "image");
}

static void
edit(Li_Node *n, Image *img)
{
        long a = img_long(li_get(n, IN_AMOUNT), 0);
        ImageColorBrightness(img, a < -255 ? -255 : a > 255 ? 255 :
                                                              (int) a);
}

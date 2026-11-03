/* image, multiplied by "color", like through colored glass: a hex color,
 * "#ff8800" or "ff8800" (with alpha, "#ff880080"). White changes nothing. */
#define IMG_FILTER
#include "image.h"

enum { IN_IMAGE,
       IN_COLOR };

void
module_init(Li_Node *n)
{
        img_in(n, "image");
        li_input_value(n, "color", LI_STRING, li_string("#ff8800"));
        img_out(n, "image");
}

static void
edit(Li_Node *n, Image *img)
{
        Li_Value v = li_get(n, IN_COLOR);
        Color c;
        if (img_color(v, &c)) ImageColorTint(img, c);
}

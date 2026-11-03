/* On tick, writes "image" to the file at "path", as png, bmp, qoi, jpg or
 * tga, from its extension (built with make RAYLIB=repo, only the first
 * three). Says if it could in "ok". Choose file... in its menu
 * picks the path. A relative path is relative to the canvas's folder. */
#include "image.h"

enum { IN_TICK,
       IN_IMAGE,
       IN_PATH };
enum { OUT_THEN,
       OUT_OK };

void
module_init(Li_Node *n)
{
        li_input(n, "tick", LI_TICK);
        img_in(n, "image");
        li_input_value(n, "path", LI_STRING, li_string("image.png"));
        li_output(n, "then", LI_TICK);
        li_output(n, "ok", LI_BOOL);
}

/* Why it can't, NULL if it did */
static const char *
save(Li_Node *n)
{
        Li_Value p       = li_get(n, IN_PATH);
        const char *path = p.type == LI_STRING && p.s ? p.s : "";
        Image img;

        if (!*path) return "no path";
        if (!IsFileExtension(path, IMG_WRITES)) return "not .png, .bmp, .qoi, .jpg or .tga";
        if (!img_get(n, IN_IMAGE, &img)) return "no image";
        img_quiet();
        bool ok = ExportImage(img, li_path(n, path)); /* relative to the canvas */
        UnloadImage(img);
        return ok ? NULL : "can't write it";
}

void
module_tick(Li_Node *n, int input)
{
        (void) input;
        const char *why = save(n);
        if (why) {
                li_print(n, "image/save: %s", why);
                li_text(n, "%s", why);
        } else
                li_text(n, "saved");
        li_set(n, OUT_OK, li_bool(!why));
        li_tick(n, OUT_THEN);
}

/* What the image/ modules share: an image on a wire, and raylib to edit it.

   An image is a value of the type image/image (image.type.lua next to
   this), a bytes type: a header (IMG_MAGIC, width, height, id) and then
   its pixels, RGBA, 8 bits each, row by row. It's never changed once made,
   so its copies share it, and it can go in variables and lists. The id is
   new for each image a square makes: a square tells an image it already
   had by it. Squares with an image out show it (li_image). The squares
   edit them with raylib's Image* functions: the editor
   has raylib in it (they find it there), and a compiled program links the
   copy Compile's runtime has (module_libs). */
#ifndef IMAGE_H_
#define IMAGE_H_

#include "lineit.h"
#include <pthread.h>
#include <raylib.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#define IMG_MAGIC "LIMG"

/* What raylib can write, built with the formats lineit's Makefile adds
 * (RAYLIB_FORMATS; the raylib in the repo, make RAYLIB=repo, has only png,
 * bmp and qoi).
 * Others aren't even tried: raylib would say so in the output. */
#define IMG_WRITES ".png;.bmp;.qoi;.tga;.jpg;.jpeg"

typedef struct ImgHeader {
        char magic[4];
        int32_t width, height;
        uint32_t unused;
        uint64_t id;
} ImgHeader;

/* The type of images, from image.type.lua. Without it, plain bytes. */
static inline Li_Type
img_type(void)
{
        Li_Type t = li_type_named("image/image");
        return t == LI_NONE ? LI_BYTES : t;
}

/* An image input, and an image output, with the room to show it */
static inline int
img_in(Li_Node *n, const char *name)
{
        return li_input(n, name, img_type());
}

static inline int
img_out(Li_Node *n, const char *name)
{
        li_image(n, NULL, 0, 0);
        return li_output(n, name, img_type());
}

/* A new id: this square's (its globals are its own, so its counter's
 * address is too), and how many it made */
static uint64_t img_made;

static inline uint64_t
img_new_id(void)
{
        return (uint64_t) (uintptr_t) &img_made << 24 ^ ++img_made;
}

/* The header of V, false if it isn't an image */
static inline bool
img_header(Li_Value v, ImgHeader *h)
{
        const unsigned char *data = li_bytes_data(v);
        size_t size               = li_bytes_size(v);
        if (!data || size < sizeof *h) return false;
        memcpy(h, data, sizeof *h);
        return !memcmp(h->magic, IMG_MAGIC, 4) && h->width > 0 && h->height > 0 &&
               size == sizeof *h + (size_t) h->width * h->height * 4;
}

/* A compiled program has raylib's default, every INFO line; the editor
 * says only warnings already */
static inline void
img_quiet(void)
{
        SetTraceLogLevel(LOG_WARNING);
}

/* The image on INPUT, as a raylib Image of its own (UnloadImage it), RGBA.
 * False if what's there isn't one. */
static inline bool
img_get(Li_Node *n, int input, Image *out)
{
        Li_Value v = li_get(n, input);
        ImgHeader h;

        if (!img_header(v, &h)) return false;
        size_t size  = li_bytes_size(v) - sizeof h;
        void *pixels = RL_MALLOC(size);
        if (!pixels) return false;
        memcpy(pixels, li_bytes_data(v) + sizeof h, size);
        *out = (Image) { pixels, h.width, h.height, 1, PIXELFORMAT_UNCOMPRESSED_R8G8B8A8 };
        return true;
}

/* IMG as an image value: yours, li_drop it. IMG stays IMG's. */
static inline Li_Value
img_value(Image img)
{
        Li_Value none = { .type = LI_NONE };
        if (!img.data || img.width <= 0 || img.height <= 0) return none;

        /* Already RGBA (what the filters have), no copy to make it one */
        bool copy  = img.format != PIXELFORMAT_UNCOMPRESSED_R8G8B8A8;
        Image rgba = copy ? ImageCopy(img) : img;
        if (!rgba.data) return none;
        if (copy) ImageFormat(&rgba, PIXELFORMAT_UNCOMPRESSED_R8G8B8A8);
        size_t pixels       = (size_t) rgba.width * rgba.height * 4;
        ImgHeader h         = { { IMG_MAGIC[0], IMG_MAGIC[1], IMG_MAGIC[2], IMG_MAGIC[3] }, rgba.width, rgba.height, 0, img_new_id() };
        Li_Value v          = li_bytes(NULL, sizeof h + pixels);
        v.type              = img_type(); /* the same buffer, of another type */
        unsigned char *data = li_bytes_data(v);
        if (data) {
                memcpy(data, &h, sizeof h);
                memcpy(data + sizeof h, rgba.data, pixels);
        }
        if (copy) UnloadImage(rgba);
        return v;
}

/* Shows V in the square, nothing if it isn't an image */
static inline void
img_show(Li_Node *n, Li_Value v)
{
        ImgHeader h;
        if (img_header(v, &h))
                li_image(n, li_bytes_data(v) + sizeof h, h.width, h.height);
        else
                li_image(n, NULL, 0, 0);
}

/* Sets OUTPUT to IMG, shows it, and frees IMG */
static inline void
img_set(Li_Node *n, int output, Image img)
{
        Li_Value v = img_value(img);
        li_set(n, output, v);
        img_show(n, v);
        li_drop(&v);
        UnloadImage(img);
}

static inline void
img_none(Li_Node *n, int output)
{
        li_set(n, output, (Li_Value) { .type = LI_NONE });
}

static inline long
img_long(Li_Value v, long otherwise)
{
        if (v.type == LI_INT) return v.i;
        if (v.type == LI_FLOAT) return (long) v.f;
        if (v.type == LI_BOOL) return v.b;
        return otherwise;
}

static inline double
img_double(Li_Value v, double otherwise)
{
        if (v.type == LI_FLOAT) return v.f;
        if (v.type == LI_INT) return (double) v.i;
        return otherwise;
}

/* The color in V, a string: RRGGBB or RRGGBBAA in hex, with or without
 * '#'. False if it isn't one. */
static inline bool
img_color(Li_Value v, Color *c)
{
        char *end;
        const char *s = v.type == LI_STRING && v.s ? v.s : "";
        if (*s == '#') s++;
        size_t len = strlen(s);
        if (len != 6 && len != 8) return false;
        unsigned long x = strtoul(s, &end, 16);
        if (*end) return false;
        if (len == 6) x = x << 8 | 0xff;
        *c = (Color) { x >> 24 & 0xff, x >> 16 & 0xff, x >> 8 & 0xff, x & 0xff };
        return true;
}

/* Compile's runtime has a copy of raylib (see the Makefile) */
const char *
module_libs(void)
{
        return "$(RT)/libraylib.a -lX11 -lm";
}

#endif /* IMAGE_H_ */

/* Squares that show their value (string/format...) read it every frame,
 * and every read computes it again: a square that defines IMG_KEEP before
 * including this keeps what it made last, and img_unchanged says if its
 * inputs are the same as then (images by their id). Same: its outputs
 * still have what it set, there's nothing to do. Run empties outputs: it
 * forgets then. Its globals are its own; img_lock is held while it
 * computes, as module_draw (the editor's thread) may do it while a branch
 * reads it. */
#if defined(IMG_KEEP) || defined(IMG_FILTER)
#ifndef IMG_KEEP_H_
#define IMG_KEEP_H_

enum { IMG_INPUTS = 8 };
static Li_Value img_kept[IMG_INPUTS]; /* images as their id, an int */
static bool img_any_kept;
static pthread_mutex_t img_lock = PTHREAD_MUTEX_INITIALIZER;

/* V as it's kept: an image by its id */
static inline Li_Value
img_key(Li_Value v)
{
        ImgHeader h;
        if (v.type == img_type()) return img_header(v, &h) ? li_int((long) h.id) : (Li_Value) { .type = LI_BYTES };
        return li_copy(v);
}

static inline bool
img_same(Li_Value a, Li_Value b)
{
        if (a.type != b.type) return false;
        switch (a.type) {
        case LI_STRING: return !strcmp(a.s ? a.s : "", b.s ? b.s : "");
        case LI_BOOL: return a.b == b.b;
        case LI_INT: return a.i == b.i;
        case LI_FLOAT: return a.f == b.f;
        default: return true;
        }
}

static inline bool
img_unchanged(Li_Node *n)
{
        bool same = img_any_kept;
        for (int i = 0; i < IMG_INPUTS; i++) {
                Li_Value k = img_key(li_get(n, i));
                if (same && img_same(k, img_kept[i])) {
                        li_drop(&k);
                        continue;
                }
                same = false;
                li_drop(&img_kept[i]);
                img_kept[i] = k;
        }
        img_any_kept = true;
        return same;
}

void
module_start(Li_Node *n)
{
        (void) n;
        pthread_mutex_lock(&img_lock);
        img_any_kept = false;
        pthread_mutex_unlock(&img_lock);
}

void
module_destroy(Li_Node *n)
{
        (void) n;
        for (int i = 0; i < IMG_INPUTS; i++)
                li_drop(&img_kept[i]);
}

#endif /* IMG_KEEP_H_ */
#endif

/* A filter: image in (input 0), image out (output 0), showing its size and
 * the image. Define IMG_FILTER before including this and, after it,
 *   static void edit(Li_Node *n, Image *img);
 * which changes IMG in place, reading its other inputs. Made again only
 * when an input changes (see IMG_KEEP), and drawn always: it's up to date
 * before Run too. */
#ifdef IMG_FILTER
#ifndef IMG_FILTER_H_
#define IMG_FILTER_H_

static void edit(Li_Node *n, Image *img);

static void
filter(Li_Node *n)
{
        Image img;
        if (img_unchanged(n)) return;
        if (!img_get(n, 0, &img)) {
                img_none(n, 0);
                li_image(n, NULL, 0, 0);
                li_text(n, "no image");
                return;
        }
        edit(n, &img);
        li_text(n, "%dx%d", img.width, img.height);
        img_set(n, 0, img);
}

void
module_eval(Li_Node *n)
{
        pthread_mutex_lock(&img_lock);
        filter(n);
        pthread_mutex_unlock(&img_lock);
}

void
module_draw(Li_Node *n)
{
        module_eval(n);
}

#endif /* IMG_FILTER_H_ */
#endif /* IMG_FILTER */

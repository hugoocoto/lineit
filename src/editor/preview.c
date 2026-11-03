/* preview.c - pictures in squares: what a module shows with li_image */
#include "editor.h"

/* Its least size in the canvas (wider than tall, as most images are: it
 * takes the width of the square), and the most pixels it keeps of a side */
#define PREVIEW_WIDTH 160
#define PREVIEW_HEIGHT 96
#define PREVIEW_PIXELS 192

/* Squares compute in branches, on other threads, and textures are made in
 * this one when drawing: what li_image gives waits in PENDING */
struct Preview {
        Image pending; /* to show next, if NEW */
        bool new;
        Texture2D texture; /* id 0: none, an empty box */
};

static pthread_mutex_t lock = PTHREAD_MUTEX_INITIALIZER;

void
li_image(Li_Node *n, const unsigned char *rgba, int width, int height)
{
        Image img = { 0 };
        if (rgba && width > 0 && height > 0) {
                Image src = { (void *) rgba, width, height, 1, PIXELFORMAT_UNCOMPRESSED_R8G8B8A8 };
                float k   = fminf(1.0f, (float) PREVIEW_PIXELS / (float) Max(width, height));
                img       = ImageCopy(src);
                if (k < 1.0f) ImageResize(&img, Max(1, (int) (width * k)), Max(1, (int) (height * k)));
        }

        pthread_mutex_lock(&lock);
        if (!n->preview) {
                n->preview = calloc(1, sizeof *n->preview);
                n->size    = (Vector2) { 0 }; /* bigger now */
        }
        if (n->preview) {
                UnloadImage(n->preview->pending);
                n->preview->pending = img;
                n->preview->new     = true;
        } else
                UnloadImage(img);
        pthread_mutex_unlock(&lock);
}

float
preview_height(Square *s)
{
        return s->preview ? PREVIEW_HEIGHT + 4 : 0;
}

float
preview_width(Square *s)
{
        return s->preview ? PREVIEW_WIDTH + 16 : 0;
}

/* Under the text, at the top: the text is its size, and the previews of a
 * row of squares line up */
static Rectangle
preview_rect(Square *s)
{
        Rectangle r = square_rect(s);
        return (Rectangle) { r.x + 8, r.y + square_text_band(), r.width - 16, PREVIEW_HEIGHT };
}

void
preview_draw(Square *s)
{
        Preview *p = s->preview;
        if (!p) return;

        pthread_mutex_lock(&lock);
        if (p->new) {
                if (p->texture.id) UnloadTexture(p->texture);
                p->texture = p->pending.data ? LoadTextureFromImage(p->pending) : (Texture2D) { 0 };
                if (p->texture.id) SetTextureFilter(p->texture, TEXTURE_FILTER_BILINEAR);
                UnloadImage(p->pending);
                p->pending = (Image) { 0 };
                p->new     = false;
        }
        pthread_mutex_unlock(&lock);

        Rectangle box = preview_rect(s);
        DrawRectangleRec(box, cfg.colors.bg);
        if (p->texture.id) {
                /* All of it, as big as it fits, in the middle */
                float k     = fminf(box.width / p->texture.width, box.height / p->texture.height);
                Rectangle d = { box.x + (box.width - p->texture.width * k) / 2,
                                box.y + (box.height - p->texture.height * k) / 2, p->texture.width * k,
                                p->texture.height * k };
                DrawTexturePro(p->texture, (Rectangle) { 0, 0, p->texture.width, p->texture.height }, d,
                               (Vector2) { 0 }, 0, WHITE);
        }
        DrawRectangleLinesEx(box, 1.0f, cfg.colors.square_border);
}

void
preview_free(Square *s)
{
        Preview *p = s->preview;
        if (!p) return;
        UnloadImage(p->pending);
        if (p->texture.id) UnloadTexture(p->texture);
        free(p);
        s->preview = NULL;
}

/* image = the image in the file at "path": png, bmp, gif (its first
 * frame), qoi, jpg, tga, psd, hdr, pic and pnm (see RAYLIB_FORMATS in the
 * Makefile; built with make RAYLIB=repo, only the first four). Drop an image file on the
 * canvas to make one with its path, or Choose file... in its menu. A
 * relative path is relative to the canvas's folder. Read
 * again when the file changes. Pure: computed when read, no ticks
 * needed. */
#include "image.h"
#include <pthread.h>
#include <stdio.h>
#include <sys/stat.h>

enum { IN_PATH };
enum { OUT_IMAGE };

/* The file read last, and when it was changed: read again only if it is
 * another one, or it changed. Locked: module_draw runs in the editor's
 * thread while a branch may be reading it. */
typedef struct Loaded {
        pthread_mutex_t lock;
        char *path;
        struct timespec mtime;
        off_t size;
        Li_Value image;
        bool changed; /* image, since it was last set to the output */
        bool unshown; /* and since it was last shown */
        char why[64];
} Loaded;

void
module_init(Li_Node *n)
{
        li_input_value(n, "path", LI_STRING, li_string(""));
        img_out(n, "image");
        Loaded *l = calloc(1, sizeof *l);
        if (l) pthread_mutex_init(&l->lock, NULL);
        li_set_data(n, l);
}

static void
load(Loaded *l, const char *path)
{
        struct stat st;
        bool same = l->path && !strcmp(l->path, path);

        if (stat(path, &st) || !S_ISREG(st.st_mode)) {
                l->unshown |= l->image.type != LI_NONE;
                l->changed |= l->image.type != LI_NONE;
                li_drop(&l->image);
                snprintf(l->why, sizeof l->why, "%s", *path ? "no such file" : "no file");
        } else if (!same || st.st_mtim.tv_sec != l->mtime.tv_sec || st.st_mtim.tv_nsec != l->mtime.tv_nsec ||
                   st.st_size != l->size) {
                img_quiet();
                Image img = LoadImage(path);
                li_drop(&l->image);
                l->image   = img_value(img);
                l->changed = l->unshown = true;
                UnloadImage(img);
                snprintf(l->why, sizeof l->why, "can't read it");
                l->mtime = st.st_mtim;
                l->size  = st.st_size;
        }
        if (!same) {
                free(l->path);
                l->path = strdup(path);
        }
}

/* Reads the file if it has to, and shows its size, or why there's none.
 * With the lock. */
static void
update(Li_Node *n, Loaded *l)
{
        Li_Value p       = li_get(n, IN_PATH);
        const char *path = p.type == LI_STRING && p.s ? p.s : "";

        load(l, li_path(n, path)); /* relative to the canvas */
        if (l->unshown) img_show(n, l->image);
        l->unshown = false;
        if (l->image.type != LI_NONE) {
                ImgHeader h;
                memcpy(&h, li_bytes_data(l->image), sizeof h);
                li_text(n, "%dx%d", h.width, h.height);
        } else
                li_text(n, "%s", l->why);
}

void
module_eval(Li_Node *n)
{
        Loaded *l = li_data(n);
        if (!l) return;
        pthread_mutex_lock(&l->lock);
        update(n, l);
        /* Not every read: it's copied, and the squares it goes to would
         * think it's a new one */
        if (l->changed) li_set(n, OUT_IMAGE, l->image);
        l->changed = false;
        pthread_mutex_unlock(&l->lock);
}

/* Its size shows as soon as a file is chosen, without copying it to the
 * output every frame */
void
module_draw(Li_Node *n)
{
        Loaded *l = li_data(n);
        if (!l) return;
        pthread_mutex_lock(&l->lock);
        update(n, l);
        pthread_mutex_unlock(&l->lock);
}

/* Run emptied its output */
void
module_start(Li_Node *n)
{
        Loaded *l = li_data(n);
        if (!l) return;
        pthread_mutex_lock(&l->lock);
        l->changed = true;
        pthread_mutex_unlock(&l->lock);
}

void
module_destroy(Li_Node *n)
{
        Loaded *l = li_data(n);
        if (!l) return;
        li_drop(&l->image);
        free(l->path);
        pthread_mutex_destroy(&l->lock);
        free(l);
}

/* drop.c - files for squares: dropped on the window, or Choose file... */
#include "editor.h"

/* What image/open may read (RAYLIB_FORMATS; built with make RAYLIB=repo,
 * only the first four) */
#define IMAGE_EXTS ".png;.bmp;.gif;.qoi;.jpg;.jpeg;.tga;.psd;.hdr;.pic;.pnm;.ppm;.pgm"

/* The field a file goes to: a "path" input of S, typed while unplugged. -1
 * if it has none. */
static int
path_input(Square *s)
{
        int i = input_find(s, "path");
        return i >= 0 && s->inputs.items[i].kind == PIN_VALUE && s->inputs.items[i].type == LI_STRING ? i : -1;
}

bool
square_takes_file(Square *s)
{
        return path_input(s) >= 0;
}

/* PATH, relative to the current folder or absolute, as the square keeps it:
 * relative to the canvas (see canvas_relative) */
static bool
set_path(Square *s, const char *path)
{
        char buf[PATH_MAX];
        int i = path_input(s);
        if (i < 0 || link_to(s->id, i) >= 0) return false;
        return input_set_text(s, i, canvas_relative(path, buf, sizeof buf), LI_STRING);
}

/* Files dropped on the canvas: on a square with a path, its path; images
 * elsewhere, an image/open each, one under the other */
void
update_drop(void)
{
        if (!IsFileDropped()) return;
        FilePathList files = LoadDroppedFiles();
        Vector2 at         = mouse_world();
        Hit h              = square_hit(at);
        Module *open       = module_find("image/open");
        int other          = 0;
        bool taken         = h.square && files.count == 1 && set_path(h.square, files.paths[0]);

        if (taken) set_status("%s: %s", h.square->module->name, GetFileName(files.paths[0]));
        for (unsigned i = 0; !taken && i < files.count; i++) {
                Square *s;
                if (!open || !IsFileExtension(files.paths[i], IMAGE_EXTS)) {
                        other++;
                        continue;
                }
                if (!(s = square_new(open, at))) continue;
                set_path(s, files.paths[i]);
                at.y += square_rect(s).height + 20;
        }
        if (other)
                set_status("Dropped %u file%s: only images (%s) make squares, or a file on a square with a path",
                           files.count, files.count == 1 ? "" : "s", IMAGE_EXTS);
        UnloadDroppedFiles(files);
}

/* Choose file...: the path of a square, typed in the prompt, where Tab
 * completes any file */
static int choosing = -1; /* the square, by id: it may go meanwhile */

static void
choose_done(const char *path)
{
        Square *s = square_get(choosing);
        if (s && !set_path(s, path)) set_status("%s: its path is wired, not typed", s->module->name);
        choosing = -1;
}

void
square_choose_file(int id, void *arg)
{
        (void) arg;
        Square *s = square_get(id);
        char now[PATH_MAX];
        int i;
        if (!s || (i = path_input(s)) < 0) return;

        input_text(s, i, now, sizeof now, true);
        choosing = id;
        /* From the folder of the one it has (relative to the canvas, typed
         * relative to here), or of the canvas */
        char where[PATH_MAX * 2];
        const char *file = canvas_file(now, where, sizeof where);
        prompt_open("File", strchr(now, '/') ? TextFormat("%s/", GetDirectoryPath(file)) : file_dir(), choose_done);
        prompt.exts = NULL;
}

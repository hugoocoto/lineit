/* text.c - fonts and text */
#include "editor.h"

Font font;               /* bar and menus, at their size */
static Font canvas_font; /* squares: big, so it's sharp zoomed in */

/* Raylib only loads these. Others (.ttc collections) silently give the
 * default font. */
#define FONT_EXTENSIONS ".ttf;.otf"

/* Path of the best font file fontconfig has for NAME that raylib can load,
 * or NULL. Must be freed. */
static char *
font_find(const char *name)
{
        FcPattern *pattern;
        FcFontSet *set;
        FcChar8 *file;
        FcResult result;
        char *path = NULL;

        if (!FcInit()) return NULL;
        if ((pattern = FcNameParse((const FcChar8 *) name)) == NULL) return NULL;

        FcConfigSubstitute(NULL, pattern, FcMatchPattern);
        FcDefaultSubstitute(pattern);

        /* Best matches first */
        if ((set = FcFontSort(NULL, pattern, FcTrue, NULL, &result))) {
                for (int i = 0; !path && i < set->nfont; i++) {
                        if (FcPatternGetString(set->fonts[i], FC_FILE, 0, &file) == FcResultMatch &&
                            IsFileExtension((char *) file, FONT_EXTENSIONS))
                                path = strdup((char *) file);
                }
                FcFontSetDestroy(set);
        }
        FcPatternDestroy(pattern);
        return path;
}

/* PATH at SIZE, or raylib's default font if it fails */
static Font
font_load(const char *path, int size)
{
        Font f = path ? LoadFontEx(path, size, NULL, 0) : GetFontDefault();
        if (!IsFontValid(f) || f.texture.id == GetFontDefault().texture.id) {
                fprintf(stderr, "font: could not load %s, using default\n", path ? path : cfg.font.name);
                f = GetFontDefault();
        }
        return f;
}

/* Load cfg.font.name twice: at the UI size, and big for the canvas */
void
load_font(void)
{
        char *path = FileExists(cfg.font.name) ? strdup(cfg.font.name) : font_find(cfg.font.name);

        /* Rasterized at the biggest size used, smaller ones are scaled down */
        font = font_load(path, Max(Max(cfg.bar.font_size, cfg.menu.font_size), cfg.output.font_size));
        SetTextureFilter(font.texture, TEXTURE_FILTER_BILINEAR);

        /* Canvas text is scaled by the zoom: rasterize it at the size it has
         * at max zoom, and use mipmaps so it's not jagged zoomed out */
        int size    = Max(cfg.canvas.pin_font_size, cfg.canvas.name_font_size) * cfg.canvas.zoom_max;
        canvas_font = font_load(path, Clamp(size, 16, 256));
        if (canvas_font.texture.id != GetFontDefault().texture.id) {
                GenTextureMipmaps(&canvas_font.texture);
                SetTextureFilter(canvas_font.texture, TEXTURE_FILTER_TRILINEAR);
        }
        free(path);
}

void
unload_fonts(void)
{
        if (font.texture.id != GetFontDefault().texture.id) UnloadFont(font);
        if (canvas_font.texture.id != GetFontDefault().texture.id) UnloadFont(canvas_font);
        /* What font_find's FcInit loaded: it's done again when fonts load again */
        FcFini();
}

void
draw_text(const char *text, float x, float y, float size, Color color)
{
        DrawTextEx(font, text, (Vector2) { x, y }, size, cfg.font.spacing, color);
}

float
measure_text(const char *text, float size)
{
        return MeasureTextEx(font, text, size, cfg.font.spacing).x;
}

/* Same, for text on the canvas (world space) */
void
draw_canvas_text(const char *text, float x, float y, float size, Color color)
{
        DrawTextEx(canvas_font, text, (Vector2) { x, y }, size, cfg.font.spacing, color);
}

float
measure_canvas_text(const char *text, float size)
{
        return MeasureTextEx(canvas_font, text, size, cfg.font.spacing).x;
}

/* IN cut with ".." at the end to fit in MAX */
void
fit_canvas_text(const char *in, char *out, int size, float max, float fs)
{
        int len = snprintf(out, size, "%s", in);
        if (len >= size) len = size - 1;
        while (len > 2 && measure_canvas_text(out, fs) > max) {
                len--;
                out[len]     = '\0';
                out[len - 1] = '.';
                out[len - 2] = '.';
        }
}

/* TEXT cut with "..." to fit in W */
const char *
text_fit(const char *text, float size, float w)
{
        static char buf[256];
        if (measure_text(text, size) <= w) return text;
        int n = snprintf(buf, sizeof buf, "%s", text);
        if (n >= (int) sizeof buf) n = sizeof buf - 1;
        while (n > 0) {
                strcpy(buf + --n, "...");
                if (measure_text(buf, size) <= w) break;
        }
        return buf;
}

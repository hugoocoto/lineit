/* prompt.c - the prompt: a path typed for Open and Save as */
#include "editor.h"

struct PromptState prompt;

void
prompt_open(const char *label, const char *text, void (*done)(const char *))
{
        prompt.open        = true;
        prompt.label       = label;
        prompt.done        = done;
        prompt.matches[0]  = '\0';
        prompt.hint        = NULL;
        prompt.no_complete = false;
        text_input_set(&prompt.input, text);
        menu.depth = 0;
}

/* Tab: complete what's typed with the folders and .lua files it can be,
 * as far as they all agree, and list them */
static void
prompt_complete(void)
{
        TextInput *t = &prompt.input;
        char *slash  = strrchr(t->buf, '/');
        int dir_len  = slash ? (int) (slash - t->buf) + 1 : 0;
        char *start  = t->buf + dir_len;
        char dir[PATH_MAX];
        Da(char *) found = { 0 };

        snprintf(dir, sizeof dir, "%.*s", dir_len, t->buf);
        if (!*dir) strcpy(dir, ".");
        FilePathList files = LoadDirectoryFiles(dir);
        for (unsigned i = 0; i < files.count; i++) {
                const char *name = GetFileName(files.paths[i]);
                bool is_dir      = DirectoryExists(files.paths[i]);
                if (strncmp(name, start, strlen(start)) || (*name == '.' && *start != '.')) continue;
                if (!is_dir && !IsFileExtension(name, ".lua")) continue;
                Da_append(&found, strdup(TextFormat("%s%s", name, is_dir ? "/" : "")));
        }
        UnloadDirectoryFiles(files);

        prompt.matches[0] = '\0';
        if (found.count) {
                qsort(found.items, found.count, sizeof *found.items, str_cmp);
                /* As far as they all agree */
                int same = strlen(found.items[0]);
                for (int i = 1; i < found.count; i++) {
                        int k = 0;
                        while (k < same && found.items[i][k] == found.items[0][k])
                                k++;
                        same = k;
                }
                snprintf(start, sizeof t->buf - dir_len, "%.*s", same, found.items[0]);
                text_input_sync(t);
                for (int i = 0; found.count > 1 && i < found.count; i++) {
                        size_t used = strlen(prompt.matches);
                        snprintf(prompt.matches + used, sizeof prompt.matches - used, "%s%s", i ? "  " : "", found.items[i]);
                }
        } else {
                snprintf(prompt.matches, sizeof prompt.matches, "nothing there");
        }
        Da_foreach(f, found)
        {
                free(*f);
        }
        Da_destroy(&found);
}

bool
update_prompt(void)
{
        bool changed = false;
        if (!prompt.open) return false;
        TextResult tr = text_input_update(&prompt.input, &changed);
        if (changed) prompt.matches[0] = '\0';
        if (tr == TEXT_NEXT && !prompt.no_complete) prompt_complete();
        if (tr == TEXT_CANCEL) prompt.open = false;
        if (tr == TEXT_DONE && prompt.input.len) {
                prompt.open = false;
                prompt.done(prompt.input.buf);
        }
        return true; /* nothing else while it's open */
}

static Rectangle
prompt_rect(void)
{
        float w = fminf(640, GetScreenWidth() - 32);
        return (Rectangle) { (GetScreenWidth() - w) / 2, tabs_height() + 16, w, menu_item_height() };
}

void
draw_prompt(void)
{
        if (!prompt.open) return;
        Rectangle r = prompt_rect();
        float fs    = cfg.menu.font_size;
        float pad   = cfg.menu.padding;
        float lw    = measure_text(prompt.label, fs) + 2 * pad;

        DrawRectangleRec(r, cfg.colors.field);
        DrawRectangleRec((Rectangle) { r.x, r.y, lw + pad, r.height }, cfg.colors.menu);
        DrawRectangleLinesEx(r, 1.0f, cfg.colors.menu_border);
        draw_text(prompt.label, r.x + pad, r.y + pad, fs, cfg.colors.menu_text);
        /* Scrolled to the cursor if it's long */
        text_input_draw(&prompt.input, (Rectangle) { r.x + lw + 2 * pad, r.y + pad, r.width - lw - 3 * pad, fs }, fs,
                        cfg.colors.square_text, false);

        const char *hint = *prompt.matches ? prompt.matches :
                           prompt.hint     ? prompt.hint :
                                             "Tab completes, Enter accepts, Esc cancels";
        Rectangle h      = { r.x, r.y + r.height, r.width, menu_item_height() };
        DrawRectangleRec(h, cfg.colors.menu);
        DrawRectangleLinesEx(h, 1.0f, cfg.colors.menu_border);
        BeginScissorMode(h.x, h.y, h.width, h.height);
        draw_text(hint, h.x + pad, h.y + pad, fs, cfg.colors.name);
        EndScissorMode();
}

/* bar.c - the bar: Run, Step, Compile, Save, Load and the status */
#include "editor.h"

/* Message shown in the bar for a few seconds */
char status[256];
static double status_until;

void
set_status(const char *fmt, ...)
{
        va_list ap;
        va_start(ap, fmt);
        vsnprintf(status, sizeof status, fmt, ap);
        va_end(ap);
        status_until = GetTime() + 3.0;
        fprintf(stderr, "%s\n", status);
        output_add(status, true);
}

/* Compile in the bar */
static void
bar_compile(void)
{
        char bin[PATH_MAX];
        const char *name = canvas_path ? GetFileNameWithoutExt(canvas_path) : "canvas";
        if (not_canvas()) return;
        if (compile_canvas(name, bin, sizeof bin)) {
                set_status("Compiled %s", bin); /* to the output too */
        } else {
                output_panel.open = true;
                set_status("Could not compile, see the output");
        }
}

void
bar_save(void)
{
        if (not_canvas()) return;
        if (!canvas_path) {
                file_save_as(); /* never saved: ask where */
        } else if (canvas_save(canvas_path)) {
                tab_mark_saved();
                set_status("Saved %s", canvas_path);
        } else {
                set_status("Could not save %s", canvas_path);
        }
}

void
bar_load(void)
{
        char why[128];
        if (not_canvas()) return;
        if (canvas_path && FileExists(canvas_path) && !canvas_check(canvas_path, why, sizeof why)) {
                set_status("Can't load %s: %s", canvas_path, why);
        } else if (canvas_path && canvas_read_file(canvas_path, READ_LOAD)) {
                history_reset();
                tab_mark_saved();
                set_status("Loaded %s", canvas_path);
        } else {
                set_status("Could not load %s", canvas_path);
        }
}

void
bar_run_stop(void)
{
        if (not_canvas()) return;
        sched_step(sched_get(), false);
        if (stepping) {
                stepping = false;
                if (sched_busy(sched)) sched_resume(sched);
                run_check_end();
        } else if (running) {
                run_stop();
        } else if (run_begin()) {
                run_check_end();
        }
}

/* Step: the next square of the run, or a new run stopped before its first
 * one */
void
bar_step(void)
{
        if (not_canvas()) return;
        Sched *sc = sched_get();
        sched_step(sc, true);
        if (!running) {
                stepping = true;
                if (!run_begin()) stepping = false;
        } else if (sched_busy(sc)) {
                stepping = true;
                sched_resume(sc);
        } else {
                /* Between frames: the next one, stopped before its first square */
                stepping = true;
                program_lower();
                program_each(false);
        }
        if (running) run_check_end();
}

static void
bar_output(void)
{
        output_panel.open   = !output_panel.open;
        output_panel.unseen = 0;
}

/* A * when there are lines not seen yet */
static const char *
output_label(void)
{
        return output_panel.unseen ? "Output*" : "Output";
}

/* The run button does the last one chosen in its list, Step at first */
enum ActCompileState bar_act;

static const char *
act_label(void)
{
        switch (bar_act) {
        case ACT_STEP: return "Step";
        case ACT_RUN: return running && !stepping ? "Stop" : "Run";
        case ACT_COMPILE: return "Compile";
        }
        return "";
}

void
bar_act_do(void)
{
        switch (bar_act) {
        case ACT_STEP: bar_step(); break;
        case ACT_RUN: bar_run_stop(); break;
        case ACT_COMPILE: bar_compile(); break;
        }
}

/* Buttons at the right of the bar, from right to left. LABEL is used when
 * GET_LABEL is NULL. WIDEST is the longest label it can show. JOINED ones
 * touch the one at their left: the arrow of the run button. */
static const struct {
        const char *label;
        const char *(*get_label)(void);
        const char *widest;
        void (*fn)(void);
        bool joined;
} bar_buttons[] = {
        { .label = "v", .widest = "v", .fn = act_menu_open, .joined = true },
        [BAR_ACT] = { .get_label = act_label, .widest = "Compile", .fn = bar_act_do },
        { .get_label = output_label, .widest = "Output*", .fn = bar_output },
        [BAR_FILE] = { .label = "File", .widest = "File", .fn = file_menu_open },
};

Rectangle
bar_button_rect(int i)
{
        float fs  = cfg.bar.font_size;
        float pad = 4;
        float h   = cfg.bar.height - 2 * pad;
        float x   = GetScreenWidth();
        float w   = 0;

        for (int k = 0; k <= i; k++) {
                w = measure_text(bar_buttons[k].widest, fs) + 4 * pad;
                x -= w + (k && bar_buttons[k - 1].joined ? 0 : pad);
        }
        return (Rectangle) { x, GetScreenHeight() - cfg.bar.height + pad, w, h };
}

/* Returns true if the bar consumed the input this frame. */
bool
update_bar(void)
{
        if (!IsMouseButtonPressed(MOUSE_BUTTON_LEFT)) return false;
        for (int i = 0; i < countof(bar_buttons); i++) {
                if (CheckCollisionPointRec(GetMousePosition(), bar_button_rect(i))) {
                        menu.depth = 0; /* before: File opens one */
                        bar_buttons[i].fn();
                        return true;
                }
        }
        return false;
}

void
draw_bar(void)
{
        int w         = GetScreenWidth();
        int h         = cfg.bar.height;
        int fs        = cfg.bar.font_size;
        int y         = GetScreenHeight() - h;
        Vector2 world = GetScreenToWorld2D(GetMousePosition(), camera);

        DrawRectangle(0, y, w, h, cfg.colors.bar);
        DrawLine(0, y, w, y, cfg.colors.bar_border);

        /* Each in a slot as wide as the widest it usually is, so the ones
         * after don't move as numbers change (whatever the font). Own
         * buffers: TextFormat only keeps a few at once. */
        static const char *widest[] = { "x: -00000", "y: -00000", "zoom: 0.00", "squares: 000",
                                        "links: 000", "modules: 000", "FPS: 000" };
        char stats[countof(widest)][48];
        /* Rounded as integers: %.0f would show -0.3 as "-0" */
        snprintf(stats[0], sizeof stats[0], "x: %ld", lroundf(world.x));
        snprintf(stats[1], sizeof stats[1], "y: %ld", lroundf(world.y));
        snprintf(stats[2], sizeof stats[2], "zoom: %.2f", camera.zoom);
        snprintf(stats[3], sizeof stats[3], "squares: %d", squares.count);
        snprintf(stats[4], sizeof stats[4], "links: %d", links.count);
        snprintf(stats[5], sizeof stats[5], "modules: %d", modules.count);
        snprintf(stats[6], sizeof stats[6], "FPS: %d", GetFPS());
        float sx = 10, gap = measure_text("  ", fs);
        /* On a narrow window, the ones that would go under the buttons
         * aren't drawn, from the last */
        float room = bar_button_rect(countof(bar_buttons) - 1).x - gap;
        for (int i = 0; i < countof(widest); i++) {
                if (sx + measure_text(stats[i], fs) > room) break;
                draw_text(stats[i], sx, y + (h - fs) / 2, fs, cfg.colors.bar_text);
                sx += fmaxf(measure_text(stats[i], fs), measure_text(widest[i], fs)) + gap;
        }

        Rectangle b = { GetScreenWidth(), 0, 0, 0 };
        for (int i = 0; i < countof(bar_buttons); i++) {
                const char *text = bar_buttons[i].get_label ? bar_buttons[i].get_label() : bar_buttons[i].label;
                bool hover       = CheckCollisionPointRec(GetMousePosition(), b = bar_button_rect(i));
                DrawRectangleRec(b, hover ? cfg.colors.menu_hover : cfg.colors.menu);
                DrawRectangleLinesEx(b, 1.0f, cfg.colors.menu_border);
                draw_text(text, b.x + (b.width - measure_text(text, fs)) / 2, b.y + (b.height - fs) / 2, fs,
                          cfg.colors.bar_text);
        }

        /* Status at the left of the buttons. Stepping, where it is, while
         * there's no newer one. */
        const char *msg = GetTime() < status_until ? status : NULL;
        if (!msg && stepping)
                msg = step_names() ? TextFormat("Step: next is %s", step_names()) :
                                     "Step: next runs a frame";
        if (msg) {
                /* Over the stats if they are long: it's the newer thing */
                float sw = measure_text(msg, fs);
                DrawRectangle(b.x - sw - 24, y + 1, sw + 20, h - 1, cfg.colors.bar);
                draw_text(msg, b.x - sw - 12, y + (h - fs) / 2, fs, cfg.colors.bar_text);
        }
}

/* Runs canvases without the editor and checks what they do. For each X.lua
 * given, it loads it, presses Start, runs FRAMES frames of fake time and
 * compares the lines printed and the text of every square with X.out, next
 * to it. Run it with `make test` from the repo root (it reads config.lua and
 * the modules from build/modules). With -u it writes the .out files instead:
 * check them before keeping them.
 *
 * Time is fake (li_time and the editor's GetTime), FPS frames per second, so
 * clocks tick the same every run. A canvas that takes longer than TIMEOUT
 * seconds of real time fails the run. */

/* The editor itself, without its main, and with fake time */
#define main    lineit_main
#define GetTime fake_time
#include "../src/main.c"
#undef GetTime
#undef main

#include <signal.h>

enum { FPS     = 60,
       FRAMES  = 10 * FPS,
       TIMEOUT = 10 };

static double now;
static const char *current;
static FILE *report; /* stdout: the real one is silenced, li_print writes there too */

double
fake_time(void)
{
        return now;
}

static void
timeout(int sig)
{
        (void) sig;
        /* Not async signal safe, but we are leaving anyway */
        fprintf(stderr, "FAIL %s: still running after %ds\n", current, TIMEOUT);
        _exit(1);
}

/* Every square of LIST with its text, and the ones inside subgraphs below
 * it, indented */
static void
write_squares(FILE *f, SquareList *list, int depth)
{
        Da_foreach(sp, *list)
        {
                Square *s = *sp;
                fprintf(f, "%*s%d %s: %s\n", depth * 4, "", s->id, s->module->name, s->text);
                if (s->graph) write_squares(f, &s->graph->squares, depth + 1);
        }
}

/* What the canvas at PATH does, as text: the printed lines, then the
 * squares with their text. NULL if it can't be read. */
static char *
run(const char *path)
{
        output_clear();
        now = 0;
        if (!canvas_read_file(path, READ_LOAD)) return NULL;

        alarm(TIMEOUT);
        running = false;
        bar_start_stop();
        for (int i = 0; i < FRAMES; i++) {
                now = (double) i / FPS;
                run_frame();
        }
        running = false;
        alarm(0);

        char *buf;
        size_t len;
        FILE *f = open_memstream(&buf, &len);
        Da_foreach(l, output.lines)
        {
                if (!l->info) fprintf(f, "%s\n", l->text);
        }
        fprintf(f, "-- %d squares, %d links\n", squares.count, links.count);
        write_squares(f, &squares, 0);
        fclose(f);
        canvas_clear(-1, NULL);
        return buf;
}

/* First line where A and B differ, from 1 */
static int
diff_line(const char *a, const char *b)
{
        int line = 1;
        for (; *a && *a == *b; a++, b++)
                if (*a == '\n') line++;
        return line;
}

static void
show_line(const char *label, const char *text, int line)
{
        for (int i = 1; i < line && text; i++)
                if ((text = strchr(text, '\n'))) text++;
        if (!text || !*text) {
                fprintf(stderr, "  %s: (end)\n", label);
                return;
        }
        fprintf(stderr, "  %s: %.*s\n", label, (int) strcspn(text, "\n"), text);
}

int
main(int argc, char **argv)
{
        bool update = argc > 1 && !strcmp(argv[1], "-u");
        int failed = 0, total = 0;

        load_config("config.lua");
        SetConfigFlags(FLAG_WINDOW_HIDDEN);
        SetTraceLogLevel(LOG_WARNING);
        InitWindow(320, 240, "test");
        load_font();
        load_modules(cfg.modules.path);
        signal(SIGALRM, timeout);
        report = fdopen(dup(STDOUT_FILENO), "w");
        setvbuf(report, NULL, _IOLBF, 0); /* in order with the FAILs on stderr */
        freopen("/dev/null", "w", stdout);

        for (int i = 1 + update; i < argc; i++) {
                const char *path = argv[i];
                char out[PATH_MAX];
                snprintf(out, sizeof out, "%.*s.out", (int) (strlen(path) - strlen(GetFileExtension(path))), path);
                current = path;
                total++;

                char *got = run(path);
                if (!got) {
                        fprintf(stderr, "FAIL %s: can't read it\n", path);
                        failed++;
                        continue;
                }

                if (update) {
                        SaveFileText(out, got);
                        fprintf(report, "wrote %s\n", out);
                } else if (!FileExists(out)) {
                        fprintf(stderr, "FAIL %s: no %s, make it with -u\n", path, out);
                        failed++;
                } else {
                        char *want = LoadFileText(out);
                        if (strcmp(got, want)) {
                                int line = diff_line(got, want);
                                fprintf(stderr, "FAIL %s: differs from %s at line %d\n", path, out, line);
                                show_line("want", want, line);
                                show_line("got ", got, line);
                                failed++;
                        } else {
                                fprintf(report, "ok   %s\n", path);
                        }
                        UnloadFileText(want);
                }
                free(got);
        }

        unload_modules();
        unload_fonts();
        /* No CloseWindow: it unloads the GL driver, and the one under Xvfb
         * (Mesa) leaves memory that LeakSanitizer can't tell from ours in
         * make test-asan. The process ends right after anyway. */
        fprintf(report, "%d/%d passed\n", total - failed, total);
        fclose(report);
        return failed != 0;
}

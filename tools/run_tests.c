/* Runs canvases without the editor and checks what they do. For each X.lua
 * given, it loads it, presses Run, runs FRAMES frames of fake time and
 * compares the lines printed and the text of every square with X.out, next
 * to it. Run it with `make test` from the repo root (it reads config.lua and
 * the modules from build/modules). With -u it writes the .out files instead:
 * check them before keeping them.
 *
 * Time is fake (li_time and the editor's GetTime), FPS frames per second, so
 * clocks tick the same every run. A canvas that takes longer than TIMEOUT
 * seconds of real time fails the run.
 *
 * With -c it compiles them instead (Compile in the bar), to build/test-bin,
 * runs the programs for the same frames (--frames) and compares what they
 * print with the lines printed in X.out: Run and Compile have to do the
 * same.
 *
 * With -s it steps them instead (Step in the bar): pressed until the program
 * is done, every branch a square at a time, and the frames one by one.
 * Stepping has to do what Run does too. */

/* The editor itself, without its main, and with fake time (built with
 * -DLINEIT_FAKE_TIME: GetTime is fake_time, see src/editor/editor.h) */
#define main lineit_main
#include "../src/main.c"
#undef main

#include <signal.h>

enum { FPS    = 60,
       FRAMES = 10 * FPS };

/* ThreadSanitizer makes every handoff between threads slow, and stepping
 * pi.lua is tens of thousands of them */
#ifdef __SANITIZE_THREAD__
enum { TIMEOUT = 120 };
#else
enum { TIMEOUT = 10 };
#endif

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
static bool step; /* -s */

/* Step pressed until the branches running are done */
static void
step_through(void)
{
        while (running && sched_busy(sched))
                bar_step();
}

static char *
run(const char *path)
{
        output_clear();
        now = 0;
        if (!canvas_read_file(path, READ_LOAD)) return NULL;

        alarm(TIMEOUT);
        running = false;
        if (step) {
                bar_step(); /* the starts, stopped before their first square */
                step_through();
                for (int i = 0; i < FRAMES; i++) {
                        now = (double) i / FPS;
                        run_frame(); /* what squares show: stepping, no frame runs here */
                        if (!running) continue;
                        bar_step(); /* the frame's branches, stopped */
                        step_through();
                }
                run_stop();
        } else {
                bar_run_stop();
                for (int i = 0; i < FRAMES; i++) {
                        now = (double) i / FPS;
                        run_frame();
                }
        }
        running = false;
        alarm(0);

        char *buf;
        size_t len;
        FILE *f = open_memstream(&buf, &len);
        Da_foreach(l, output_panel.lines)
        {
                if (!l->info) fprintf(f, "%s\n", l->text);
        }
        fprintf(f, "-- %d squares, %d links\n", squares.count, links.count);
        write_squares(f, &squares, 0);
        fclose(f);
        canvas_clear(-1, NULL);
        return buf;
}

/* -c: the canvas at PATH compiled, and what it prints in FRAMES frames.
 * NULL if it can't be read, compiled or run. */
static char *
run_compiled(const char *path)
{
        char bin[PATH_MAX], cmd[PATH_MAX + 32];
        char *buf = NULL;
        size_t len;

        output_clear();
        if (!canvas_read_file(path, READ_LOAD)) return NULL;

        canvas_path = path;
        bool ok     = compile_canvas(GetFileNameWithoutExt(path), bin, sizeof bin);
        canvas_path = NULL;
        canvas_clear(-1, NULL);
        if (!ok) {
                Da_foreach(l, output_panel.lines)
                {
                        fprintf(stderr, "  %s\n", l->text);
                }
                return NULL;
        }

        alarm(TIMEOUT);
        snprintf(cmd, sizeof cmd, "'%s' --frames %d", bin, FRAMES);
        FILE *p = popen(cmd, "r");
        FILE *f = open_memstream(&buf, &len);
        int c;
        while (p && (c = fgetc(p)) != EOF)
                fputc(c, f);
        fclose(f);
        if (!p || pclose(p) != 0) {
                free(buf);
                buf = NULL;
        }
        alarm(0);
        return buf;
}

/* The lines printed in the .out TEXT, before the squares */
static void
printed_only(char *text)
{
        char *end = strstr(text, "-- ");
        while (end && end != text && end[-1] != '\n')
                end = strstr(end + 1, "-- ");
        if (end) *end = '\0';
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
        bool update  = argc > 1 && !strcmp(argv[1], "-u");
        bool compile = argc > 1 && !strcmp(argv[1], "-c");
        step         = argc > 1 && !strcmp(argv[1], "-s");
        int failed = 0, total = 0;

        config_start("config.lua");
        /* Every line printed, to compare: the panel keeps only the last ones */
        cfg.output.lines = INT_MAX;
        SetTraceLogLevel(LOG_WARNING);
        /* No window with ThreadSanitizer (make test-tsan): the GL driver's
         * own threads aren't what it's there for. Running doesn't need the
         * font, Compile runs without it too. */
#ifndef __SANITIZE_THREAD__
        SetConfigFlags(FLAG_WINDOW_HIDDEN);
        InitWindow(320, 240, "test");
        load_font();
#endif
        load_modules(cfg.modules.path);
        signal(SIGALRM, timeout);
        report = fdopen(dup(STDOUT_FILENO), "w");
        setvbuf(report, NULL, _IOLBF, 0); /* in order with the FAILs on stderr */
        if (!freopen("/dev/null", "w", stdout)) {
                perror("run_tests: /dev/null");
                return 1;
        }

        /* Not where the config says: these are only tests */
        if (cfg.compile.output != cfg_defaults.compile.output) free((char *) cfg.compile.output);
        cfg.compile.output = "build/test-bin";
        for (int i = 1 + update + compile + step; i < argc; i++) {
                const char *path = argv[i];
                char out[PATH_MAX];
                snprintf(out, sizeof out, "%.*s.out", (int) (strlen(path) - strlen(GetFileExtension(path))), path);
                current = path;
                total++;

                char *got = compile ? run_compiled(path) : run(path);
                if (!got && compile) {
                        fprintf(stderr, "FAIL %s: can't compile or run it\n", path);
                        failed++;
                        continue;
                }
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
                        if (compile) printed_only(want);
                        if (strcmp(got, want)) {
                                int line = diff_line(got, want);
                                fprintf(stderr, "FAIL %s: differs from %s at line %d\n", path, out, line);
                                show_line("want", want, line);
                                show_line("got ", got, line);
                                failed++;
                        } else {
                                fprintf(report, "ok   %s%s\n", path, compile ? " (compile)" : "");
                        }
                        UnloadFileText(want);
                }
                free(got);
        }

        unload_modules();
#ifndef __SANITIZE_THREAD__
        unload_fonts();
#endif
        /* No CloseWindow: it unloads the GL driver, and the one under Xvfb
         * (Mesa) leaves memory that LeakSanitizer can't tell from ours in
         * make test-asan. The process ends right after anyway. */
        fprintf(report, "%d/%d passed%s\n", total - failed, total, compile ? ", compile" : step ? ", stepped" :
                                                                                                  "");
        fclose(report);
        return failed != 0;
}

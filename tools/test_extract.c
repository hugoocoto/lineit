/* Tests Make subgraph: extracts squares of example canvases into subgraphs
 * and checks the canvas prints the same as before, with the ports it should
 * have. Run it with `make test` from the repo root. The subgraphs go to a
 * temporary folder, modules.user while it runs. */

/* The editor itself, without its main, and with fake time */
#define main lineit_main
#define GetTime fake_time
#include "../src/main.c"
#undef GetTime
#undef main

static double now;
static int failed;

double
fake_time(void)
{
        return now;
}

/* What it prints in 2 seconds after Run */
static char *
run(void)
{
        char *buf;
        size_t len;
        output_clear();
        running = false;
        bar_run_stop();
        for (int i = 0; i < 120; i++) {
                now = i / 60.0;
                run_frame();
        }
        running = false;
        FILE *f = open_memstream(&buf, &len);
        Da_foreach(l, output_panel.lines)
        {
                if (!l->info) fprintf(f, "%s\n", l->text);
        }
        fclose(f);
        return buf;
}

static void
pins(char *buf, int size, PinList *l)
{
        *buf = '\0';
        Da_foreach(p, *l)
        {
                size_t used = strlen(buf);
                snprintf(buf + used, size - used, "%s%s:%s", used ? " " : "", p->name, type_name(p->type));
        }
}

/* Extract squares IDS of CANVAS as the subgraph NAME, it should have the
 * pins INS and OUTS */
static void
check(const char *canvas, const int *ids, int n, const char *name, const char *ins, const char *outs)
{
        char path[PATH_MAX], got_ins[256], got_outs[256];
        canvas_read_file(canvas, READ_LOAD);
        char *before = run();
        select_all(false);
        for (int i = 0; i < n; i++)
                square_get(ids[i])->selected = true;

        snprintf(path, sizeof path, "%s/test-extract/%s", cfg.modules.user, name);
        subgraph_make(path);
        char *after = run();

        Square *s = NULL;
        Da_foreach(sp, squares)
        {
                if ((*sp)->selected) s = *sp;
        }
        if (!s || strcmp(s->module->name, TextFormat("test-extract/%s", name))) {
                fprintf(stderr, "FAIL %s: no subgraph square (%s)\n", canvas, status);
                failed++;
        } else {
                pins(got_ins, sizeof got_ins, &s->inputs);
                pins(got_outs, sizeof got_outs, &s->outputs);
                if (strcmp(got_ins, ins) || strcmp(got_outs, outs)) {
                        fprintf(stderr, "FAIL %s: pins are [%s] [%s], want [%s] [%s]\n", canvas, got_ins, got_outs, ins, outs);
                        failed++;
                } else if (strcmp(before, after) || !*after) {
                        fprintf(stderr, "FAIL %s: prints something else after it\n", canvas);
                        failed++;
                } else {
                        fprintf(stderr, "ok   %s -> %s\n", canvas, s->module->name);
                }
        }
        free(before);
        free(after);
        canvas_clear(-1, NULL);
}

int
main(void)
{
        char user[] = "/tmp/lineit-test-extract-XXXXXX";
        config_start("config.lua");
        SetConfigFlags(FLAG_WINDOW_HIDDEN);
        SetTraceLogLevel(LOG_WARNING);
        InitWindow(320, 240, "test");
        load_font();
        load_modules(cfg.modules.path);
        if (!mkdtemp(user)) return 1;
        const char *user_default = cfg.modules.user;
        cfg.modules.user         = user;
        /* print writes to stdout too: only the results, on stderr */
        if (!freopen("/dev/null", "w", stdout)) return 1;

        /* Data only: index feeds both pins of mul, it's one input */
        check("examples/sum_squares.lua", (int[]) { 25, 27 }, 2, "mul_add", "index:int value:any", "sum:any");
        /* Ticks in and out: the first FizzBuzz check */
        check("examples/fizzbuzz.lua", (int[]) { 4, 5, 6, 7 }, 4, "fizz", "body:tick index:int", "false:tick");

        unload_modules();
        unload_fonts();
        /* No CloseWindow: it unloads the GL driver, and the one under Xvfb
         * (Mesa) leaves memory that LeakSanitizer can't tell from ours in
         * make test-asan. The process ends right after anyway. */
        cfg.modules.user = user_default;
        /* What it made: the subgraphs */
        if (system(TextFormat("rm -rf '%s'", user)) != 0) failed++;
        fprintf(stderr, "%s\n", failed ? "test_extract: FAILED" : "test_extract: ok");
        return failed != 0;
}

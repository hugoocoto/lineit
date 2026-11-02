/* Tests changing the canvas while stepping: branches are stopped inside
 * squares (a loop between two ticks of its body), and deleting, cutting or
 * undoing them, or reloading their module, must not leave the branches to
 * go on in squares that are gone. Run it with `make test` from the repo
 * root (make test-asan finds what's used after it's freed). */

/* The editor itself, without its main, and with fake time (built with
 * -DLINEIT_FAKE_TIME: GetTime is fake_time, see src/editor/editor.h) */
#define main lineit_main
#include "../src/main.c"
#undef main

static double now;
static int failed;

double
fake_time(void)
{
        return now;
}

/* EXAMPLE loaded and stepped STEPS times: stopped inside its loop */
static bool
stepped(const char *example, int steps)
{
        canvas_clear(-1, NULL);
        if (!canvas_read_file(example, READ_LOAD)) return false;
        history_reset();
        run_frame();
        for (int i = 0; i < steps; i++) {
                bar_step();
                now += 1.0 / 60;
                run_frame();
        }
        return stepping && sched_busy(sched_get());
}

static void
check(const char *what, bool ok)
{
        fprintf(stderr, "%s %s\n", ok ? "ok  " : "FAIL", what);
        failed += !ok;
}

/* A frame after a change: the change is seen and the run stops */
static void
frame(void)
{
        now += 1.0 / 60;
        run_frame();
        history_check();
}

int
main(void)
{
        config_start("config.lua");
        SetTraceLogLevel(LOG_WARNING);
        SetConfigFlags(FLAG_WINDOW_HIDDEN);
        InitWindow(320, 240, "test");
        load_font();
        load_modules(cfg.modules.path);
        if (!freopen("/dev/null", "w", stdout)) return 1;

        /* sum_squares: entry, var/var, then flow/loop (id 23) ticking its body */
        check("stepped into the loop", stepped("examples/sum_squares.lua", 6));
        square_delete(23, NULL);
        frame();
        check("deleting the square a branch is in stops the run", !stepping && !sched_busy(sched_get()));
        bar_step(); /* and Step again starts a new one */
        frame();
        run_stop();

        check("stepped into the loop again", stepped("examples/sum_squares.lua", 6));
        select_all(true);
        selection_copy();
        each_selected(square_delete);
        frame();
        check("cutting everything stops the run", !stepping && !sched_busy(sched_get()));
        history_step(true); /* undo: the squares come back, new ones */
        frame();
        bar_step();
        frame();
        run_stop();

        /* Undo puts back a link, not a square: the program changes, the
         * squares don't (once it called a node that wasn't the one anymore) */
        check("stepped into the loop for an undone link", stepped("examples/sum_squares.lua", 6));
        Da_remove(&links, links.count - 1);
        frame(); /* the change stops the run */
        for (int i = 0; i < 6; i++) {
                bar_step();
                frame();
        }
        check("stepping again", stepping && sched_busy(sched_get()));
        history_step(true);
        frame();
        check("undoing it stops the run", !stepping && !sched_busy(sched_get()));
        run_stop();

        check("stepped into the loop for reloading", stepped("examples/sum_squares.lua", 6));
        Square *s = square_get(23);
        if (s) module_reload_squares(s->module);
        frame();
        check("reloading the module of the square a branch is in stops the run", !stepping && !sched_busy(sched_get()));
        run_stop();

        canvas_clear(-1, NULL);
        unload_modules();
        unload_fonts(); /* what fontconfig keeps: LeakSanitizer would say it's lost */
        fprintf(stderr, "%s\n", failed ? "test_step_edit: FAILED" : "test_step_edit: ok");
        return failed != 0;
}

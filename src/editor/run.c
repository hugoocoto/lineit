/* run.c - Run and Step: starting, stepping and stopping the program */
#include "editor.h"
bool running;  /* module_frame is only called while running */
bool stepping; /* a run going one square at a time, see run.c */

void
run_frame(void)
{
        got_free(0); /* what the main thread read since the last frame: draws, tooltips */
        program_lower();
        /* Stepping, frames go on with Step */
        if (running && !stepping) program_each(false);
        for (int i = 0; i < squares.count; i++) {
                Square *s = squares.items[i];
                if (s->fn.draw) s->fn.draw(s);
        }
        tabs_run_background();
}

/* Only squares with module_frame (a clock) start tick chains after the
 * start. Without them a run is over once the starts are done, like a
 * compiled program (see compile.c). */
static bool
program_has_frames(void)
{
        for (int i = 0; i < prog.nodes.count; i++) {
                if (prog.nodes.items[i].square->fn.frame) return true;
        }
        return false;
}

void
run_stop(void)
{
        running  = false;
        stepping = false;
        if (sched) sched_abort(sched);
}

/* The branches started are done (or stopped, stepping): over, if nothing
 * starts more */
void
run_check_end(void)
{
        if (!sched_busy(sched_get()) && !program_has_frames()) run_stop();
}

/* Squares say their pin types when drawn (var/get takes its variable's,
 * value/convert the one chosen): without a window (the tests, --compile)
 * nothing was. Before a run or a compile, every square says them. */
void
squares_settle(void)
{
        int mark = got_mark();
        Da_foreach(sp, squares)
        {
                if ((*sp)->fn.draw) (*sp)->fn.draw(*sp);
        }
        got_free(mark);
}

/* A new run: from the declared values of the variables, the starts first
 * (as branches, see branches.h). False if it can't. */
bool
run_begin(void)
{
        const char *bad = decls_check();
        if (bad) {
                set_status("The squares of variable %s say different things: make them agree to run", bad);
                return false;
        }
        running = true;
        vars_reset();
        if (cfg.output.clear_on_run) output_clear();
        output_panel.open   = true;
        output_panel.unseen = 0;
        program_lower();
        squares_settle();
        program_lower();
        program_each(true);
        return true;
}

/* Run, or Stop. Stepping, it goes on to the end at once. */
/* The square of the canvas shown that has node X: itself, or the
 * subgraph square it's inside of */
static Square *
node_square(int x)
{
        if (x < 0 || x >= prog.nodes.count) return NULL;
        Square *s = prog.nodes.items[x].square;
        for (int lv = prog.nodes.items[x].level; lv > 0 && lv < levels.count; lv = levels.items[lv].up)
                s = levels.items[lv].owner;
        return s;
}

/* Stepping, the squares the branches run next: one each */
#define STEP_MAX 64

static int
step_squares(Square **out)
{
        int nodes[STEP_MAX], n = stepping && sched ? sched_next(sched, nodes, STEP_MAX) : 0, k = 0;
        for (int i = 0; i < n; i++) {
                Square *s = node_square(nodes[i]);
                if (s) out[k++] = s;
        }
        return k;
}

/* S is one of them: it has an outline */
bool
step_next(Square *s)
{
        Square *next[STEP_MAX];
        int n = step_squares(next);
        for (int i = 0; i < n; i++) {
                if (next[i] == s) return true;
        }
        return false;
}

/* "print, push" for the bar */
const char *
step_names(void)
{
        static char buf[256];
        Square *next[STEP_MAX];
        int n = step_squares(next), used = 0;
        buf[0] = '\0';
        for (int i = 0; i < n && used < (int) sizeof buf; i++)
                used += snprintf(buf + used, sizeof buf - used, "%s%s", i ? ", " : "", next[i]->module->label);
        return n ? buf : NULL;
}

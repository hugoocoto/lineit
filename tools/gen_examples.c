/* Builds examples/*.lua: places the squares and wires with the real modules
 * and writes them with the editor's own canvas_save, so pin indices can't be
 * wrong. Run it with `make examples` from the repo root (it reads config.lua
 * and the modules from build/modules). It opens a hidden window: square
 * sizes depend on the font.
 *
 * To add an example, write a function like hello() and call it from main.
 * sq() places a square on a layout grid, w() wires an output to an input
 * and fails the run if the types don't match. */

/* The editor itself, without its main */
#define main lineit_main
#include "../src/main.c"
#undef main

static int bad;

/* A square centered at column X, row Y of the layout grid */
static Square *
sq(const char *module, float x, float y)
{
        Module *m = module_find(module);
        if (!m) {
                fprintf(stderr, "no module %s\n", module);
                exit(1);
        }
        return square_new(m, (Vector2) { x * 190, y * 150 });
}

static Square *
param(Square *s, const char *value)
{
        if (!param_set_text(s, 0, value)) {
                fprintf(stderr, "bad param %s for %s\n", value, s->module->name);
                exit(1);
        }
        return s;
}

/* Wire A.out -> B.in, and complain if it doesn't fit */
static void
w(Square *a, int out, Square *b, int in)
{
        int before = links.count;
        link_add(a, out, b, in);
        if (links.count == before || !link_valid(links.items[links.count - 1])) {
                fprintf(stderr, "bad link %s.%d -> %s.%d\n", a->module->name, out, b->module->name, in);
                bad++;
        }
}

/* Type TEXT in the field of input IN of S, or fail the run */
static Square *
in(Square *s, int input, const char *text)
{
        if (input >= s->inputs.count || !input_set_text(s, input, text, LI_NONE)) {
                fprintf(stderr, "bad value %s for %s input %d\n", text, s->module->name, input);
                exit(1);
        }
        return s;
}

static void
save(const char *name, const char *description)
{
        /* Center the view on the squares */
        Vector2 lo = { 1e9, 1e9 }, hi = { -1e9, -1e9 };
        Da_foreach(sp, squares)
        {
                Rectangle r = square_rect(*sp);
                lo          = Vector2Min(lo, (Vector2) { r.x, r.y });
                hi          = Vector2Max(hi, (Vector2) { r.x + r.width, r.y + r.height + 24 });
        }
        float zoom = fminf(1.0f, fminf(1200 / (hi.x - lo.x + 80), 640 / (hi.y - lo.y + 80)));
        camera     = (Camera2D) { .target = Vector2Scale(Vector2Add(lo, hi), 0.5f), .offset = { 640, 344 }, .zoom = zoom };

        char path[256], tmp[256];
        snprintf(path, sizeof path, "examples/%s.lua", name);
        snprintf(tmp, sizeof tmp, "examples/%s.tmp", name);
        canvas_save(tmp);

        /* Description on top, as Lua comments */
        FILE *in = fopen(tmp, "r"), *out = fopen(path, "w");
        fprintf(out, "-- %s\n", name);
        for (const char *p = description; *p;) {
                const char *nl = strchr(p, '\n');
                int len        = nl ? (int) (nl - p) : (int) strlen(p);
                fprintf(out, "-- %.*s\n", len, p);
                p += len + (nl != NULL);
        }
        fprintf(out, "--\n-- Open it with: ./lineit -f %s, then press Start.\n", path);
        char line[4096];
        fgets(line, sizeof line, in); /* skip "-- lineit canvas" */
        while (fgets(line, sizeof line, in))
                fputs(line, out);
        fclose(in), fclose(out), remove(tmp);
        printf("wrote %s (%d squares, %d links)\n", path, squares.count, links.count);
        canvas_clear(-1, NULL);
}

static void
hello(void)
{
        Square *e = sq("flow/entry", 0, 0);
        Square *p = in(sq("io/print", 1, 0), 1, "Hello, world!");
        w(e, 0, p, 0);
        save("hello", "Prints \"Hello, world!\" when you press Start.\n"
                      "entry ticks once on Start. print writes its value to stdout and shows\n"
                      "it: the text is typed in its value field (click it to change it).");
}

static void
fizzbuzz(void)
{
        Square *e  = sq("flow/entry", 0, 0);
        Square *lp = in(in(sq("flow/loop", 1, 0), 1, "1"), 2, "101");
        w(e, 0, lp, 0);

        /* index % N == 0, for N = 15, 3, 5 */
        const char *ns[]    = { "15", "3", "5" };
        const char *words[] = { "FizzBuzz", "Fizz", "Buzz" };
        Square *prev        = NULL;
        for (int k = 0; k < 3; k++) {
                float x     = 2.3f + k * 1.5f;
                Square *mod = in(sq("math/modulo", x - 0.4f, 1.6), 1, ns[k]);
                Square *eq  = in(sq("compare/equal", x + 0.2f, 2.6), 1, "0");
                Square *ie  = sq("flow/ifelse", x, 0);
                Square *pr  = in(sq("io/print", x + 0.6f, -1.1), 1, words[k]);
                w(lp, 1, mod, 0);
                w(mod, 0, eq, 0);
                w(eq, 0, ie, 1);
                w(prev ? prev : lp, prev ? 1 : 0, ie, 0); /* loop.body, or the previous ifelse.false */
                w(ie, 0, pr, 0);
                prev = ie;
        }
        Square *pn = sq("io/print", 6.9, 0.6);
        w(prev, 1, pn, 0), w(lp, 1, pn, 1);
        save("fizzbuzz", "FizzBuzz from 1 to 100: multiples of 15 print FizzBuzz, of 3 Fizz,\n"
                         "of 5 Buzz, and the rest the number. The output goes to stdout.\n"
                         "loop runs index from 1 to 100 (typed in its from and to fields).\n"
                         "For each one, three ifelse in a row check index % 15, % 3 and % 5\n"
                         "== 0 (math/modulo + compare/equal), and the first true one prints its\n"
                         "word. If none is, the last print prints the index.");
}

static void
clock_example(void)
{
        /* The check below the clock, so the wire back to reset can be seen */
        Square *e   = sq("flow/entry", 0, 0);
        Square *clk = sq("flow/clock", 1, 0);
        Square *d   = sq("io/display", 2.3, -0.7);
        Square *cmp = in(in(sq("compare/compare", 2.1, 1.3), 0, ">="), 2, "10");
        Square *ie  = sq("flow/ifelse", 3.2, 0.9);
        w(e, 0, clk, 0);
        w(clk, 0, ie, 0), w(clk, 1, cmp, 1), w(cmp, 0, ie, 1);
        w(ie, 0, clk, 0);
        w(clk, 1, d, 0);
        save("clock", "Counts seconds from 0 to 9 forever.\n"
                      "entry resets the clock on Start so it always starts at 0. On each\n"
                      "tick, ifelse checks count >= 10 (compare, with >= chosen in its op\n"
                      "list) and, if so, resets the clock again. display shows the count.\n"
                      "Type another number in the clock's seconds field to change the speed.");
}

static void
sum_squares(void)
{
        Square *e    = sq("flow/entry", 0, 0);
        Square *init = in(param(sq("var/set", 1, 0), "sum"), 1, "0");
        Square *lp   = sq("flow/loop", 2.1, 0);
        Square *mul  = sq("math/multiply", 3.2, 1.3);
        Square *get  = param(sq("var/get", 3.2, 2.4), "sum");
        Square *add  = sq("math/add", 4.3, 1.7);
        Square *set  = param(sq("var/set", 5.3, 0), "sum");
        Square *pr   = sq("io/print", 5.3, 2.6);
        w(e, 0, init, 0);
        w(init, 0, lp, 0);
        w(lp, 1, mul, 0), w(lp, 1, mul, 1);
        w(get, 0, add, 0), w(mul, 0, add, 1);
        w(lp, 0, set, 0), w(add, 0, set, 1);
        w(lp, 2, pr, 0), w(get, 0, pr, 1);
        save("sum_squares", "Sum of the squares of 0..9 (285), with a variable.\n"
                            "entry sets sum = 0, then loop runs index from 0 to 9 and does\n"
                            "sum = sum + index * index (var/get, math/multiply, math/add, var/set).\n"
                            "When the loop is done, print shows sum.");
}

static void
collatz(void)
{
        /* Ticks along the top row, values below what uses them */
        Square *e      = sq("flow/entry", 0, 0);
        Square *set_n  = in(param(sq("var/set", 1, 0), "n"), 1, "27");
        Square *set_s  = in(param(sq("var/set", 2, 0), "steps"), 1, "0");
        Square *wh     = sq("flow/while", 3.1, 0);
        Square *ne     = in(in(sq("compare/compare", 2.5, 1.4), 0, "!="), 2, "1");
        Square *get_n  = param(sq("var/get", 1.3, 2.4), "n");
        Square *ie     = sq("flow/ifelse", 4.5, 0);
        Square *mod    = in(sq("math/modulo", 3.3, 2.5), 1, "2");
        Square *eq     = in(sq("compare/equal", 4.3, 1.5), 1, "0");
        Square *half   = in(sq("math/divide", 5.2, -1.5), 1, "2");
        Square *even   = param(sq("var/set", 6.2, -0.8), "n");
        Square *triple = in(sq("math/multiply", 4.6, 2.9), 1, "3");
        Square *plus1  = in(sq("math/add", 5.5, 2.2), 1, "1");
        Square *odd    = param(sq("var/set", 6.2, 0.9), "n");
        Square *get_s  = param(sq("var/get", 6.6, 2.5), "steps");
        Square *inc    = in(sq("math/add", 7.3, 1.5), 1, "1");
        Square *set_s2 = param(sq("var/set", 7.6, 0), "steps");
        Square *pr_n   = sq("io/print", 8.8, 0);
        Square *pr_s   = sq("io/print", 4.3, -1.6);
        w(e, 0, set_n, 0);
        w(set_n, 0, set_s, 0);
        w(set_s, 0, wh, 0);
        w(get_n, 0, ne, 1), w(ne, 0, wh, 1);
        w(wh, 0, ie, 0);
        w(get_n, 0, mod, 0), w(mod, 0, eq, 0), w(eq, 0, ie, 1);
        w(get_n, 0, half, 0);
        w(get_n, 0, triple, 0), w(triple, 0, plus1, 0);
        w(ie, 0, even, 0), w(half, 0, even, 1);
        w(ie, 1, odd, 0), w(plus1, 0, odd, 1);
        w(even, 0, set_s2, 0), w(odd, 0, set_s2, 0);
        w(get_s, 0, inc, 0), w(inc, 0, set_s2, 1);
        w(set_s2, 0, pr_n, 0), w(get_n, 0, pr_n, 1);
        w(wh, 1, pr_s, 0), w(get_s, 0, pr_s, 1);
        save("collatz", "Collatz sequence of 27: while n != 1, n = n / 2 if it's even, and\n"
                        "n = 3n + 1 if it's odd. Prints every n, and the number of steps\n"
                        "(111) at the end.\n"
                        "Shows while with a comparison as its cond, ifelse, and variables.\n"
                        "Type another number than 27 in the first var/set to try it.");
}

static void
fibonacci(void)
{
        Square *e     = sq("flow/entry", 0, 0);
        Square *clear = param(sq("list/clear", 1, 0), "fib");
        Square *push0 = in(param(sq("list/push", 2, 0), "fib"), 1, "0");
        Square *push1 = in(param(sq("list/push", 3, 0), "fib"), 1, "1");
        Square *lp    = in(in(sq("flow/loop", 4.1, 0), 1, "2"), 2, "30");
        Square *last  = in(param(sq("list/get", 4.1, 1.6), "fib"), 0, "-1");
        Square *prev  = in(param(sq("list/get", 4.1, 2.6), "fib"), 0, "-2");
        Square *add   = sq("math/add", 5.2, 2.1);
        Square *push  = param(sq("list/push", 6.2, 0), "fib");
        Square *pr    = sq("io/print", 7.2, 0);
        Square *len   = param(sq("list/length", 6.4, 1.6), "fib");
        Square *d     = sq("io/display", 7.4, 1.6);
        w(e, 0, clear, 0);
        w(clear, 0, push0, 0);
        w(push0, 0, push1, 0);
        w(push1, 0, lp, 0);
        w(last, 0, add, 0), w(prev, 0, add, 1);
        w(lp, 0, push, 0), w(add, 0, push, 1);
        w(push, 0, pr, 0), w(last, 0, pr, 1);
        w(len, 0, d, 0);
        save("fibonacci", "The first 30 Fibonacci numbers, in a list.\n"
                          "entry clears the list fib and pushes 0 and 1. Then loop 2..29\n"
                          "pushes fib[-1] + fib[-2] (list/get with negative indices count\n"
                          "from the end) and prints the new last one. display shows the\n"
                          "length of the list.");
}

int
main(void)
{
        load_config("config.lua");
        SetConfigFlags(FLAG_WINDOW_HIDDEN);
        SetTraceLogLevel(LOG_WARNING);
        InitWindow(320, 240, "gen");
        load_font();
        load_modules(cfg.modules.path);

        hello();
        fizzbuzz();
        clock_example();
        sum_squares();
        collatz();
        fibonacci();

        unload_modules();
        unload_fonts();
        CloseWindow();
        printf("%d bad links\n", bad);
        return bad != 0;
}

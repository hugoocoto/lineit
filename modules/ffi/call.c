/* Calls a C function of a library, as its prototype says, with libffi: no
 * module to write for it. "lib" is the library ("m" for libm, "c", a path
 * to a .so; empty: whatever lineit has loaded already, libc and libm
 * included), "fn" its prototype, as in C:
 *
 *   double pow(double x, double y)
 *   unsigned long strlen(const char *s)
 *   FILE *fopen(const char *path, const char *mode)
 *   int snprintf(char buf[64], size_t n, const char *fmt, ..., int, double)
 *   div_t div(int num, int den)
 *   time_t timegm(tm *t)
 *   void qsort(void *base, size_t n, size_t size, void *compar)
 *
 * The square takes the shape of it: an input per argument (named as in the
 * prototype, or a1, a2...), "result" (none for void), and an output per
 * argument C fills: a pointer to a record that isn't const (it goes in,
 * and comes out as C left it), and T name[N] (N of T, zeroed, for C to
 * fill: no input). See proto.h for how types are. A variadic function says
 * the types of this call's extra arguments after the dots.
 *
 * Ticked (tick in, then out), it calls on each tick: for what it does (a
 * file, the screen...). With "pure" on, it has no tick pins and calls when
 * its outputs are read, like math/add: for functions that only compute
 * (sin, strlen). A wrong prototype or a bad pointer crashes lineit: it's C.
 * Compiled programs call it the same way, with libffi. */
#include "proto.h"

enum { PARAM_LIB,
       PARAM_FN,
       PARAM_PURE };

static Proto f;
static bool pure;
static void *lib, *fn;

/* Its pins, from the params: ticked or pure, an input per argument */
static void
reshape(Li_Node *n)
{
        const char *name = li_get_param(n, PARAM_LIB).s;
        pure             = li_get_param(n, PARAM_PURE).b;
        f.ok             = proto_parse(&f, li_get_param(n, PARAM_FN).s);

        if (lib) dlclose(lib);
        lib = fn = NULL;
        if (f.ok && !(lib = lib_open(name))) {
                snprintf(f.why, sizeof f.why, "no library %s", name);
                f.ok = false;
        }
        if (f.ok && !(fn = dlsym(lib, f.name))) {
                snprintf(f.why, sizeof f.why, "no %s in %s", f.name, *name ? name : "lineit");
                f.ok = false;
        }

        li_pins_reset(n);
        if (!pure) li_input(n, "tick", LI_TICK);
        for (int i = 0; f.ok && i < f.nargs; i++) {
                Arg *a = &f.args[i];
                switch (a->kind) {
                case K_FLOAT: li_input_value(n, a->name, LI_FLOAT, li_float(0)); break;
                case K_BOOL: li_input_value(n, a->name, LI_BOOL, li_bool(false)); break;
                case K_STRING:
                case K_CHARP: li_input_value(n, a->name, kind_type(a->kind, a->rec), li_string("")); break;
                case K_PTR:
                case K_REC:
                case K_RECP: li_input(n, a->name, kind_type(a->kind, a->rec)); break; /* only a wire */
                case K_OUT: break;                                                    /* only an output */
                default: li_input_value(n, a->name, LI_INT, li_int(0)); break;
                }
        }
        if (!pure) li_output(n, "then", LI_TICK);
        if (f.ok && f.ret.kind != K_VOID) li_output(n, "result", kind_type(f.ret.kind, f.ret.rec));
        for (int i = 0; f.ok && i < f.nargs; i++) {
                Arg *a = &f.args[i];
                if (a->kind == K_RECP && !a->is_const) li_output(n, a->name, a->rec);
                if (a->kind == K_OUT) li_output(n, a->name, out_type(a));
        }
}

void
module_init(Li_Node *n)
{
        li_param(n, "lib", li_string(""));
        li_param(n, "fn", li_string("int abs(int x)"));
        li_param(n, "pure", li_bool(false));
        reshape(n);
}

void
module_param(Li_Node *n, int param)
{
        (void) param;
        reshape(n);
}

/* Li_set and li_drop: V is owned */
static void
set_owned(Li_Node *n, int output, Li_Value v)
{
        li_set(n, output, v);
        li_drop(&v);
}

/* Call it with what the inputs have, and set the outputs */
static void
call(Li_Node *n)
{
        union {
                int64_t i;
                double d;
                void *p;
        } vals[ARGS_MAX];
        void *avalues[ARGS_MAX], *heap[ARGS_MAX] = { 0 };
        int input = pure ? 0 : 1, output = pure ? 0 : 1, nargs = f.nargs;
        bool ok = f.ok;

        if (!ok) return;
        for (int i = 0; i < nargs; i++) {
                Arg *a     = &f.args[i];
                Li_Value v = a->kind == K_OUT ? (Li_Value) { .type = LI_NONE } : li_get(n, input++);
                value_put(a, v, &vals[i], &heap[i]);
                avalues[i] = arg_value(a, &vals[i]);
                /* Records and out-parameters are in memory of their own */
                if ((a->kind == K_REC || a->kind == K_RECP || a->kind == K_OUT) && !heap[i]) ok = false;
        }

        /* Big enough for any result: an ffi_arg, a double, a record */
        size_t rsize = f.ret.kind == K_REC ? (size_t) f.ret.size : 0;
        if (rsize < sizeof(ffi_arg) + sizeof(double)) rsize = sizeof(ffi_arg) + sizeof(double);
        void *ret = calloc(1, rsize);
        if (ok && ret) {
                ffi_call(&f.cif, FFI_FN(fn), ret, avalues);
                if (f.ret.kind != K_VOID) set_owned(n, output++, return_value(&f.ret, ret));
                for (int i = 0; i < nargs; i++) {
                        Arg *a = &f.args[i];
                        if (!heap[i]) continue; /* only records and out-parameters come out */
                        if (a->kind == K_RECP && !a->is_const) set_owned(n, output++, li_c_read(a->rec, heap[i]));
                        if (a->kind == K_OUT) set_owned(n, output++, out_value(a, heap[i]));
                }
        }
        free(ret);
        for (int i = 0; i < nargs; i++)
                free(heap[i]);
}

void
module_tick(Li_Node *n, int input)
{
        (void) input;
        call(n);
        li_tick(n, 0);
}

void
module_eval(Li_Node *n)
{
        if (pure) call(n); /* ticked: its last outputs stay */
}

void
module_draw(Li_Node *n)
{
        if (f.ok)
                li_text(n, "%s%s", f.name, pure ? "()" : "");
        else
                li_text(n, "%s", f.why);
}

void
module_destroy(Li_Node *n)
{
        (void) n;
        if (lib) dlclose(lib);
        lib = NULL;
        proto_free(&f);
}

const char *
module_libs(void)
{
        return "-lffi -ldl";
}

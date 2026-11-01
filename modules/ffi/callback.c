/* A C function made of squares, for C functions that take one (qsort's
 * comparator, bsearch's, a library's handler). "fn" is its prototype, as
 * in ffi/call:
 *
 *   int cmp(const void *a, const void *b)
 *   double f(double x)
 *
 * "fn" (the output) is the function, a ptr: wire it to the argument of an
 * ffi/call that takes it (a void *, or any pointer). When C calls it, the
 * outputs after it are that call's arguments (a pointer to a record comes
 * out as the record), and what's wired to "result" is what it returns.
 * Only pure squares can compute it: what reads the arguments and gives
 * the result is computed when C calls, there's no tick. Branches can call
 * it at the same time: each call has its own arguments (see handler). Its
 * outputs have them only while C calls it: between calls, nothing. C
 * calling it from a thread of its own, not a branch's, must not do it
 * while lineit runs squares.
 *
 * With libffi's closures. Compiled programs make it the same way. */
#include "proto.h"

enum { PARAM_FN };
enum { IN_RESULT };
enum { OUT_FN };

static Proto f;
static ffi_closure *closure;
static void *code; /* the function: what C calls */
/* The arguments of the call running on this thread, owned. Branches can
 * call it at once, each from its thread: what computes the result reads
 * them on the same thread, so each sees its own call's. */
static _Thread_local Li_Value args[ARGS_MAX];

static void
args_clear(void)
{
        for (int i = 0; i < ARGS_MAX; i++)
                li_drop(&args[i]);
}

/* What C calls: the arguments to the outputs, the result back */
static void
handler(ffi_cif *cif, void *ret, void **avalues, void *data)
{
        (void) cif;
        Li_Node *n = data;
        Li_Value outer[ARGS_MAX]; /* of a call this one is inside, on this thread */
        memcpy(outer, args, sizeof outer);
        memset(args, 0, sizeof args);
        for (int i = 0; i < f.nargs; i++) {
                Arg *a  = &f.args[i];
                args[i] = value_at(a->kind, a->size, a->rec, avalues[i]);
        }
        /* What's wired to "result" reads the outputs: module_eval sets
         * them from ARGS */
        Li_Value r = f.ret.kind != K_VOID ? li_get(n, IN_RESULT) : (Li_Value) { .type = LI_NONE };
        return_put(&f.ret, r, ret);
        args_clear();
        memcpy(args, outer, sizeof args);
}

static void
closure_free(void)
{
        if (closure) ffi_closure_free(closure);
        closure = NULL;
        code    = NULL;
}

static void
reshape(Li_Node *n)
{
        closure_free();
        args_clear();
        f.ok = proto_parse(&f, li_get_param(n, PARAM_FN).s);
        if (f.ok && f.nfixed < f.nargs) {
                snprintf(f.why, sizeof f.why, "it can't be variadic");
                f.ok = false;
        }
        for (int i = 0; f.ok && i < f.nargs; i++) {
                if (f.args[i].kind != K_OUT) continue;
                snprintf(f.why, sizeof f.why, "%s[N]: write it as a pointer", f.args[i].name);
                f.ok = false;
        }
        if (f.ok) {
                closure = ffi_closure_alloc(sizeof *closure, &code);
                if (!closure || ffi_prep_closure_loc(closure, &f.cif, handler, n, code) != FFI_OK) {
                        snprintf(f.why, sizeof f.why, "libffi can't make it");
                        closure_free();
                        f.ok = false;
                }
        }

        li_pins_reset(n);
        Arg *r = &f.ret;
        if (f.ok) {
                switch (r->kind) {
                case K_VOID: li_input(n, "result", LI_ANY); break; /* nothing to give: unused */
                case K_FLOAT: li_input_value(n, "result", LI_FLOAT, li_float(0)); break;
                case K_BOOL: li_input_value(n, "result", LI_BOOL, li_bool(false)); break;
                case K_INT:
                case K_UINT: li_input_value(n, "result", LI_INT, li_int(0)); break;
                default: li_input(n, "result", kind_type(r->kind, r->rec)); break;
                }
        }
        li_output(n, "fn", LI_PTR);
        for (int i = 0; f.ok && i < f.nargs; i++) {
                Arg *a    = &f.args[i];
                Li_Type t = a->kind == K_CHARP ? LI_STRING : kind_type(a->kind, a->rec);
                li_output(n, a->name, t);
        }
}

void
module_init(Li_Node *n)
{
        li_param(n, "fn", li_string("int cmp(const void *a, const void *b)"));
        reshape(n);
}

void
module_param(Li_Node *n, int param)
{
        (void) param;
        reshape(n);
}

void
module_eval(Li_Node *n)
{
        li_set(n, OUT_FN, li_ptr(code));
        for (int i = 0; f.ok && i < f.nargs; i++)
                li_set(n, 1 + i, args[i]);
}

void
module_draw(Li_Node *n)
{
        li_text(n, "%s", f.ok ? f.name : f.why);
}

void
module_destroy(Li_Node *n)
{
        (void) n;
        closure_free();
        args_clear();
        proto_free(&f);
}

const char *
module_libs(void)
{
        return "-lffi";
}

/* Calls a C function of a library, as its prototype says, with libffi: no
 * module to write for it. "lib" is the library ("m" for libm, "c", a path
 * to a .so; empty: whatever lineit has loaded already, libc and libm
 * included), "fn" its prototype, as in C:
 *
 *   double pow(double x, double y)
 *   unsigned long strlen(const char *s)
 *   FILE *fopen(const char *path, const char *mode)
 *   int snprintf(char *buf, size_t n, const char *fmt, ..., int, double)
 *
 * The square takes the shape of it: an input per argument (named as in the
 * prototype, or a1, a2...) and "result" (none for void). A variadic one
 * says the types of this call's extra arguments after the dots.
 *
 * Integers are int, float and double are float, bool is bool, const char *
 * is a string (copied when it's the result), and any other pointer is a ptr,
 * passed along as it is (a char * takes a string, copied, or a ptr: a
 * buffer of ffi/alloc). Struct values aren't. ffi/alloc, ffi/read,
 * ffi/write and ffi/free work with what pointers point to.
 *
 * Ticked (tick in, then out), it calls on each tick: for what it does (a
 * file, the screen...). With "pure" on, it has no tick pins and calls when
 * its result is read, like math/add: for functions that only compute
 * (sin, strlen). A wrong prototype or a bad pointer crashes lineit: it's C.
 * Compiled programs call it the same way, with libffi. */
#include "lineit.h"
#include <ctype.h>
#include <dlfcn.h>
#include <ffi.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>
#include <time.h>

enum { PARAM_LIB,
       PARAM_FN,
       PARAM_PURE };

typedef enum Kind {
        K_VOID,
        K_INT,
        K_UINT,
        K_FLOAT,
        K_BOOL,
        K_STRING, /* const char * */
        K_CHARP,  /* char *: a string (copied) or a ptr */
        K_PTR,
} Kind;

typedef struct Arg {
        Kind kind;
        int size; /* in bytes */
        ffi_type *ffi;
        char name[32];
} Arg;

#define ARGS_MAX 16

/* What the prototype says, ready to call (each square has its own copy) */
static struct {
        bool ok;
        char why[96]; /* if not */
        char name[64];
        Arg ret, args[ARGS_MAX];
        int nargs, nfixed; /* nfixed < nargs: variadic */
        bool pure;
        void *lib, *fn;
        ffi_cif cif;
        ffi_type *types[ARGS_MAX];
} f;

/* ---- The prototype ---- */

typedef struct Tok {
        char text[32];
} Tok;

/* PROTO in tokens: names, * ( ) , and ... */
static int
tokenize(const char *p, Tok *toks, int max)
{
        int n = 0;
        while (*p && n < max) {
                if (isspace((unsigned char) *p)) {
                        p++;
                } else if (isalnum((unsigned char) *p) || *p == '_') {
                        int len = 0;
                        while ((isalnum((unsigned char) p[len]) || p[len] == '_') && len < 31)
                                len++;
                        snprintf(toks[n++].text, sizeof toks->text, "%.*s", len, p);
                        p += len;
                        while (isalnum((unsigned char) *p) || *p == '_')
                                p++;
                } else if (!strncmp(p, "...", 3)) {
                        snprintf(toks[n++].text, sizeof toks->text, "...");
                        p += 3;
                } else if (strchr("*(),", *p)) {
                        snprintf(toks[n++].text, sizeof toks->text, "%c", *p++);
                } else if (*p == '[') {
                        /* char buf[] is a char *buf */
                        while (*p && *p != ']')
                                p++;
                        if (*p) p++;
                        snprintf(toks[n++].text, sizeof toks->text, "*");
                } else {
                        return -1;
                }
        }
        return n;
}

/* Integer types and their size, signed or not */
static const struct {
        const char *name;
        int size;
        bool is_unsigned;
} ints[] = {
        { "char", sizeof(char), false },
        { "short", sizeof(short), false },
        { "int", sizeof(int), false },
        { "long", sizeof(long), false },
        { "size_t", sizeof(size_t), true },
        { "ssize_t", sizeof(ssize_t), false },
        { "intptr_t", sizeof(intptr_t), false },
        { "uintptr_t", sizeof(uintptr_t), true },
        { "ptrdiff_t", sizeof(ptrdiff_t), false },
        { "off_t", sizeof(off_t), false },
        { "int8_t", 1, false },
        { "uint8_t", 1, true },
        { "int16_t", 2, false },
        { "uint16_t", 2, true },
        { "int32_t", 4, false },
        { "uint32_t", 4, true },
        { "int64_t", 8, false },
        { "uint64_t", 8, true },
        { "time_t", sizeof(time_t), false },
        { "pid_t", sizeof(int), false },
        { "uid_t", sizeof(unsigned), true },
        { "mode_t", sizeof(unsigned), true },
        { "wchar_t", sizeof(int), false },
};

static bool
is_qualifier(const char *w)
{
        return !strcmp(w, "const") || !strcmp(w, "volatile") || !strcmp(w, "restrict") || !strcmp(w, "extern") ||
               !strcmp(w, "static") || !strcmp(w, "inline");
}

static bool
is_type_word(const char *w)
{
        if (!strcmp(w, "void") || !strcmp(w, "float") || !strcmp(w, "double") || !strcmp(w, "bool") ||
            !strcmp(w, "_Bool") || !strcmp(w, "signed") || !strcmp(w, "unsigned") || !strcmp(w, "long") ||
            !strcmp(w, "struct") || !strcmp(w, "union") || !strcmp(w, "enum"))
                return true;
        for (size_t i = 0; i < sizeof ints / sizeof *ints; i++)
                if (!strcmp(w, ints[i].name)) return true;
        return false;
}

static ffi_type *
int_ffi(int size, bool is_unsigned)
{
        switch (size) {
        case 1: return is_unsigned ? &ffi_type_uint8 : &ffi_type_sint8;
        case 2: return is_unsigned ? &ffi_type_uint16 : &ffi_type_sint16;
        case 4: return is_unsigned ? &ffi_type_uint32 : &ffi_type_sint32;
        default: return is_unsigned ? &ffi_type_uint64 : &ffi_type_sint64;
        }
}

/* The type in tokens T[0..N) into A, and its name if it has one (then
 * NAMED). False, with why in f.why, if it isn't one it knows. */
static bool
type_parse(Tok *t, int n, Arg *a, bool *named)
{
        int stars = 0, words = 0, longs = 0;
        bool is_unsigned = false, is_const = false, tagged = false;
        const char *base = NULL, *unknown = NULL;

        *named     = false;
        a->name[0] = '\0';
        for (int i = 0; i < n; i++) {
                const char *w = t[i].text;
                if (!strcmp(w, "*")) {
                        stars++;
                } else if (!strcmp(w, "const")) {
                        is_const = stars == 0 || is_const;
                } else if (is_qualifier(w)) {
                        continue;
                } else if (!strcmp(w, "struct") || !strcmp(w, "union") || !strcmp(w, "enum")) {
                        tagged = true;
                } else if (tagged) {
                        tagged  = false; /* the tag: struct timeval */
                        unknown = w;
                        words++;
                } else if (!strcmp(w, "unsigned")) {
                        is_unsigned = true;
                        words++;
                } else if (!strcmp(w, "signed")) {
                        words++;
                } else if (!strcmp(w, "long")) {
                        longs++;
                        words++;
                } else if (is_type_word(w)) {
                        base = w;
                        words++;
                } else if (words && i == n - 1) {
                        snprintf(a->name, sizeof a->name, "%s", w); /* its name */
                        *named = true;
                } else {
                        unknown = w; /* a typedef it doesn't know: fine behind a * */
                        words++;
                }
        }
        if (!words) {
                snprintf(f.why, sizeof f.why, "a type is missing");
                return false;
        }

        if (stars) {
                bool chr = base && !strcmp(base, "char") && stars == 1 && !unknown;
                a->kind  = chr ? (is_const ? K_STRING : K_CHARP) : K_PTR;
                a->size  = sizeof(void *);
                a->ffi   = &ffi_type_pointer;
                return true;
        }
        if (unknown) {
                snprintf(f.why, sizeof f.why, "unknown type %s (a pointer to it is fine)", unknown);
                return false;
        }
        if (!base && longs) base = "long"; /* long, long long, unsigned long */
        if (!base) base = "int";           /* signed, unsigned */
        if (!strcmp(base, "void")) {
                a->kind = K_VOID, a->size = 0, a->ffi = &ffi_type_void;
        } else if (!strcmp(base, "float")) {
                a->kind = K_FLOAT, a->size = sizeof(float), a->ffi = &ffi_type_float;
        } else if (!strcmp(base, "double")) {
                if (longs) {
                        snprintf(f.why, sizeof f.why, "long double isn't supported");
                        return false;
                }
                a->kind = K_FLOAT, a->size = sizeof(double), a->ffi = &ffi_type_double;
        } else if (!strcmp(base, "bool") || !strcmp(base, "_Bool")) {
                a->kind = K_BOOL, a->size = sizeof(bool), a->ffi = &ffi_type_uint8;
        } else {
                int size = sizeof(int);
                for (size_t i = 0; i < sizeof ints / sizeof *ints; i++) {
                        if (strcmp(base, ints[i].name)) continue;
                        size        = ints[i].size;
                        is_unsigned = is_unsigned || ints[i].is_unsigned;
                }
                if (longs) size = longs > 1 ? sizeof(long long) : sizeof(long);
                a->kind = is_unsigned ? K_UINT : K_INT;
                a->size = size;
                a->ffi  = int_ffi(size, is_unsigned);
        }
        return true;
}

/* Arguments of variadic functions are promoted: float to double, small
 * integers to int */
static void
promote(Arg *a)
{
        if (a->kind == K_FLOAT && a->size == sizeof(float)) a->size = sizeof(double), a->ffi = &ffi_type_double;
        if ((a->kind == K_INT || a->kind == K_UINT || a->kind == K_BOOL) && a->size < (int) sizeof(int)) {
                a->kind = a->kind == K_UINT ? K_UINT : K_INT;
                a->size = sizeof(int);
                a->ffi  = int_ffi(sizeof(int), a->kind == K_UINT);
        }
}

/* PROTO into f. False, with why, if it isn't one it can call. */
static bool
proto_parse(const char *proto)
{
        Tok t[96];
        int n = tokenize(proto ? proto : "", t, 96);
        bool named;
        int open = -1;

        f.nargs = f.nfixed = 0;
        if (n < 0) {
                snprintf(f.why, sizeof f.why, "can't read it as C");
                return false;
        }
        for (int i = 0; i < n && open < 0; i++)
                if (!strcmp(t[i].text, "(")) open = i;
        if (open < 2 || strcmp(t[n - 1].text, ")")) {
                snprintf(f.why, sizeof f.why, "write it like: double pow(double, double)");
                return false;
        }
        /* Its name is the last word before the parenthesis */
        snprintf(f.name, sizeof f.name, "%s", t[open - 1].text);
        if (!type_parse(t, open - 1, &f.ret, &named)) return false;

        /* Arguments, between commas. (void) is none. */
        bool varargs = false;
        int start    = open + 1;
        for (int i = open + 1; i < n; i++) {
                if (strcmp(t[i].text, ",") && i != n - 1) continue;
                int len = i - start;
                if (len == 1 && !strcmp(t[start].text, "...")) {
                        varargs  = true;
                        f.nfixed = f.nargs;
                } else if (len == 1 && !strcmp(t[start].text, "void") && f.nargs == 0 && i == n - 1) {
                        /* (void) */
                } else if (len > 0) {
                        if (f.nargs == ARGS_MAX) {
                                snprintf(f.why, sizeof f.why, "more than %d arguments", ARGS_MAX);
                                return false;
                        }
                        Arg *a = &f.args[f.nargs];
                        if (!type_parse(&t[start], len, a, &named)) return false;
                        if (a->kind == K_VOID) {
                                snprintf(f.why, sizeof f.why, "an argument can't be void");
                                return false;
                        }
                        if (varargs) promote(a);
                        if (!named) snprintf(a->name, sizeof a->name, "a%d", f.nargs + 1);
                        f.nargs++;
                }
                start = i + 1;
        }
        if (!varargs) f.nfixed = f.nargs;

        for (int i = 0; i < f.nargs; i++)
                f.types[i] = f.args[i].ffi;
        ffi_status st = varargs ? ffi_prep_cif_var(&f.cif, FFI_DEFAULT_ABI, f.nfixed, f.nargs, f.ret.ffi, f.types) :
                                  ffi_prep_cif(&f.cif, FFI_DEFAULT_ABI, f.nargs, f.ret.ffi, f.types);
        if (st != FFI_OK) {
                snprintf(f.why, sizeof f.why, "libffi can't call it");
                return false;
        }
        return true;
}

/* ---- The library ---- */

/* LIB: "m" is libm.so (or libm.so.6...), a path or a .so is itself, nothing
 * is what's loaded already */
static void *
lib_open(const char *lib)
{
        char path[256];
        void *h;

        if (!lib || !*lib) return dlopen(NULL, RTLD_NOW); /* lineit, and what it has loaded */
        if (strchr(lib, '/') || strstr(lib, ".so")) return dlopen(lib, RTLD_NOW | RTLD_LOCAL);
        snprintf(path, sizeof path, "lib%s.so", lib);
        if ((h = dlopen(path, RTLD_NOW | RTLD_LOCAL))) return h;
        /* libm.so may be a linker script: the real one has a version */
        for (int v = 0; v < 10; v++) {
                snprintf(path, sizeof path, "lib%s.so.%d", lib, v);
                if ((h = dlopen(path, RTLD_NOW | RTLD_LOCAL))) return h;
        }
        return NULL;
}

/* ---- The square ---- */

static Li_Type
lineit_type(Kind k)
{
        switch (k) {
        case K_INT:
        case K_UINT: return LI_INT;
        case K_FLOAT: return LI_FLOAT;
        case K_BOOL: return LI_BOOL;
        case K_STRING: return LI_STRING;
        case K_CHARP: return LI_ANY; /* a string or a ptr */
        case K_PTR: return LI_PTR;
        default: return LI_NONE;
        }
}

/* Its pins, from the params: ticked or pure, an input per argument */
static void
reshape(Li_Node *n)
{
        const char *lib = li_get_param(n, PARAM_LIB).s;
        f.pure          = li_get_param(n, PARAM_PURE).b;
        f.ok            = proto_parse(li_get_param(n, PARAM_FN).s);

        if (f.lib) dlclose(f.lib);
        f.lib = NULL;
        f.fn  = NULL;
        if (f.ok && !(f.lib = lib_open(lib))) {
                snprintf(f.why, sizeof f.why, "no library %s", lib);
                f.ok = false;
        }
        if (f.ok && !(f.fn = dlsym(f.lib, f.name))) {
                snprintf(f.why, sizeof f.why, "no %s in %s", f.name, *lib ? lib : "lineit");
                f.ok = false;
        }

        li_pins_reset(n);
        if (!f.pure) li_input(n, "tick", LI_TICK);
        for (int i = 0; f.ok && i < f.nargs; i++) {
                Arg *a = &f.args[i];
                switch (a->kind) {
                case K_FLOAT: li_input_value(n, a->name, LI_FLOAT, li_float(0)); break;
                case K_BOOL: li_input_value(n, a->name, LI_BOOL, li_bool(false)); break;
                case K_STRING:
                case K_CHARP: li_input_value(n, a->name, lineit_type(a->kind), li_string("")); break;
                case K_PTR: li_input(n, a->name, LI_PTR); break; /* only a wire: a ptr can't be typed */
                default: li_input_value(n, a->name, LI_INT, li_int(0)); break;
                }
        }
        if (!f.pure) li_output(n, "then", LI_TICK);
        if (f.ok && f.ret.kind != K_VOID) li_output(n, "result", lineit_type(f.ret.kind));
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

static long long
as_int(Li_Value v)
{
        return v.type == LI_INT ? v.i : v.type == LI_FLOAT ? (long long) v.f :
                                        v.type == LI_BOOL  ? v.b :
                                                             0;
}

/* An integer of SIZE bytes at P */
static void
int_put(void *p, int size, long long x)
{
        switch (size) {
        case 1: *(int8_t *) p = (int8_t) x; break;
        case 2: *(int16_t *) p = (int16_t) x; break;
        case 4: *(int32_t *) p = (int32_t) x; break;
        default: *(int64_t *) p = (int64_t) x; break;
        }
}

/* The integer libffi gives back for a result of A (widened to ffi_arg) */
static long long
int_get(ffi_arg r, const Arg *a)
{
        if (a->kind == K_UINT) {
                switch (a->size) {
                case 1: return (uint8_t) r;
                case 2: return (uint16_t) r;
                case 4: return (uint32_t) r;
                default: return (long long) (uint64_t) r;
                }
        }
        switch (a->size) {
        case 1: return (int8_t) r;
        case 2: return (int16_t) r;
        case 4: return (int32_t) r;
        default: return (int64_t) r;
        }
}

/* Call it with what the inputs have. Its result, LI_NONE for void. */
static Li_Value
call(Li_Node *n)
{
        union {
                int64_t i;
                double d;
                float fl;
                void *p;
        } vals[ARGS_MAX];
        void *avalues[ARGS_MAX];
        char *copies[ARGS_MAX] = { 0 };
        char text[64];
        int first = f.pure ? 0 : 1;

        if (!f.ok) return (Li_Value) { .type = LI_NONE };
        for (int i = 0; i < f.nargs; i++) {
                Arg *a     = &f.args[i];
                Li_Value v = li_get(n, first + i);
                avalues[i] = &vals[i];
                switch (a->kind) {
                case K_FLOAT: {
                        double d = v.type == LI_FLOAT ? v.f : (double) as_int(v);
                        if (a->size == sizeof(float))
                                vals[i].fl = (float) d;
                        else
                                vals[i].d = d;
                        break;
                }
                case K_STRING:
                        if (v.type == LI_STRING) {
                                vals[i].p = (void *) v.s;
                        } else {
                                li_format(v, text, sizeof text);
                                vals[i].p = copies[i] = strdup(v.type == LI_NONE ? "" : text);
                        }
                        break;
                case K_CHARP:
                        /* A buffer (ptr) is passed as it is, a string as a copy it can change */
                        if (v.type == LI_PTR) {
                                vals[i].p = v.p;
                        } else {
                                li_format(v, text, sizeof text);
                                vals[i].p = copies[i] = strdup(v.type == LI_STRING ? v.s : v.type == LI_NONE ? "" :
                                                                                                               text);
                        }
                        break;
                case K_PTR:
                        vals[i].p = NULL;
                        if (v.type == LI_PTR) vals[i].p = v.p;
                        if (v.type == LI_INT) vals[i].p = (void *) (intptr_t) v.i; /* an address */
                        break;
                        break;
                default: int_put(&vals[i], a->size, as_int(v)); break;
                }
        }

        union {
                ffi_arg a;
                double d;
                float fl;
                void *p;
        } r = { 0 };
        ffi_call(&f.cif, FFI_FN(f.fn), &r, avalues);

        Li_Value out = { .type = LI_NONE };
        switch (f.ret.kind) {
        case K_INT:
        case K_UINT: out = li_int((long) int_get(r.a, &f.ret)); break;
        case K_FLOAT: out = li_float(f.ret.size == sizeof(float) ? r.fl : r.d); break;
        case K_BOOL: out = li_bool((uint8_t) r.a != 0); break;
        case K_STRING:
        case K_CHARP:
                if (r.p) out = li_string(r.p); /* li_set copies it */
                break;
        case K_PTR: out = li_ptr(r.p); break;
        default: break;
        }
        for (int i = 0; i < f.nargs; i++)
                free(copies[i]);
        return out;
}

void
module_tick(Li_Node *n, int input)
{
        (void) input;
        Li_Value v = call(n);
        if (f.ok && f.ret.kind != K_VOID) li_set(n, 1, v);
        li_tick(n, 0);
}

void
module_eval(Li_Node *n)
{
        if (!f.pure) return; /* ticked: its last result stays */
        Li_Value v = call(n);
        if (f.ok && f.ret.kind != K_VOID) li_set(n, 0, v);
}

void
module_draw(Li_Node *n)
{
        if (f.ok)
                li_text(n, "%s%s", f.name, f.pure ? "()" : "");
        else
                li_text(n, "%s", f.why);
}

void
module_destroy(Li_Node *n)
{
        (void) n;
        if (f.lib) dlclose(f.lib);
        f.lib = NULL;
}

const char *
module_libs(void)
{
        return "-lffi -ldl";
}

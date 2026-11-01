/* Shared by ffi/call and ffi/callback: a C prototype read into what libffi
 * needs to call it (or to be called as it), and values between lineit and
 * C. Each square has its own copy of a module, so of its Proto too.
 *
 * Types:
 *   integers of any size and sign    int (uint64 above INT64_MAX wraps)
 *   float, double                    float
 *   bool, _Bool                      bool
 *   const char *                     string
 *   char *                           a string (a copy C may change), bytes
 *                                    or a ptr
 *   other pointers                   ptr (bytes too: their data)
 *   a custom type with a C layout    a record, by value: point p
 *   a pointer to one                 a record: C gets a pointer to a copy,
 *                                    and if it isn't const, what C left in
 *                                    it comes back out: point *p
 *   T name[N]                        out: N of T, zeroed, for C to fill; it
 *                                    comes out as a string (char), bytes
 *                                    (unsigned char, uint8_t), T (N is 1)
 *                                    or a list of T
 * A custom type is found by its name ("types/point") or its last part
 * ("point"), with struct before it or not. */
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

typedef enum Kind {
        K_VOID,
        K_INT,
        K_UINT,
        K_FLOAT,
        K_BOOL,
        K_STRING, /* const char * */
        K_CHARP,  /* char *: a string (copied), bytes or a ptr */
        K_PTR,
        K_REC,  /* a record by value */
        K_RECP, /* a pointer to one */
        K_OUT,  /* T name[N] */
} Kind;

typedef struct Arg {
        Kind kind;
        int size; /* in bytes */
        ffi_type *ffi;
        char name[32];
        Li_Type rec;   /* K_REC, K_RECP, and K_OUT of records */
        bool is_const; /* K_RECP: C only reads it, nothing comes out */
        bool is_char;  /* char, signed char: K_OUT of them is a string */
        /* K_OUT: COUNT of ELEM, ELEM_SIZE bytes each */
        Kind elem;
        int elem_size, count;
} Arg;

enum { ARGS_MAX  = 16,
       MADE_MAX  = 32, /* struct types made for libffi, nested ones too */
       OUT_LIMIT = 1 << 20 };

typedef struct Made {
        ffi_type type;
        ffi_type *elements[]; /* the last one NULL */
} Made;

typedef struct Proto {
        bool ok;
        char why[96]; /* if not */
        char name[64];
        Arg ret, args[ARGS_MAX];
        int nargs, nfixed; /* nfixed < nargs: variadic */
        ffi_cif cif;
        ffi_type *types[ARGS_MAX];
        Made *made[MADE_MAX];
        int nmade;
} Proto;

static inline void
proto_free(Proto *p)
{
        for (int i = 0; i < p->nmade; i++)
                free(p->made[i]);
        p->nmade = 0;
}

/* ---- Reading it ---- */

typedef struct Tok {
        char text[32];
} Tok;

/* PROTO in tokens: names, * ( ) , ... and [N] (char buf[] is char *buf) */
static inline int
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
                        const char *end = strchr(p, ']');
                        if (!end) return -1;
                        long count = strtol(p + 1, NULL, 10);
                        if (count > 0)
                                snprintf(toks[n++].text, sizeof toks->text, "[%ld]", count);
                        else
                                snprintf(toks[n++].text, sizeof toks->text, "*");
                        p = end + 1;
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

static inline bool
is_qualifier(const char *w)
{
        return !strcmp(w, "const") || !strcmp(w, "volatile") || !strcmp(w, "restrict") || !strcmp(w, "extern") ||
               !strcmp(w, "static") || !strcmp(w, "inline");
}

static inline bool
is_type_word(const char *w)
{
        if (!strcmp(w, "void") || !strcmp(w, "float") || !strcmp(w, "double") || !strcmp(w, "bool") ||
            !strcmp(w, "_Bool"))
                return true;
        for (size_t i = 0; i < sizeof ints / sizeof *ints; i++)
                if (!strcmp(w, ints[i].name)) return true;
        return false;
}

static inline ffi_type *
int_ffi(int size, bool is_unsigned)
{
        switch (size) {
        case 1: return is_unsigned ? &ffi_type_uint8 : &ffi_type_sint8;
        case 2: return is_unsigned ? &ffi_type_uint16 : &ffi_type_sint16;
        case 4: return is_unsigned ? &ffi_type_uint32 : &ffi_type_sint32;
        default: return is_unsigned ? &ffi_type_uint64 : &ffi_type_sint64;
        }
}

/* Record type T for libffi, as C lays it out (see li_c_members) */
static inline ffi_type *
rec_ffi(Proto *p, Li_Type t, int depth)
{
        Li_C_Member m[64];
        int n = li_c_members(t, m, 64);
        if (n <= 0 || depth > 16 || p->nmade == MADE_MAX) return NULL;
        Made *st = calloc(1, sizeof *st + (n + 1) * sizeof *st->elements);
        if (!st) return NULL;
        p->made[p->nmade++] = st;
        st->type.type       = FFI_TYPE_STRUCT;
        st->type.elements   = st->elements;
        for (int i = 0; i < n; i++) {
                ffi_type *e = NULL;
                switch (m[i].kind) {
                case 'i': e = int_ffi(m[i].size, false); break;
                case 'u': e = int_ffi(m[i].size, true); break;
                case 'f': e = m[i].size == 4 ? &ffi_type_float : &ffi_type_double; break;
                case 'p': e = &ffi_type_pointer; break;
                case 'r': e = rec_ffi(p, m[i].type, depth + 1); break;
                }
                if (!e) return NULL;
                st->elements[i] = e;
        }
        return &st->type;
}

/* A custom type with a C layout named NAME, LI_NONE if there's none */
static inline Li_Type
rec_named(const char *name)
{
        Li_Type t = li_type_named(name);
        return t != LI_NONE && li_c_size(t) ? t : LI_NONE;
}

/* The type in tokens T[0..N) into A, and its name if it has one (then
 * NAMED). False, with why in P, if it isn't one it knows. */
static inline bool
type_parse(Proto *p, Tok *t, int n, Arg *a, bool *named)
{
        int stars = 0, words = 0, longs = 0, count = 0;
        bool is_unsigned = false, is_const = false, tagged = false;
        const char *base = NULL, *unknown = NULL;

        memset(a, 0, sizeof *a);
        a->rec = LI_NONE;
        *named = false;
        if (n > 0 && t[n - 1].text[0] == '[') count = atoi(t[--n].text + 1); /* name[N] */
        for (int i = 0; i < n; i++) {
                const char *w = t[i].text;
                if (!strcmp(w, "*")) {
                        stars++;
                } else if (w[0] == '[') {
                        snprintf(p->why, sizeof p->why, "only one [N], after the name");
                        return false;
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
                        unknown = w; /* a typedef or a custom type */
                        words++;
                }
        }
        if (!words) {
                snprintf(p->why, sizeof p->why, "a type is missing");
                return false;
        }

        Li_Type rec = unknown ? rec_named(unknown) : LI_NONE;
        if (count) {
                /* An out-parameter: its elements are the type without [N] */
                if (stars) {
                        a->elem = K_PTR, a->elem_size = sizeof(void *);
                } else if (rec != LI_NONE) {
                        a->elem = K_REC, a->elem_size = (int) li_c_size(rec), a->rec = rec;
                } else if (unknown) {
                        snprintf(p->why, sizeof p->why, "unknown type %s", unknown);
                        return false;
                } else {
                        Arg e;
                        bool en;
                        Tok bt[4];
                        int bn = 0;
                        if (is_unsigned) snprintf(bt[bn++].text, sizeof bt->text, "unsigned");
                        for (int i = 0; i < longs && bn < 3; i++)
                                snprintf(bt[bn++].text, sizeof bt->text, "long");
                        if (base) snprintf(bt[bn++].text, sizeof bt->text, "%s", base);
                        if (!type_parse(p, bt, bn, &e, &en)) return false;
                        if (e.kind == K_VOID) {
                                snprintf(p->why, sizeof p->why, "an array of void");
                                return false;
                        }
                        a->elem = e.kind, a->elem_size = e.size, a->is_char = e.is_char;
                }
                if ((long) count * a->elem_size > OUT_LIMIT) {
                        snprintf(p->why, sizeof p->why, "%s[%d] is too big", a->name, count);
                        return false;
                }
                a->kind  = K_OUT;
                a->count = count;
                a->size  = sizeof(void *);
                a->ffi   = &ffi_type_pointer;
                return true;
        }
        if (stars) {
                bool chr = base && !strcmp(base, "char") && stars == 1 && !unknown;
                a->size  = sizeof(void *);
                a->ffi   = &ffi_type_pointer;
                if (rec != LI_NONE && stars == 1) {
                        a->kind     = K_RECP;
                        a->rec      = rec;
                        a->is_const = is_const;
                } else {
                        a->kind = chr ? (is_const ? K_STRING : K_CHARP) : K_PTR;
                }
                return true;
        }
        if (rec != LI_NONE) {
                a->kind = K_REC;
                a->rec  = rec;
                a->size = (int) li_c_size(rec);
                if (!(a->ffi = rec_ffi(p, rec, 0))) {
                        snprintf(p->why, sizeof p->why, "libffi can't lay %s out", unknown);
                        return false;
                }
                return true;
        }
        if (unknown) {
                snprintf(p->why, sizeof p->why, "unknown type %s (a pointer to it is fine)", unknown);
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
                        snprintf(p->why, sizeof p->why, "long double isn't supported");
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
                a->kind    = is_unsigned ? K_UINT : K_INT;
                a->size    = size;
                a->ffi     = int_ffi(size, is_unsigned);
                a->is_char = !strcmp(base, "char") && !is_unsigned;
        }
        return true;
}

/* Arguments of variadic functions are promoted: float to double, small
 * integers to int */
static inline void
promote(Arg *a)
{
        if (a->kind == K_FLOAT && a->size == sizeof(float)) a->size = sizeof(double), a->ffi = &ffi_type_double;
        if ((a->kind == K_INT || a->kind == K_UINT || a->kind == K_BOOL) && a->size < (int) sizeof(int)) {
                a->kind = a->kind == K_UINT ? K_UINT : K_INT;
                a->size = sizeof(int);
                a->ffi  = int_ffi(sizeof(int), a->kind == K_UINT);
        }
}

/* The struct types libffi laid out have the size lineit's layout says */
static inline bool
layout_agrees(Proto *p, const Arg *a)
{
        if (a->kind != K_REC || a->ffi->size == li_c_size(a->rec)) return true;
        snprintf(p->why, sizeof p->why, "libffi lays %s out differently", a->name[0] ? a->name : "a record");
        return false;
}

/* PROTO into P. False, with why, if it isn't one it can use. */
static inline bool
proto_parse(Proto *p, const char *proto)
{
        Tok t[96];
        int n = tokenize(proto ? proto : "", t, 96);
        bool named;
        int open = -1;

        proto_free(p);
        p->nargs = p->nfixed = 0;
        p->why[0]            = '\0';
        if (n < 0) {
                snprintf(p->why, sizeof p->why, "can't read it as C");
                return false;
        }
        for (int i = 0; i < n && open < 0; i++)
                if (!strcmp(t[i].text, "(")) open = i;
        if (open < 2 || strcmp(t[n - 1].text, ")")) {
                snprintf(p->why, sizeof p->why, "write it like: double pow(double, double)");
                return false;
        }
        /* Its name is the last word before the parenthesis */
        snprintf(p->name, sizeof p->name, "%s", t[open - 1].text);
        if (!type_parse(p, t, open - 1, &p->ret, &named)) return false;
        if (p->ret.kind == K_OUT) {
                snprintf(p->why, sizeof p->why, "a function can't return an array");
                return false;
        }

        /* Arguments, between commas. (void) is none. */
        bool varargs = false;
        int start    = open + 1;
        for (int i = open + 1; i < n; i++) {
                if (strcmp(t[i].text, ",") && i != n - 1) continue;
                int len = i - start;
                if (len == 1 && !strcmp(t[start].text, "...")) {
                        varargs   = true;
                        p->nfixed = p->nargs;
                } else if (len == 1 && !strcmp(t[start].text, "void") && p->nargs == 0 && i == n - 1) {
                        /* (void) */
                } else if (len > 0) {
                        if (p->nargs == ARGS_MAX) {
                                snprintf(p->why, sizeof p->why, "more than %d arguments", ARGS_MAX);
                                return false;
                        }
                        Arg *a = &p->args[p->nargs];
                        if (!type_parse(p, &t[start], len, a, &named)) return false;
                        if (a->kind == K_VOID) {
                                snprintf(p->why, sizeof p->why, "an argument can't be void");
                                return false;
                        }
                        if (varargs && a->kind == K_REC) {
                                snprintf(p->why, sizeof p->why, "a record can't be a variadic argument");
                                return false;
                        }
                        if (varargs) promote(a);
                        if (!named) snprintf(a->name, sizeof a->name, "a%d", p->nargs + 1);
                        p->nargs++;
                }
                start = i + 1;
        }
        if (!varargs) p->nfixed = p->nargs;

        for (int i = 0; i < p->nargs; i++)
                p->types[i] = p->args[i].ffi;
        ffi_status st = varargs ? ffi_prep_cif_var(&p->cif, FFI_DEFAULT_ABI, p->nfixed, p->nargs, p->ret.ffi, p->types) :
                                  ffi_prep_cif(&p->cif, FFI_DEFAULT_ABI, p->nargs, p->ret.ffi, p->types);
        if (st != FFI_OK) {
                snprintf(p->why, sizeof p->why, "libffi can't call it");
                return false;
        }
        /* ffi_prep_cif laid the structs out: they have to be C's */
        if (!layout_agrees(p, &p->ret)) return false;
        for (int i = 0; i < p->nargs; i++)
                if (!layout_agrees(p, &p->args[i])) return false;
        return true;
}

/* ---- The library ---- */

/* LIB: "m" is libm.so (or libm.so.6...), a path or a .so is itself, nothing
 * is what's loaded already */
static inline void *
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

/* ---- Values ---- */

/* The lineit type of a value of kind K (records: REC) */
static inline Li_Type
kind_type(Kind k, Li_Type rec)
{
        switch (k) {
        case K_INT:
        case K_UINT: return LI_INT;
        case K_FLOAT: return LI_FLOAT;
        case K_BOOL: return LI_BOOL;
        case K_STRING: return LI_STRING;
        case K_CHARP: return LI_ANY; /* a string, bytes or a ptr */
        case K_PTR: return LI_PTR;
        case K_REC:
        case K_RECP: return rec;
        default: return LI_NONE;
        }
}

/* What an out-parameter comes out as */
static inline Li_Type
out_type(const Arg *a)
{
        if (a->elem == K_INT && a->elem_size == 1) return a->is_char ? LI_STRING : LI_BYTES;
        if (a->elem == K_UINT && a->elem_size == 1) return LI_BYTES;
        return a->count == 1 ? kind_type(a->elem, a->rec) : LI_LIST;
}

static inline long long
as_int(Li_Value v)
{
        return v.type == LI_INT ? v.i : v.type == LI_FLOAT ? (long long) v.f :
                                        v.type == LI_BOOL  ? v.b :
                                                             0;
}

/* An integer of SIZE bytes at P */
static inline void
int_put(void *p, int size, long long x)
{
        switch (size) {
        case 1: *(int8_t *) p = (int8_t) x; break;
        case 2: *(int16_t *) p = (int16_t) x; break;
        case 4: *(int32_t *) p = (int32_t) x; break;
        default: *(int64_t *) p = (int64_t) x; break;
        }
}

static inline long long
int_at(const void *p, int size, bool is_unsigned)
{
        switch (size) {
        case 1: return is_unsigned ? *(const uint8_t *) p : *(const int8_t *) p;
        case 2: return is_unsigned ? *(const uint16_t *) p : *(const int16_t *) p;
        case 4: return is_unsigned ? (long long) *(const uint32_t *) p : *(const int32_t *) p;
        default: return *(const int64_t *) p;
        }
}

/* The value of kind K, SIZE bytes, at P: owned, li_drop it */
static inline Li_Value
value_at(Kind k, int size, Li_Type rec, const void *p)
{
        switch (k) {
        case K_INT:
        case K_UINT: return li_int((long) int_at(p, size, k == K_UINT));
        case K_FLOAT: return li_float(size == sizeof(float) ? *(const float *) p : *(const double *) p);
        case K_BOOL: return li_bool(*(const uint8_t *) p != 0);
        case K_STRING:
        case K_CHARP: {
                const char *s = *(const char *const *) p;
                return s ? li_copy(li_string(s)) : (Li_Value) { .type = LI_NONE };
        }
        case K_PTR: return li_ptr(*(void *const *) p);
        case K_REC: return li_c_read(rec, p);
        case K_RECP: {
                const void *r = *(const void *const *) p;
                return r ? li_c_read(rec, r) : (Li_Value) { .type = LI_NONE };
        }
        default: return (Li_Value) { .type = LI_NONE };
        }
}

/* What an out-parameter A has in BUF, as out_type says: owned */
static inline Li_Value
out_value(const Arg *a, const unsigned char *buf)
{
        Li_Type t = out_type(a);
        if (t == LI_STRING) return li_copy(li_string((const char *) buf)); /* buf has a '\0' after it */
        if (t == LI_BYTES) return li_bytes(buf, a->count);
        if (a->count == 1) return value_at(a->elem, a->elem_size, a->rec, buf);
        Li_Value list = { .type = LI_NONE };
        for (int i = 0; i < a->count; i++) {
                Li_Value v = value_at(a->elem, a->elem_size, a->rec, buf + (size_t) i * a->elem_size);
                li_list_push(&list, v); /* copies it */
                li_drop(&v);
        }
        return list;
}

/* The pointer a ptr, bytes or an address (an int) is */
static inline void *
as_pointer(Li_Value v)
{
        if (v.type == LI_PTR) return v.p;
        if (v.type == LI_BYTES) return li_bytes_data(v);
        if (v.type == LI_INT) return (void *) (intptr_t) v.i;
        return NULL;
}

/* V as a C value of A at SLOT (sizeof a pointer at least, and 8): for
 * strings and records it may need memory, then in *HEAP, to free after */
static inline void
value_put(const Arg *a, Li_Value v, void *slot, void **heap)
{
        char text[64];
        *heap = NULL;
        switch (a->kind) {
        case K_FLOAT: {
                double d = v.type == LI_FLOAT ? v.f : (double) as_int(v);
                if (a->size == sizeof(float))
                        *(float *) slot = (float) d;
                else
                        *(double *) slot = d;
                break;
        }
        case K_BOOL: *(uint8_t *) slot = as_int(v) != 0; break;
        case K_STRING:
                if (v.type == LI_STRING) {
                        *(const char **) slot = v.s;
                } else if (v.type == LI_BYTES) {
                        *(void **) slot = li_bytes_data(v); /* they end in a '\0' */
                } else {
                        li_format(v, text, sizeof text);
                        *(char **) slot = *heap = strdup(v.type == LI_NONE ? "" : text);
                }
                break;
        case K_CHARP:
                /* A buffer is passed as it is, a string as a copy C can change */
                if (v.type == LI_PTR || v.type == LI_BYTES) {
                        *(void **) slot = as_pointer(v);
                } else {
                        li_format(v, text, sizeof text);
                        *(char **) slot = *heap = strdup(v.type == LI_STRING ? v.s ? v.s : "" :
                                                         v.type == LI_NONE   ? "" :
                                                                               text);
                }
                break;
        case K_PTR: *(void **) slot = as_pointer(v); break;
        case K_REC:
        case K_RECP: {
                /* A copy of the record as C lays it out; zero if it isn't one */
                void *mem = calloc(1, li_c_size(a->rec));
                if (v.type == a->rec) li_c_write(a->rec, v, mem);
                *(void **) slot = *heap = mem; /* by value, libffi reads MEM itself: see arg_value */
                break;
        }
        case K_OUT:
                /* Zeroed, and a '\0' after it, so a char buffer is a string */
                *(void **) slot = *heap = calloc(1, (size_t) a->count * a->elem_size + 1);
                break;
        default: int_put(slot, a->size, as_int(v)); break;
        }
}

/* Where libffi reads argument A from, once value_put put it in SLOT */
static inline void *
arg_value(const Arg *a, void *slot)
{
        return a->kind == K_REC ? *(void **) slot : slot;
}

/* V as what C returns of A at RET (libffi widens small integers to an
 * ffi_arg) */
static inline void
return_put(const Arg *a, Li_Value v, void *ret)
{
        switch (a->kind) {
        case K_VOID: break;
        case K_INT:
        case K_BOOL: *(ffi_sarg *) ret = (ffi_sarg) (a->kind == K_BOOL ? as_int(v) != 0 : as_int(v)); break;
        case K_UINT: *(ffi_arg *) ret = (ffi_arg) as_int(v); break;
        case K_FLOAT:
                if (a->size == sizeof(float))
                        *(float *) ret = (float) (v.type == LI_FLOAT ? v.f : (double) as_int(v));
                else
                        *(double *) ret = v.type == LI_FLOAT ? v.f : (double) as_int(v);
                break;
        case K_REC:
                memset(ret, 0, a->size);
                if (v.type == a->rec) li_c_write(a->rec, v, ret);
                break;
        default: *(void **) ret = as_pointer(v); break; /* strings would dangle: a ptr */
        }
}

/* What C returned of A at RET, owned */
static inline Li_Value
return_value(const Arg *a, const void *ret)
{
        switch (a->kind) {
        case K_INT:
        case K_UINT: {
                ffi_arg r = *(const ffi_arg *) ret;
                bool u    = a->kind == K_UINT;
                switch (a->size) {
                case 1: return li_int(u ? (long) (uint8_t) r : (long) (int8_t) r);
                case 2: return li_int(u ? (long) (uint16_t) r : (long) (int16_t) r);
                case 4: return li_int(u ? (long) (uint32_t) r : (long) (int32_t) r);
                default: return li_int((long) r);
                }
        }
        case K_BOOL: return li_bool((uint8_t) *(const ffi_arg *) ret != 0);
        case K_FLOAT: return value_at(K_FLOAT, a->size, LI_NONE, ret);
        case K_REC: return li_c_read(a->rec, ret);
        case K_VOID: return (Li_Value) { .type = LI_NONE };
        default: return value_at(a->kind, a->size, a->rec, ret); /* a pointer */
        }
}

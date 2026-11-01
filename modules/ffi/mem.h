/* Shared by ffi/read and ffi/write: what's at a pointer, as one of TYPES
 * (an options input), at an offset in bytes. "string" is the bytes up to a
 * '\0' (written with it). Nothing checks the pointer: C. */
#include "lineit.h"
#include <stdint.h>
#include <string.h>

#define TYPES "int|uint|char|uchar|short|ushort|long|ulong|int8|uint8|int16|uint16|int32|uint32|int64|uint64|float|double|string|ptr"

enum { T_INT,
       T_UINT,
       T_CHAR,
       T_UCHAR,
       T_SHORT,
       T_USHORT,
       T_LONG,
       T_ULONG,
       T_I8,
       T_U8,
       T_I16,
       T_U16,
       T_I32,
       T_U32,
       T_I64,
       T_U64,
       T_FLOAT,
       T_DOUBLE,
       T_STRING,
       T_PTR };

/* An int from any number, 0 if it isn't one */
static inline long long
mem_int(Li_Value v)
{
        return v.type == LI_INT ? v.i : v.type == LI_FLOAT ? (long long) v.f :
                                        v.type == LI_BOOL  ? v.b :
                                                             0;
}

/* Bytes TYPE takes, at least: a string, its '\0' */
static inline long
mem_size(int type)
{
        static const unsigned char sizes[] = {
                [T_INT]    = sizeof(int),
                [T_UINT]   = sizeof(unsigned),
                [T_CHAR]   = 1,
                [T_UCHAR]  = 1,
                [T_SHORT]  = sizeof(short),
                [T_USHORT] = sizeof(short),
                [T_LONG]   = sizeof(long),
                [T_ULONG]  = sizeof(long),
                [T_I8]     = 1,
                [T_U8]     = 1,
                [T_I16]    = 2,
                [T_U16]    = 2,
                [T_I32]    = 4,
                [T_U32]    = 4,
                [T_I64]    = 8,
                [T_U64]    = 8,
                [T_FLOAT]  = sizeof(float),
                [T_DOUBLE] = sizeof(double),
                [T_STRING] = 1,
                [T_PTR]    = sizeof(void *),
        };
        return type >= 0 && type <= T_PTR ? sizes[type] : 1;
}

/* The pointer PTR (a ptr, or bytes) plus OFFSET, NULL if it has none. In
 * bytes, NULL too if a TYPE there wouldn't fit: they know their size. */
static inline char *
mem_at(Li_Value ptr, Li_Value offset, int type)
{
        long off = mem_int(offset);
        if (ptr.type == LI_PTR) return ptr.p ? (char *) ptr.p + off : NULL;
        if (ptr.type != LI_BYTES || off < 0 || off + mem_size(type) > (long) li_bytes_size(ptr)) return NULL;
        return (char *) li_bytes_data(ptr) + off;
}

static inline Li_Value
mem_read(const char *p, int type)
{
        if (!p) return (Li_Value) { .type = LI_NONE };
        switch (type) {
        case T_INT: return li_int(*(const int *) p);
        case T_UINT: return li_int(*(const unsigned *) p);
        case T_CHAR: return li_int(*(const char *) p);
        case T_UCHAR: return li_int(*(const unsigned char *) p);
        case T_SHORT: return li_int(*(const short *) p);
        case T_USHORT: return li_int(*(const unsigned short *) p);
        case T_LONG: return li_int(*(const long *) p);
        case T_ULONG: return li_int((long) *(const unsigned long *) p);
        case T_I8: return li_int(*(const int8_t *) p);
        case T_U8: return li_int(*(const uint8_t *) p);
        case T_I16: return li_int(*(const int16_t *) p);
        case T_U16: return li_int(*(const uint16_t *) p);
        case T_I32: return li_int(*(const int32_t *) p);
        case T_U32: return li_int(*(const uint32_t *) p);
        case T_I64: return li_int((long) *(const int64_t *) p);
        case T_U64: return li_int((long) *(const uint64_t *) p);
        case T_FLOAT: return li_float(*(const float *) p);
        case T_DOUBLE: return li_float(*(const double *) p);
        case T_STRING: return li_string(p);
        case T_PTR: return li_ptr(*(void *const *) p);
        default: return (Li_Value) { .type = LI_NONE };
        }
}

static inline void
mem_write(char *p, int type, Li_Value v)
{
        long long i = mem_int(v);
        double d    = v.type == LI_FLOAT ? v.f : (double) i;
        if (!p) return;
        switch (type) {
        case T_INT: *(int *) p = (int) i; break;
        case T_UINT: *(unsigned *) p = (unsigned) i; break;
        case T_CHAR: *(char *) p = (char) i; break;
        case T_UCHAR: *(unsigned char *) p = (unsigned char) i; break;
        case T_SHORT: *(short *) p = (short) i; break;
        case T_USHORT: *(unsigned short *) p = (unsigned short) i; break;
        case T_LONG: *(long *) p = (long) i; break;
        case T_ULONG: *(unsigned long *) p = (unsigned long) i; break;
        case T_I8: *(int8_t *) p = (int8_t) i; break;
        case T_U8: *(uint8_t *) p = (uint8_t) i; break;
        case T_I16: *(int16_t *) p = (int16_t) i; break;
        case T_U16: *(uint16_t *) p = (uint16_t) i; break;
        case T_I32: *(int32_t *) p = (int32_t) i; break;
        case T_U32: *(uint32_t *) p = (uint32_t) i; break;
        case T_I64: *(int64_t *) p = (int64_t) i; break;
        case T_U64: *(uint64_t *) p = (uint64_t) i; break;
        case T_FLOAT: *(float *) p = (float) d; break;
        case T_DOUBLE: *(double *) p = d; break;
        case T_STRING:
                if (v.type == LI_STRING && v.s) memcpy(p, v.s, strlen(v.s) + 1);
                break;
        case T_PTR: *(void **) p = v.type == LI_PTR ? v.p : NULL; break;
        default: break;
        }
}

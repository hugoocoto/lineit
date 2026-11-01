/* Functions for tests/ffi_types.lua to call with ffi/call: what libc has
 * none of, records by value (types/point, types/segment), uint64_t and
 * float, out-parameters and a callback. Built to build/libffitest.so for
 * make test. */
#include <math.h>
#include <stdint.h>

typedef struct point {
        double x, y;
} point;

typedef struct segment {
        point from, to;
} segment;

point
mid(point a, point b)
{
        return (point) { (a.x + b.x) / 2, (a.y + b.y) / 2 };
}

double
length(segment s)
{
        return hypot(s.to.x - s.from.x, s.to.y - s.from.y);
}

void
scale(point *p, double k)
{
        p->x *= k;
        p->y *= k;
}

uint64_t
twice(uint64_t x)
{
        return 2 * x;
}

float
half(float x)
{
        return x / 2;
}

void
count_from(int out[3], int start)
{
        for (int i = 0; i < 3; i++)
                out[i] = start + i;
}

void
bytes_from(uint8_t out[4], int start)
{
        for (int i = 0; i < 4; i++)
                out[i] = (uint8_t) (start + i);
}

/* F(0) + F(1) + ... + F(N - 1) */
double
sum_of(double (*f)(double), int n)
{
        double s = 0;
        for (int i = 0; i < n; i++)
                s += f(i);
        return s;
}

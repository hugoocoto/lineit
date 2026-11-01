-- branches: what branches running at the same time share. Two branches
-- call sum_of (build/libffitest.so, from tests/lib/ffitest.c) at once,
-- with the same ffi/callback square, f(x) = fma(x, x, 0), so x * x: each
-- call has its own arguments, so each sum is its own (sum of x^2 for x
-- below 3000, and below 2000). They meet at the first print. The second entry starts a
-- loop through a fan-out (one -> two -> one, and three each time): forked
-- branches are as deep as the tick that forked them, so it stops at the
-- depth limit instead of forking forever. Checked by `make test` (see
-- tools/run_tests.c).
lineit_canvas = "0.0.1" -- the lineit that wrote it, see LI_VERSION

squares = {
        { id = 0, module = "flow/entry", x = 0, y = 0 },
        { id = 1, module = "ffi/callback", x = 0, y = 300, params = { { name = "fn", value = "double f(double x)" }, } },
        { id = 2, module = "ffi/call", x = 200, y = 300, params = { { name = "lib", value = "m" }, { name = "fn", value = "double fma(double x, double y, double z)" }, { name = "pure", value = "true" }, } },
        { id = 3, module = "ffi/call", x = 200, y = -100, params = { { name = "lib", value = "build/libffitest.so" }, { name = "fn", value = "double sum_of(void *f, int n)" }, { name = "pure", value = "false" }, }, inputs = { { name = "n", type = "int", value = "3000" }, } },
        { id = 4, module = "ffi/call", x = 200, y = 100, params = { { name = "lib", value = "build/libffitest.so" }, { name = "fn", value = "double sum_of(void *f, int n)" }, { name = "pure", value = "false" }, }, inputs = { { name = "n", type = "int", value = "2000" }, } },
        { id = 5, module = "ffi/call", x = 400, y = -250, params = { { name = "lib", value = "c" }, { name = "fn", value = "int snprintf(char buf[32], size_t n, const char *fmt, ..., double)" }, { name = "pure", value = "true" }, }, inputs = { { name = "n", type = "int", value = "32" }, { name = "fmt", type = "string", value = "%.0f" }, } },
        { id = 6, module = "ffi/call", x = 600, y = -250, params = { { name = "lib", value = "c" }, { name = "fn", value = "int snprintf(char buf[32], size_t n, const char *fmt, ..., double)" }, { name = "pure", value = "true" }, }, inputs = { { name = "n", type = "int", value = "32" }, { name = "fmt", type = "string", value = "%.0f" }, } },
        { id = 7, module = "io/print", x = 400, y = 0 },
        { id = 8, module = "io/print", x = 600, y = 0 },
        -- a loop through a fan-out
        { id = 9, module = "flow/entry", x = 0, y = 600 },
        { id = 10, module = "flow/counter", x = 200, y = 600 },
        { id = 11, module = "flow/counter", x = 400, y = 750 },
        { id = 12, module = "flow/counter", x = 400, y = 600 },
}

links = {
        { from = 1, from_pin = 1, to = 2, to_pin = 0 },
        { from = 1, from_pin = 1, to = 2, to_pin = 1 },
        { from = 2, from_pin = 0, to = 1, to_pin = 0 },
        { from = 0, from_pin = 0, to = 3, to_pin = 0 },
        { from = 0, from_pin = 0, to = 4, to_pin = 0 },
        { from = 1, from_pin = 0, to = 3, to_pin = 1 },
        { from = 1, from_pin = 0, to = 4, to_pin = 1 },
        { from = 3, from_pin = 0, to = 7, to_pin = 0 },
        { from = 4, from_pin = 0, to = 7, to_pin = 0 },
        { from = 3, from_pin = 1, to = 5, to_pin = 2 },
        { from = 5, from_pin = 1, to = 7, to_pin = 1 },
        { from = 7, from_pin = 0, to = 8, to_pin = 0 },
        { from = 4, from_pin = 1, to = 6, to_pin = 2 },
        { from = 6, from_pin = 1, to = 8, to_pin = 1 },
        { from = 9, from_pin = 0, to = 10, to_pin = 0 },
        { from = 10, from_pin = 0, to = 11, to_pin = 0 },
        { from = 10, from_pin = 0, to = 12, to_pin = 0 },
        { from = 11, from_pin = 0, to = 10, to_pin = 0 },
}

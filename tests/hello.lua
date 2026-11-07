-- An entry ticks a print.
return {
        lineit = 1,
        squares = {
                { id = 1, module = "flow/entry" },
                { id = 2, module = "io/print", fields = { value = "Hello, world!" } },
        },
        links = { { 1, "then", 2, "tick" } },
}

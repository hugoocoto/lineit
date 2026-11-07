-- A clock ticks a counter every 0.05 s; at 3 it prints and that's all it
-- does (the tests run it for 0.4 s).
return {
        lineit = 1,
        squares = {
                { id = 1, module = "flow/clock", fields = { interval = "0.05" } },
                { id = 2, module = "flow/counter" },
                { id = 3, module = "compare/equal", fields = { b = "3" } },
                { id = 4, module = "flow/ifelse" },
                { id = 5, module = "io/print", fields = { value = "three ticks" } },
        },
        links = {
                { 1, "tick", 2, "tick" }, { 2, "then", 4, "tick" },
                { 2, "count", 3, "a" }, { 3, "result", 4, "condition" }, { 4, "then", 5, "tick" },
        },
}

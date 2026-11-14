return {
        lineit = 1,
        doc = "A function that runs on ticks: adds x to a total of its own and prints the call, in two branches at once. Gives the total.",
        squares = {
                { id = 1, module = "graph/input", y = 0, params = { name = "tick", type = "tick" } },
                { id = 2, module = "graph/input", y = 50, params = { name = "x", type = "int" } },
                { id = 3, module = "var/var", params = { name = "total", type = "int", initial = "0" } },
                { id = 5, module = "math/add" },
                { id = 6, module = "var/set", params = { name = "total" } },
                { id = 9, module = "string/format", params = { format = "  called with {}" } },
                { id = 10, module = "io/print" },
                { id = 11, module = "graph/output", params = { name = "total", type = "int" } },
        },
        links = {
                { 1, "value", 6, "tick" }, { 1, "value", 10, "tick" },
                { 3, "value", 5, "a" }, { 2, "value", 5, "b" }, { 5, "result", 6, "value" },
                { 2, "value", 9, "1" }, { 9, "text", 10, "value" },
                { 3, "value", 11, "value" },
        },
}

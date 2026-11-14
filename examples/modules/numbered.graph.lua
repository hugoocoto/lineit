return {
        lineit = 1,
        doc = "A function that runs on ticks (it has a tick input): counts its calls in a variable of its own, prints each one and gives the count times x. Two branches run at once.",
        squares = {
                { id = 1, module = "graph/input", y = 0, params = { name = "tick", type = "tick" } },
                { id = 2, module = "graph/input", y = 50, params = { name = "x", type = "int" } },
                { id = 3, module = "var/var", params = { name = "calls", type = "int", initial = "0" } },
                { id = 4, module = "math/add", fields = { b = "1" } },
                { id = 5, module = "var/set", params = { name = "calls" } },
                { id = 6, module = "string/format", params = { format = "  call with {}" } },
                { id = 7, module = "io/print" },
                { id = 8, module = "math/multiply" },
                { id = 9, module = "graph/output", params = { name = "y", type = "int" } },
        },
        links = {
                { 1, "value", 5, "tick" }, { 1, "value", 7, "tick" },
                { 3, "value", 4, "a" }, { 4, "result", 5, "value" },
                { 2, "value", 6, "1" }, { 6, "text", 7, "value" },
                { 3, "value", 8, "a" }, { 2, "value", 8, "b" }, { 8, "result", 9, "value" },
        },
}

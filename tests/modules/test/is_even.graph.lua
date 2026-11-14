return {
        lineit = 1,
        doc = "Whether x is even",
        squares = {
                { id = 1, module = "graph/input", params = { name = "x", type = "int" } },
                { id = 2, module = "math/modulo", fields = { b = "2" } },
                { id = 3, module = "compare/equal", fields = { b = "0" } },
                { id = 4, module = "graph/output", params = { name = "even", type = "bool" } },
        },
        links = { { 1, "value", 2, "a" }, { 2, "result", 3, "a" }, { 3, "result", 4, "value" } },
}

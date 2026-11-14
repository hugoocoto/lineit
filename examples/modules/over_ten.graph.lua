return {
        lineit = 1,
        doc = "Whether x is more than 10: a function for list/filter",
        squares = {
                { id = 1, module = "graph/input", params = { name = "x", type = "int" } },
                { id = 2, module = "compare/greater", fields = { b = "10" } },
                { id = 3, module = "graph/output", params = { name = "big", type = "bool" } },
        },
        links = { { 1, "value", 2, "a" }, { 2, "result", 3, "value" } },
}

return {
        lineit = 1,
        doc = "x times 2: a function for list/map",
        squares = {
                { id = 1, module = "graph/input", params = { name = "x", type = "int" } },
                { id = 2, module = "math/multiply", fields = { b = "2" } },
                { id = 3, module = "graph/output", params = { name = "y", type = "int" } },
        },
        links = { { 1, "value", 2, "a" }, { 2, "result", 3, "value" } },
}

return {
        lineit = 1,
        doc = "x times x",
        squares = {
                { id = 1, module = "graph/input", y = 0, params = { name = "x", type = "int" } },
                { id = 2, module = "math/multiply" },
                { id = 3, module = "graph/output", params = { name = "y", type = "int" } },
        },
        links = { { 1, "value", 2, "a" }, { 1, "value", 2, "b" }, { 2, "result", 3, "value" } },
}

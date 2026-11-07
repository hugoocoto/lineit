return {
        lineit = 1,
        doc = "The average of two numbers: a subgraph, made of squares. Its graph/input and graph/output squares are its pins.",
        squares = {
                { id = 1, module = "graph/input", params = { name = "a", type = "float", default = "0" } },
                { id = 2, module = "graph/input", params = { name = "b", type = "float", default = "0" } },
                { id = 3, module = "math/add" },
                { id = 4, module = "math/divide", fields = { b = "2" } },
                { id = 5, module = "graph/output", params = { name = "average", type = "float" } },
        },
        links = { { 1, "value", 3, "a" }, { 2, "value", 3, "b" }, { 3, "result", 4, "a" }, { 4, "result", 5, "value" } },
}

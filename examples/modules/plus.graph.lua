return {
        lineit = 1,
        doc = "acc plus x: a function for list/reduce (the first input is the total so far, the output the new total)",
        squares = {
                { id = 1, module = "graph/input", y = 0, params = { name = "acc", type = "int" } },
                { id = 2, module = "graph/input", y = 50, params = { name = "x", type = "int" } },
                { id = 3, module = "math/add" },
                { id = 4, module = "graph/output", params = { name = "acc", type = "int" } },
        },
        links = { { 1, "value", 3, "a" }, { 2, "value", 3, "b" }, { 3, "result", 4, "value" } },
}

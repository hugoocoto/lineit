return {
        lineit = 1,
        doc = "x plus a variable of its own, which starts at 100",
        squares = {
                { id = 1, module = "graph/input", params = { name = "x", type = "int" } },
                { id = 2, module = "var/var", params = { name = "shift", type = "int", initial = "100" } },
                { id = 3, module = "math/add" },
                { id = 4, module = "graph/output", params = { name = "y", type = "int" } },
        },
        links = { { 1, "value", 3, "a" }, { 2, "value", 3, "b" }, { 3, "result", 4, "value" } },
}

-- Adds `value` to a total of its own, each tick.
return {
        lineit = 1,
        squares = {
                { id = 1, module = "graph/input", y = 0, params = { name = "tick", type = "tick" } },
                { id = 2, module = "graph/input", y = 50, params = { name = "value", type = "int", default = "1" } },
                { id = 3, module = "var/var", params = { name = "total", type = "int", initial = "0" } },
                { id = 4, module = "math/add" },
                { id = 5, module = "var/set", params = { name = "total" } },
                { id = 6, module = "graph/output", y = 0, params = { name = "then", type = "tick" } },
                { id = 7, module = "graph/output", y = 50, params = { name = "total", type = "int" } },
        },
        links = {
                { 1, "value", 5, "tick" }, { 3, "value", 4, "a" }, { 2, "value", 4, "b" }, { 4, "result", 5, "value" },
                { 5, "then", 6, "value" }, { 3, "value", 7, "value" },
        },
}

return {
        lineit = 1,
        doc = "acc, a dash and x",
        squares = {
                { id = 1, module = "graph/input", y = 0, params = { name = "acc", type = "str" } },
                { id = 2, module = "graph/input", y = 50, params = { name = "x", type = "str" } },
                { id = 3, module = "string/format", params = { format = "{}-{}" } },
                { id = 4, module = "graph/output", params = { name = "acc", type = "str" } },
        },
        links = { { 1, "value", 3, "1" }, { 2, "value", 3, "2" }, { 3, "text", 4, "value" } },
}

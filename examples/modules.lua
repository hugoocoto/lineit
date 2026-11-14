-- lineit canvas
return {
        lineit = 1,
        doc = "Modules of your own, from the modules folder next to this canvas: a subgraph (average), a Lua module (shout) and a record type (point). Ctrl+G makes a subgraph of the squares you select.",
        squares = {
                { id = 1, module = "flow/entry", x = 0, y = 0 },
                { id = 2, module = "flow/sequence", x = 280, y = 0, params = { count = "3" } },
                { id = 3, module = "average", x = 0, y = 106, fields = { a = "3", b = "4" } },
                { id = 4, module = "io/print", x = 560, y = 0 },
                { id = 5, module = "shout", x = 0, y = 234, fields = { text = "lineit" } },
                { id = 6, module = "io/print", x = 560, y = 128 },
                { id = 7, module = "record/make/point", x = 280, y = 172, fields = { y = "-1" } },
                { id = 8, module = "io/print", x = 560, y = 256 },
        },
        links = {
                { 1, "then", 2, "tick" },
                { 2, "1", 4, "tick" },
                { 3, "average", 4, "value" },
                { 2, "2", 6, "tick" },
                { 5, "result", 6, "value" },
                { 2, "3", 8, "tick" },
                { 3, "average", 7, "x" },
                { 7, "point", 8, "value" },
        },
}

-- lineit canvas
return {
        lineit = 1,
        doc = "The sum of the squares of 0 to 9. var/var declares `sum`, an int that starts at 0; the loop sets it to sum + index × index, and when it's done the print shows it.",
        camera = { x = 0, y = 0, zoom = 1 },
        squares = {
                { id = 1, module = "flow/entry", x = 0, y = 0 },
                { id = 2, module = "var/var", x = 0, y = 106, params = { initial = "0", name = "sum", type = "int" } },
                { id = 3, module = "flow/loop", x = 280, y = 0, fields = { from = "0", to = "10" } },
                { id = 4, module = "math/multiply", x = 560, y = 0 },
                { id = 5, module = "math/add", x = 840, y = 0 },
                { id = 6, module = "var/set", x = 1120, y = 0, params = { name = "sum" } },
                { id = 7, module = "io/print", x = 560, y = 128 },
        },
        links = {
                { 1, "then", 3, "tick" },
                { 3, "body", 6, "tick" },
                { 3, "index", 4, "a" },
                { 3, "index", 4, "b" },
                { 2, "value", 5, "a" },
                { 4, "result", 5, "b" },
                { 5, "result", 6, "value" },
                { 3, "done", 7, "tick" },
                { 2, "value", 7, "value" },
        },
}

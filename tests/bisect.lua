-- For `lineit bisect`: a loop that adds the index to `total` (21 at the end),
-- except that the 4th iteration adds 100 instead: the bug, in square 8.
return {
        lineit = 1,
        squares = {
                { id = 1, module = "flow/entry" },
                { id = 2, module = "var/var", params = { name = "total", type = "int", initial = "0" } },
                { id = 3, module = "flow/loop", fields = { from = "1", to = "7" } },
                { id = 4, module = "compare/equal", fields = { b = "4" } },
                { id = 5, module = "flow/ifelse" },
                { id = 6, module = "var/get", params = { name = "total" } },
                { id = 7, module = "math/add", fields = { b = "100" } },
                { id = 8, module = "var/set", params = { name = "total" } },
                { id = 9, module = "math/add" },
                { id = 10, module = "var/set", params = { name = "total" } },
                { id = 11, module = "io/print" },
        },
        links = {
                { 1, "then", 3, "tick" }, { 3, "body", 5, "tick" },
                { 3, "index", 4, "a" }, { 4, "result", 5, "condition" },
                { 5, "then", 8, "tick" }, { 5, "else", 10, "tick" },
                { 6, "value", 7, "a" }, { 7, "result", 8, "value" },
                { 6, "value", 9, "a" }, { 3, "index", 9, "b" }, { 9, "result", 10, "value" },
                { 3, "done", 11, "tick" }, { 6, "value", 11, "value" },
        },
}

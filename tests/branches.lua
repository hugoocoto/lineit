-- A fan-out: an entry ticks three pushes, which meet again at a print of
-- the length, once. The two sides of an ifelse meeting don't wait.
return {
        lineit = 1,
        squares = {
                { id = 1, module = "flow/entry" },
                { id = 2, module = "var/var", params = { name = "l", type = "list<str>" } },
                { id = 3, module = "list/push", params = { list = "l" }, fields = { value = "a" } },
                { id = 4, module = "list/push", params = { list = "l" }, fields = { value = "b" } },
                { id = 5, module = "list/push", params = { list = "l" }, fields = { value = "c" } },
                { id = 6, module = "list/length", params = { list = "l" } },
                { id = 7, module = "io/print" },
                { id = 8, module = "flow/ifelse", fields = { condition = "true" } },
                { id = 9, module = "io/print", fields = { value = "after" } },
        },
        links = {
                { 1, "then", 3, "tick" }, { 1, "then", 4, "tick" }, { 1, "then", 5, "tick" },
                { 3, "then", 7, "tick" }, { 4, "then", 7, "tick" }, { 5, "then", 7, "tick" },
                { 6, "length", 7, "value" },
                { 7, "then", 8, "tick" }, { 8, "then", 9, "tick" }, { 8, "else", 9, "tick" },
        },
}

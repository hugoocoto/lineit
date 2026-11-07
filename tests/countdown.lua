-- A loop made of wires: a set ticks an ifelse that ticks the set again,
-- 100000 times. Tail calls keep it from growing the stack.
return {
        lineit = 1,
        squares = {
                { id = 1, module = "flow/entry" },
                { id = 2, module = "var/var", params = { name = "n", type = "int", initial = "100000" } },
                { id = 3, module = "math/subtract", fields = { b = "1" } },
                { id = 4, module = "var/set", params = { name = "n" } },
                { id = 5, module = "compare/greater", fields = { b = "0" } },
                { id = 6, module = "flow/ifelse" },
                { id = 7, module = "io/print", fields = { value = "done" } },
                { id = 8, module = "var/get", params = { name = "n" } },
        },
        links = {
                { 1, "then", 4, "tick" },
                { 8, "value", 3, "a" }, { 3, "result", 4, "value" },
                { 4, "then", 6, "tick" }, { 8, "value", 5, "a" }, { 5, "result", 6, "condition" },
                { 6, "then", 4, "tick" }, { 6, "else", 7, "tick" },
        },
}

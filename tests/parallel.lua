-- Two branches change the same variables at once, 100000 times each with
-- var/set (get + 1), and 1000 times each in a code square (Shared.total):
-- no change is lost, compiled (on threads) as interpreted.
return {
        lineit = 1,
        squares = {
                { id = 1, module = "flow/entry", x = 0, y = 0 },
                { id = 2, module = "var/var", x = 0, y = 106, params = { name = "::n", type = "int", initial = "0" } },
                { id = 3, module = "var/var", x = 0, y = 278, params = { name = "::total", type = "int", initial = "0" } },
                { id = 4, module = "flow/loop", x = 280, y = 0, fields = { from = "0", to = "100000" } },
                { id = 5, module = "flow/loop", x = 280, y = 150, fields = { from = "0", to = "100000" } },
                { id = 6, module = "var/get", x = 0, y = 450, params = { name = "::n" } },
                { id = 7, module = "math/add", x = 280, y = 300, fields = { b = "1" } },
                { id = 8, module = "var/set", x = 560, y = 0, params = { name = "::n" } },
                { id = 9, module = "var/set", x = 560, y = 150, params = { name = "::n" } },
                { id = 10, module = "flow/loop", x = 560, y = 300, fields = { from = "0", to = "1000" } },
                { id = 11, module = "flow/loop", x = 560, y = 450, fields = { from = "0", to = "1000" } },
                { id = 12, module = "code/function", x = 840, y = 0, params = { code = "function add()\n    Shared.total = Shared.total + 1\nend\n" } },
                { id = 13, module = "code/function", x = 840, y = 226, params = { code = "function add()\n    Shared.total = Shared.total + 1\nend\n" } },
                { id = 14, module = "var/get", x = 0, y = 578, params = { name = "::total" } },
                { id = 15, module = "string/format", x = 280, y = 428, params = { format = "n = {}, total = {}" } },
                { id = 16, module = "io/print", x = 840, y = 452 },
        },
        links = {
                { 1, "then", 4, "tick" }, { 1, "then", 5, "tick" },
                { 6, "value", 7, "a" }, { 7, "result", 8, "value" }, { 7, "result", 9, "value" },
                { 4, "body", 8, "tick" }, { 5, "body", 9, "tick" },
                { 4, "done", 10, "tick" }, { 5, "done", 11, "tick" },
                { 10, "body", 12, "tick" }, { 11, "body", 13, "tick" },
                { 10, "done", 16, "tick" }, { 11, "done", 16, "tick" },
                { 6, "value", 15, "1" }, { 14, "value", 15, "2" }, { 15, "text", 16, "value" },
        },
}

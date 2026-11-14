-- Tests of a canvas. The program adds 1, 2... up to ::n into ::total and
-- prints it. Each test/case starts it with its own ::n (typed in its field),
-- and says in ::want what ::total should be; the test/expect checks it when
-- the loop is done.
-- A normal run starts at the entry (::n = 4), and test/expect passes by.
return {
        lineit = 1,
        squares = {
                { id = 1, module = "var/var", x = 0, y = 0, params = { name = "::n", type = "int", initial = "4" } },
                { id = 2, module = "var/var", x = 0, y = 172, params = { name = "::total", type = "int", initial = "0" } },
                { id = 3, module = "var/var", x = 0, y = 344, params = { name = "::want", type = "int", initial = "0" } },
                { id = 4, module = "flow/entry", x = 0, y = 516 },
                { id = 5, module = "test/case", x = 0, y = 622, params = { name = "up to 3" }, fields = { ["::n"] = "3", ["::want"] = "6" } },
                { id = 6, module = "test/case", x = 0, y = 772, params = { name = "up to 10" }, fields = { ["::n"] = "10", ["::want"] = "55" } },
                { id = 7, module = "flow/loop", x = 560, y = 0, fields = { from = "1" } },
                { id = 8, module = "var/get", x = 0, y = 922, params = { name = "::n" } },
                { id = 9, module = "math/add", x = 280, y = 0, fields = { b = "1" } },
                { id = 10, module = "var/get", x = 0, y = 1050, params = { name = "::total" } },
                { id = 11, module = "math/add", x = 840, y = 0 },
                { id = 12, module = "var/set", x = 1120, y = 0, params = { name = "::total" } },
                { id = 13, module = "test/expect", x = 840, y = 128, params = { label = "the total" } },
                { id = 14, module = "var/get", x = 0, y = 1178, params = { name = "::want" } },
                { id = 15, module = "io/print", x = 1120, y = 150 },
        },
        links = {
                { 4, "then", 7, "tick" }, { 5, "then", 7, "tick" }, { 6, "then", 7, "tick" },
                { 8, "value", 9, "a" }, { 9, "result", 7, "to" },
                { 7, "body", 12, "tick" }, { 10, "value", 11, "a" }, { 7, "index", 11, "b" }, { 11, "result", 12, "value" },
                { 7, "done", 13, "tick" }, { 10, "value", 13, "value" }, { 14, "value", 13, "expected" },
                { 13, "then", 15, "tick" }, { 10, "value", 15, "value" },
        },
}

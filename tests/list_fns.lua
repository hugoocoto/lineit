-- list/map, list/filter and list/reduce call a subgraph (the `function`
-- param) for each item: pure ones, ones that run on ticks with branches and
-- variables of their own, and a chain of the three.
return {
        lineit = 1,
        squares = {
                { id = 1, module = "flow/entry" },
                { id = 2, module = "var/var", params = { name = "nums", type = "list<int>", initial = "" } },
                { id = 3, module = "flow/loop", fields = { from = "1", to = "7" } },
                { id = 4, module = "list/push", params = { list = "nums" } },
                { id = 5, module = "var/get", params = { name = "nums" } },
                { id = 6, module = "flow/sequence", params = { count = "9" } },
                -- map
                { id = 10, module = "list/map", params = { fn = "test/square" } },
                { id = 11, module = "io/print" },
                -- filter
                { id = 12, module = "list/filter", params = { fn = "test/is_even" } },
                { id = 13, module = "io/print" },
                -- reduce, on numbers and on text
                { id = 14, module = "list/reduce", params = { fn = "test/add_up" }, fields = { initial = "0" } },
                { id = 15, module = "io/print" },
                { id = 16, module = "string/split", fields = { text = "a b c", separator = " " } },
                { id = 17, module = "list/reduce", params = { fn = "test/dashed" }, fields = { initial = "start" } },
                { id = 18, module = "io/print" },
                -- the three one after the other
                { id = 19, module = "list/map", params = { fn = "test/square" } },
                { id = 20, module = "list/filter", params = { fn = "test/is_even" } },
                { id = 21, module = "list/reduce", params = { fn = "test/add_up" } },
                { id = 22, module = "io/print" },
                -- a function with a variable of its own
                { id = 23, module = "list/map", params = { fn = "test/shifted" } },
                { id = 24, module = "io/print" },
                -- functions that run on ticks: variables per square, branches
                { id = 25, module = "list/map", params = { fn = "test/running" } },
                { id = 26, module = "io/print" },
                { id = 27, module = "list/filter", params = { fn = "test/is_even" } },
                { id = 28, module = "list/map", params = { fn = "test/running" } },
                { id = 29, module = "io/print" },
                -- a function that has a list/reduce inside
                { id = 30, module = "list/map", params = { fn = "test/sum_to" } },
                { id = 31, module = "io/print" },
        },
        links = {
                { 1, "then", 3, "tick" }, { 3, "body", 4, "tick" }, { 3, "index", 4, "value" }, { 3, "done", 6, "tick" },
                { 6, "1", 10, "tick" }, { 5, "value", 10, "list" }, { 10, "result", 11, "value" }, { 10, "done", 11, "tick" },
                { 6, "2", 12, "tick" }, { 5, "value", 12, "list" }, { 12, "result", 13, "value" }, { 12, "done", 13, "tick" },
                { 6, "3", 14, "tick" }, { 5, "value", 14, "list" }, { 14, "result", 15, "value" }, { 14, "done", 15, "tick" },
                { 6, "4", 17, "tick" }, { 16, "result", 17, "list" }, { 17, "result", 18, "value" }, { 17, "done", 18, "tick" },
                { 6, "5", 19, "tick" }, { 5, "value", 19, "list" },
                { 19, "done", 20, "tick" }, { 19, "result", 20, "list" },
                { 20, "done", 21, "tick" }, { 20, "result", 21, "list" },
                { 21, "done", 22, "tick" }, { 21, "result", 22, "value" },
                { 6, "6", 23, "tick" }, { 5, "value", 23, "list" }, { 23, "result", 24, "value" }, { 23, "done", 24, "tick" },
                { 6, "7", 25, "tick" }, { 5, "value", 25, "list" }, { 25, "result", 26, "value" }, { 25, "done", 26, "tick" },
                { 6, "8", 27, "tick" }, { 5, "value", 27, "list" },
                { 27, "done", 28, "tick" }, { 27, "result", 28, "list" }, { 28, "result", 29, "value" }, { 28, "done", 29, "tick" },
                { 6, "9", 30, "tick" }, { 5, "value", 30, "list" }, { 30, "result", 31, "value" }, { 30, "done", 31, "tick" },
        },
}

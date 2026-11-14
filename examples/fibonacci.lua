-- lineit canvas
return {
        lineit = 1,
        doc = "The first 30 Fibonacci numbers, in a list. The entry pushes 0 and 1, then a loop pushes the sum of the last two (list/get counts from the end with negative indexes) and prints the new last one. Hover a pin while it runs to see what it carries.",
        squares = {
                { id = 1, module = "flow/entry", x = 0, y = 0 },
                { id = 2, module = "var/var", x = 0, y = 106, params = { name = "fib", type = "list<int>" } },
                { id = 3, module = "list/push", x = 280, y = 0, params = { list = "fib" }, fields = { value = "0" } },
                { id = 4, module = "list/push", x = 560, y = 0, params = { list = "fib" }, fields = { value = "1" } },
                { id = 5, module = "flow/loop", x = 840, y = 0, fields = { from = "2", to = "30" } },
                { id = 6, module = "list/get", x = 0, y = 278, params = { list = "fib" }, fields = { index = "-1" } },
                { id = 7, module = "list/get", x = 0, y = 406, params = { list = "fib" }, fields = { index = "-2" } },
                { id = 8, module = "math/add", x = 280, y = 150 },
                { id = 9, module = "list/push", x = 1120, y = 0, params = { list = "fib" } },
                { id = 10, module = "list/get", x = 0, y = 534, params = { list = "fib" }, fields = { index = "-1" } },
                { id = 11, module = "io/print", x = 1400, y = 0 },
        },
        links = {
                { 1, "then", 3, "tick" },
                { 3, "then", 4, "tick" },
                { 4, "then", 5, "tick" },
                { 5, "body", 9, "tick" },
                { 6, "value", 8, "a" },
                { 7, "value", 8, "b" },
                { 8, "result", 9, "value" },
                { 9, "then", 11, "tick" },
                { 10, "value", 11, "value" },
        },
}

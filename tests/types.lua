-- Inference: 2 + 2.5 is a float (the int widens), 7 / 2 an int, and a
-- convert, a format, a concat of anything, and comparing strings.
return {
        lineit = 1,
        squares = {
                { id = 1, module = "flow/entry" },
                { id = 2, module = "flow/sequence", params = { count = "6" } },
                { id = 3, module = "math/add", fields = { a = "2", b = "2.5" } },
                { id = 4, module = "io/print" },
                { id = 5, module = "math/divide", fields = { a = "7", b = "2" } },
                { id = 6, module = "io/print" },
                { id = 7, module = "value/convert", params = { to = "int" }, fields = { value = " 42 " } },
                { id = 8, module = "math/multiply", fields = { b = "2" } },
                { id = 9, module = "io/print" },
                { id = 10, module = "string/format", params = { format = "{} is {}, {{ok}}" } },
                { id = 11, module = "value/constant", params = { type = "bool", value = "true" } },
                { id = 12, module = "io/print" },
                { id = 13, module = "string/concat", fields = { a = "x = " } },
                { id = 14, module = "io/print" },
                { id = 15, module = "compare/less", fields = { a = "apple", b = "banana" } },
                { id = 16, module = "io/print" },
                { id = 17, module = "math/divide", fields = { a = "1.0", b = "0" } },
                { id = 18, module = "io/print" },
        },
        links = {
                { 1, "then", 2, "tick" },
                { 2, "1", 4, "tick" }, { 3, "result", 4, "value" },
                { 2, "2", 6, "tick" }, { 5, "result", 6, "value" },
                { 2, "3", 9, "tick" }, { 7, "value", 8, "a" }, { 8, "result", 9, "value" },
                { 2, "4", 12, "tick" }, { 3, "result", 10, "1" }, { 11, "value", 10, "2" }, { 10, "text", 12, "value" },
                { 2, "5", 14, "tick" }, { 5, "result", 13, "b" }, { 13, "result", 14, "value" },
                { 2, "6", 16, "tick" }, { 15, "result", 16, "value" },
        },
}

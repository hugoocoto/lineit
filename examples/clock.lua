-- lineit canvas
return {
        lineit = 1,
        doc = "A clock ticks every half second while the program runs; a counter counts the ticks, and at 10 flow/stop ends the program. Without a stop, a program with a clock runs until you press Stop.",
        camera = { x = 0, y = 0, zoom = 1 },
        squares = {
                { id = 1, module = "flow/clock", x = 0, y = 0, fields = { interval = "0.5" } },
                { id = 2, module = "flow/counter", x = 280, y = 0 },
                { id = 3, module = "string/format", x = 560, y = 0, params = { format = "tick {}" } },
                { id = 4, module = "io/print", x = 840, y = 0 },
                { id = 5, module = "compare/greater_equal", x = 560, y = 128, fields = { b = "10" } },
                { id = 6, module = "flow/ifelse", x = 1120, y = 0 },
                { id = 7, module = "flow/stop", x = 1400, y = 0 },
        },
        links = {
                { 1, "tick", 2, "tick" },
                { 2, "then", 4, "tick" },
                { 2, "count", 3, "1" },
                { 3, "text", 4, "value" },
                { 4, "then", 6, "tick" },
                { 2, "count", 5, "a" },
                { 5, "result", 6, "condition" },
                { 6, "then", 7, "tick" },
        },
}

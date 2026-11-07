-- lineit canvas
return {
        lineit = 1,
        doc = "The smallest program: when it starts, the entry ticks a print. Press Run (F5).",
        camera = { x = 0, y = 0, zoom = 1 },
        squares = {
                { id = 1, module = "flow/entry", x = 0, y = 0 },
                { id = 2, module = "io/print", x = 280, y = 0, fields = { value = "Hello, world!" } },
        },
        links = {
                { 1, "then", 2, "tick" },
        },
}

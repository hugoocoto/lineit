return {
        doc = "The text, backwards",
        inputs = { { "text", "str", default = "abc" } },
        outputs = { { "result", "str" } },
        eval = function(i) return { result = i.text:reverse() } end,
}

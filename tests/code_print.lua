-- What code squares print goes to the program's output, in order with
-- its own lines: one square of each language prints, then an io/print.
return {
        lineit = 1,
        squares = {
                { id = 1, module = "flow/entry", x = 0, y = 0 },
                { id = 2, module = "flow/sequence", x = 280, y = 0, params = { count = "4" } },
                { id = 3, module = "code/function", x = 560, y = 0, params = { code = "function hi()\n    print(\"lua says\", 1)\nend\n" } },
                { id = 4, module = "code/function", x = 560, y = 226, params = { language = "python", code = "def hi():\n    print(\"python says\", 2)\n" } },
                { id = 5, module = "code/function", x = 560, y = 438, params = { language = "rust", code = "pub fn hi() {\n    println!(\"rust says {}\", 3);\n}\n" } },
                { id = 6, module = "code/function", x = 560, y = 664, params = { language = "c", code = "void hi(void) {\n    printf(\"c says %d\\n\", 4);\n    puts(\"and\");\n    printf(\"no end\");\n}\n" } },
                { id = 7, module = "io/print", x = 898, y = 0, fields = { value = "after lua" } },
                { id = 8, module = "io/print", x = 898, y = 128, fields = { value = "after python" } },
                { id = 9, module = "io/print", x = 898, y = 256, fields = { value = "after rust" } },
                { id = 10, module = "io/print", x = 898, y = 384, fields = { value = "after c" } },
        },
        links = {
                { 1, "then", 2, "tick" },
                { 2, "1", 3, "tick" }, { 3, "then", 7, "tick" },
                { 2, "2", 4, "tick" }, { 4, "then", 8, "tick" },
                { 2, "3", 5, "tick" }, { 5, "then", 9, "tick" },
                { 2, "4", 6, "tick" }, { 6, "then", 10, "tick" },
        },
}

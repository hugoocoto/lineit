-- A Python square that writes to stderr while it runs fine, between two prints.
return { lineit = 1, squares = {
  { id = 1, module = "flow/entry" },
  { id = 2, module = "io/print", fields = { value = "before" } },
  { id = 3, module = "code/function", params = { language = "python", code = "import sys\n\ndef warn():\n    print(\"to stderr\", file=sys.stderr, flush=True)\n" } },
  { id = 4, module = "io/print", fields = { value = "after" } },
}, links = { { 1, "then", 2, "tick" }, { 2, "then", 3, "tick" }, { 3, "then", 4, "tick" } } }

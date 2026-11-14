-- A C square that writes to stdout itself, then one that crashes: lineit
-- says so and ends as it would for any error, with what was printed.
return { lineit = 1, squares = {
  { id = 1, module = "flow/entry" },
  { id = 2, module = "code/function", params = { language = "c", code = "#include <stdio.h>\nvoid write_raw(void) {\n    fprintf(stdout, \"raw to stdout\\n\");\n    fflush(stdout);\n}\n" } },
  { id = 3, module = "io/print", fields = { value = "still talking" } },
  { id = 4, module = "code/function", params = { language = "c", code = "void boom(void) {\n    int *p = 0;\n    *p = 1;\n}\n" } },
  { id = 5, module = "io/print", fields = { value = "not reached" } },
}, links = { { 1, "then", 2, "tick" }, { 2, "then", 3, "tick" }, { 3, "then", 4, "tick" }, { 4, "then", 5, "tick" } } }

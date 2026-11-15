-- A C# square with a typo (`y` isn't there): dotnet says "The build failed" on
-- its stderr, and lineit's own error has the compiler's message.
return { lineit = 1, squares = {
  { id = 1, module = "flow/entry" },
  { id = 2, module = "code/function", params = { language = "csharp", code = "static class A\n{\n    public static long F(long x)\n    {\n        return y;\n    }\n}\n" }, fields = { x = "3" } },
  { id = 3, module = "io/print" },
}, links = { { 1, "then", 3, "tick" }, { 2, "result", 3, "value" } } }

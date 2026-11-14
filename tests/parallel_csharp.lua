-- Two branches call C# squares with the same code at once, 300 times each:
-- each square keeps its own count, and no change to ::total is lost.
return {
        lineit = 1,
        squares = {
                { id = 1, module = "flow/entry", x = 0, y = 0 },
                { id = 2, module = "var/var", x = 0, y = 106, params = { name = "::total", type = "int", initial = "0" } },
                { id = 3, module = "flow/loop", x = 280, y = 0, fields = { from = "0", to = "300" } },
                { id = 4, module = "flow/loop", x = 280, y = 150, fields = { from = "0", to = "300" } },
                { id = 5, module = "code/function", x = 560, y = 0, params = { language = "csharp", code = [[
static class Adder
{
    static long mine = 0;

    public static long Add()
    {
        mine++;
        Shared.total += 1;
        return mine;
    }
}
]] } },
                { id = 6, module = "code/function", x = 560, y = 150, params = { language = "csharp", code = [[
static class Adder
{
    static long mine = 0;

    public static long Add()
    {
        mine++;
        Shared.total += 1;
        return mine;
    }
}
]] } },
                { id = 7, module = "var/get", x = 0, y = 278, params = { name = "::total" } },
                { id = 8, module = "string/format", x = 840, y = 300, params = { format = "total = {}, counts {} {}" } },
                { id = 9, module = "io/print", x = 1120, y = 300 },
        },
        links = {
                { 1, "then", 3, "tick" }, { 1, "then", 4, "tick" },
                { 3, "body", 5, "tick" }, { 4, "body", 6, "tick" },
                { 3, "done", 9, "tick" }, { 4, "done", 9, "tick" },
                { 7, "value", 8, "1" }, { 5, "result", 8, "2" }, { 6, "result", 8, "3" }, { 8, "text", 9, "value" },
        },
}

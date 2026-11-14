-- A Rust code square takes and gives records (tests/modules/point.type.lua):
-- the glue makes their struct, `point`.
return {
        lineit = 1,
        squares = {
                { id = 1, module = "flow/entry", x = 0, y = 0 },
                { id = 2, module = "record/make/point", x = 0, y = 106, fields = { x = "1.5", y = "2" } },
                { id = 3, module = "code/function", x = 280, y = 0, params = { language = "rust", code = [[
pub struct Moved {
    p: point,
    count: i64,
}

/// P moved DX to the right, and how many points there were.
pub fn moved(p: point, dx: f64, all: Vec<point>) -> Moved {
    Moved { p: point { x: p.x + dx, y: p.y }, count: all.len() as i64 }
}
]] }, fields = { dx = "10" } },
                { id = 4, module = "record/split/point", x = 902, y = 0 },
                { id = 5, module = "string/format", x = 1182, y = 0, params = { format = "x = {}, y = {}, count = {}" } },
                { id = 6, module = "io/print", x = 1462, y = 0 },
                { id = 7, module = "var/var", x = 0, y = 234, params = { name = "pts", type = "list<point>" } },
                { id = 8, module = "list/push", x = 902, y = 128, params = { list = "pts" } },
                { id = 9, module = "list/push", x = 902, y = 278, params = { list = "pts" } },
                { id = 10, module = "var/get", x = 0, y = 406, params = { name = "pts" } },
                { id = 11, module = "flow/sequence", x = 280, y = 354, params = { count = "3" } },
        },
        links = {
                { 1, "then", 11, "tick" },
                { 11, "1", 8, "tick" }, { 2, "point", 8, "value" },
                { 11, "2", 9, "tick" }, { 2, "point", 9, "value" },
                { 11, "3", 6, "tick" },
                { 2, "point", 3, "p" }, { 10, "value", 3, "all" },
                { 3, "p", 4, "point" }, { 4, "x", 5, "1" }, { 4, "y", 5, "2" }, { 3, "count", 5, "3" },
                { 5, "text", 6, "value" },
        },
}

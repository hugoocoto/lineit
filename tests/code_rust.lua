-- Code squares, in Rust, of each shape (tests/code.lua in Lua). A pure one squares 7. A counter
-- keeps its count (a global) and fires `then`. A loop from 1 to 5 ticks
-- one with two exits, `even` and `odd`, which adds each number to
-- ::total; and one fires its exit once per word.
return {
        lineit = 1,
        squares = {
                { id = 1, module = "flow/entry", x = 0, y = 0 },
                { id = 2, module = "code/function", x = 0, y = 106, params = { language = "rust", code = [[
pub struct Squared {
    square: i64,
    text: String,
}

/// The square of x, and what it did.
pub fn square(x: i64) -> Squared {
    Squared { square: x * x, text: "squared".into() }
}
]] }, fields = { x = "7" } },
                { id = 3, module = "string/format", x = 491, y = 0, params = { format = "{} {}" } },
                { id = 4, module = "io/print", x = 771, y = 0 },
                { id = 5, module = "var/var", x = 0, y = 438, params = { name = "::total", type = "int", initial = "0" } },
                { id = 6, module = "flow/loop", x = 771, y = 128, fields = { from = "1", to = "6" } },
                { id = 7, module = "code/function", x = 1276, y = 0, params = { language = "rust", code = [[
#[derive(Default)]
pub struct Collatz {
    count: i64,
}

pub struct Odd {
    calls: i64,
    n: i64,
}

impl Collatz {
    /// Halves even numbers; counts and passes on the odd ones.
    pub fn collatz(&mut self, n: i64, shared: &mut Shared, mut even: impl FnMut(i64), mut odd: impl FnMut(Odd)) {
        shared.total += n;
        if n % 2 == 0 {
            return even(n / 2);
        }
        self.count += 1;
        odd(Odd { calls: self.count, n })
    }
}
]], tests = [[{
  { given = { n = "4", ["::total"] = "10" }, expect = { fires = "even", ["even.value"] = "2", ["::total"] = "14", count = "0" } },
  { given = { n = "3" }, expect = { fires = "odd", calls = "1", n = "3", count = "1" } },
  { given = { n = "5" }, expect = { fires = "odd", calls = "2", count = "2" } },
  { given = { n = "7", count = "41" }, expect = { fires = "odd", calls = "42" } },
}]] } },
                { id = 8, module = "string/format", x = 1996, y = 0, params = { format = "even, half is {}" } },
                { id = 9, module = "io/print", x = 2276, y = 0 },
                { id = 10, module = "string/format", x = 1996, y = 128, params = { format = "odd call {}: {}" } },
                { id = 11, module = "io/print", x = 2276, y = 128 },
                { id = 12, module = "var/get", x = 0, y = 610, params = { name = "::total" } },
                { id = 13, module = "io/print", x = 1276, y = 566 },
                { id = 14, module = "code/function", x = 1276, y = 694, params = { language = "rust", code = [[
#[derive(Default)]
pub struct Counter {
    ticks: i64,
}

impl Counter {
    pub fn counter(&mut self) -> i64 {
        self.ticks += 1;
        self.ticks
    }
}
]], tests = [[{
  { expect = { fires = "then", result = "1", ticks = "1" } },
  { expect = { result = "2" } },
  { given = { ticks = "9" }, expect = { result = "10", ticks = "10" } },
}]] } },
                { id = 15, module = "string/format", x = 1996, y = 278, params = { format = "tick {}" } },
                { id = 16, module = "io/print", x = 2276, y = 256 },
                { id = 17, module = "code/function", x = 771, y = 278, params = { language = "rust", code = [[
pub struct Word {
    w: String,
    at: i64,
}

pub fn words(text: &str, word: &mut dyn FnMut(Word)) {
    for (i, w) in text.split_whitespace().enumerate() {
        word(Word { w: w.into(), at: i as i64 + 1 });
    }
}
]] }, fields = { text = "one two three" } },
                { id = 18, module = "string/format", x = 1276, y = 1054, params = { format = "word {}: {}" } },
                { id = 19, module = "io/print", x = 1996, y = 406 },
                { id = 20, module = "flow/sequence", x = 491, y = 150, params = { count = "4" } },
                { id = 21, module = "flow/loop", x = 771, y = 646, fields = { from = "0", to = "3" } },
        },
        links = {
                { 1, "then", 20, "tick" },
                { 20, "1", 4, "tick" }, { 2, "square", 3, "1" }, { 2, "text", 3, "2" }, { 3, "text", 4, "value" },
                { 20, "2", 6, "tick" },
                { 6, "body", 7, "tick" }, { 6, "index", 7, "n" },
                { 7, "even", 9, "tick" }, { 7, "even.value", 8, "1" }, { 8, "text", 9, "value" },
                { 7, "odd", 11, "tick" }, { 7, "calls", 10, "1" }, { 7, "n", 10, "2" }, { 10, "text", 11, "value" },
                { 6, "done", 13, "tick" }, { 12, "value", 13, "value" },
                { 20, "3", 21, "tick" }, { 21, "body", 14, "tick" }, { 14, "then", 16, "tick" }, { 14, "result", 15, "1" }, { 15, "text", 16, "value" },
                { 20, "4", 17, "tick" }, { 17, "word", 19, "tick" }, { 17, "w", 18, "2" }, { 17, "at", 18, "1" }, { 18, "text", 19, "value" },
        },
}

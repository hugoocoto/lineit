-- Code squares in JavaScript, a language added with a file
-- (languages/javascript.lua, next to this canvas), of each shape; as
-- tests/code.lua in Lua. A pure one squares 7. A counter
-- keeps its count (a global) and fires `then`. A loop from 1 to 5 ticks
-- one with two exits, `even` and `odd`, which adds each number to
-- ::total; and one fires its exit once per word.
return {
        lineit = 1,
        squares = {
                { id = 1, module = "flow/entry", x = 0, y = 0 },
                { id = 2, module = "code/function", x = 0, y = 106, params = { language = "javascript", code = [==[
/**
 * The square of x, and what it did.
 * @param {int} x
 * @returns {{square: int, text: string}}
 */
export function square(x) {
    return { square: x * x, text: "squared" };
}
]==] }, fields = { x = "7" } },
                { id = 3, module = "string/format", x = 440, y = 0, params = { format = "{} {}" } },
                { id = 4, module = "io/print", x = 720, y = 0 },
                { id = 5, module = "var/var", x = 0, y = 424, params = { name = "::total", type = "int", initial = "0" } },
                { id = 6, module = "flow/loop", x = 720, y = 128, fields = { from = "1", to = "6" } },
                { id = 7, module = "code/function", x = 1357, y = 0, params = { language = "javascript", code = [==[
var count = 0;

/**
 * Halves even numbers; counts and passes on the odd ones.
 * @param {int} n
 * @param {(half: int) => void} even
 * @param {(calls: int, n: int) => void} odd
 */
export function collatz(n, even, odd) {
    Shared.total += n;
    if (n % 2 === 0) return even(n / 2);
    count += 1;
    return odd(count, n);
}
]==], tests = [[{
  { given = { n = "4", ["::total"] = "10" }, expect = { fires = "even", half = "2", ["::total"] = "14", count = "0" } },
  { given = { n = "3" }, expect = { fires = "odd", calls = "1", n = "3", count = "1" } },
  { given = { n = "5" }, expect = { fires = "odd", calls = "2", count = "2" } },
  { given = { n = "7", count = "41" }, expect = { fires = "odd", calls = "42" } },
}]] } },
                { id = 8, module = "string/format", x = 1884, y = 0, params = { format = "even, half is {}" } },
                { id = 9, module = "io/print", x = 2164, y = 0 },
                { id = 10, module = "string/format", x = 1884, y = 128, params = { format = "odd call {}: {}" } },
                { id = 11, module = "io/print", x = 2164, y = 128 },
                { id = 12, module = "var/get", x = 0, y = 596, params = { name = "::total" } },
                { id = 13, module = "io/print", x = 1357, y = 468 },
                { id = 14, module = "code/function", x = 1357, y = 596, params = { language = "javascript", code = [==[
var ticks = 0;

/** @returns {int} result */
export function counter() {
    ticks += 1;
    return ticks;
}
]==], tests = [[{
  { expect = { fires = "then", result = "1", ticks = "1" } },
  { expect = { result = "2" } },
  { given = { ticks = "9" }, expect = { result = "10", ticks = "10" } },
}]] } },
                { id = 15, module = "string/format", x = 1884, y = 278, params = { format = "tick {}" } },
                { id = 16, module = "io/print", x = 2164, y = 256 },
                { id = 17, module = "code/function", x = 720, y = 278, params = { language = "javascript", code = [==[
/**
 * @param {string} text
 * @param {(w: string, at: int) => void} word
 */
export function words(text, word) {
    text.split(/\s+/).filter((w) => w).forEach((w, i) => word(w, i + 1));
}
]==] }, fields = { text = "one two three" } },
                { id = 18, module = "string/format", x = 1357, y = 900, params = { format = "word {}: {}" } },
                { id = 19, module = "io/print", x = 1884, y = 406 },
                { id = 20, module = "flow/sequence", x = 440, y = 150, params = { count = "4" } },
                { id = 21, module = "flow/loop", x = 720, y = 604, fields = { from = "0", to = "3" } },
        },
        links = {
                { 1, "then", 20, "tick" },
                { 20, "1", 4, "tick" }, { 2, "square", 3, "1" }, { 2, "text", 3, "2" }, { 3, "text", 4, "value" },
                { 20, "2", 6, "tick" },
                { 6, "body", 7, "tick" }, { 6, "index", 7, "n" },
                { 7, "even", 9, "tick" }, { 7, "half", 8, "1" }, { 8, "text", 9, "value" },
                { 7, "odd", 11, "tick" }, { 7, "calls", 10, "1" }, { 7, "n", 10, "2" }, { 10, "text", 11, "value" },
                { 6, "done", 13, "tick" }, { 12, "value", 13, "value" },
                { 20, "3", 21, "tick" }, { 21, "body", 14, "tick" }, { 14, "then", 16, "tick" }, { 14, "result", 15, "1" }, { 15, "text", 16, "value" },
                { 20, "4", 17, "tick" }, { 17, "word", 19, "tick" }, { 17, "w", 18, "2" }, { 17, "at", 18, "1" }, { 18, "text", 19, "value" },
        },
}

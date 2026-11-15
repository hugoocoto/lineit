//! A C#, Python or Lua square that keeps state lineit can't see is refused,
//! with the line on the square: it'd read as stateless, a pure function with
//! no tick that runs only if something reads it (it was a square that did
//! nothing, with no diagnostics). What isn't refused is common correct code
//! (a cache in a `readonly` static, in a `_`name or in capitals).

use lineit_core::lower::lower;
use lineit_core::registry::Registry;

fn lowered(language: &str, code: &str, ticked: bool) -> lineit_core::lower::Lowered {
    // a pure function has no tick nor then: the clock ticks the print
    let ticks =
        if ticked { r#"{ 1, "tick", 2, "tick" }, { 2, "then", 4, "tick" }"# } else { r#"{ 1, "tick", 4, "tick" }"# };
    let src = format!(
        r#"return {{ lineit = 1, squares = {{
        {{ id = 1, module = "flow/clock", fields = {{ interval = "100" }} }},
        {{ id = 2, module = "code/function", params = {{ language = "{language}", code = {} }} }},
        {{ id = 3, module = "string/format", params = {{ format = "{{}}" }} }},
        {{ id = 4, module = "io/print" }},
        }}, links = {{ {{ 2, "result", 3, "1" }}, {{ 3, "text", 4, "value" }}, {ticks} }} }}"#,
        lineit_core::file::quote(code)
    );
    lower(&lineit_core::file::read_canvas(&src, "hidden").unwrap(), &Registry::builtin())
}

/// The code is refused with a diagnostic that says SAYS.
fn refused(language: &str, code: &str, says: &str) {
    let l = lowered(language, code, true);
    assert!(l.program.is_none(), "{code}");
    assert!(format!("{:?}", l.diags).contains(says), "{code}: {:?}", l.diags);
}

/// The code lowers, and its square is a ticked one (TICKED) or a pure
/// function.
fn accepted(language: &str, code: &str, ticked: bool) {
    let l = lowered(language, code, ticked);
    assert!(l.diags.is_empty(), "{code}: {:?}", l.diags);
    assert!(l.program.is_some(), "{code}");
    // a pure function has no tick to wire
    if !ticked {
        let l = lowered(language, code, true);
        assert!(format!("{:?}", l.diags).contains("has no pin `tick`"), "{code}: {:?}", l.diags);
    }
}

#[test]
fn python_attributes_of_the_function_are_refused() {
    refused(
        "python",
        "def f() -> int:\n    f.count += 1\n    return f.count\nf.count = 0\n",
        "line 2: `f.count` is set on `f`",
    );
    refused("python", "class C:\n    n = 0\ndef f() -> int:\n    C.n += 1\n    return C.n\n", "`C.n` is set on `C`");
    // its own object's are its own
    accepted("python", "def f(o) -> int:\n    o.n = 1\n    return 1\n", false);
}

#[test]
fn python_a_default_the_function_changes_is_refused() {
    let goes = "is a default that the function changes";
    refused("python", "def f(acc=[]) -> int:\n    acc.append(1)\n    return len(acc)\n", goes);
    refused("python", "def f(m: dict[str, int] = {}) -> int:\n    m['a'] = 1\n    return 1\n", goes);
    refused("python", "def f(acc=list()) -> int:\n    acc += [1]\n    return len(acc)\n", goes);
    // one it only reads, or that isn't mutable
    accepted("python", "def f(xs=[]) -> int:\n    return len(xs)\n", false);
    accepted("python", "def f(n=0) -> int:\n    n += 1\n    return n\n", false);
}

#[test]
fn python_a_global_of_a_helper_is_state() {
    accepted(
        "python",
        "_n = 0\ndef _bump():\n    global _n\n    _n += 1\ndef f() -> int:\n    _bump()\n    return _n\n",
        true,
    );
    // what's in a `_` name or in capitals is left alone: a cache
    accepted("python", "_cache = {}\ndef f(x: int) -> int:\n    _cache[x] = x\n    return len(_cache)\n", false);
}

#[test]
fn csharp_a_static_property_that_keeps_its_value_is_refused() {
    refused(
        "csharp",
        "static class A { static long N { get; set; } public static long F() { return ++N; } }",
        "`N` is a `static` property that keeps its value",
    );
    refused(
        "csharp",
        "static class A { static long N { get; private set; } public static long F() { N++; return N; } }",
        "`N` is a `static` property",
    );
    // one that gets and sets a field has it in the field; one that's read only is a constant
    accepted(
        "csharp",
        "static class A { static long n; static long N { get => n; set => n = value; } public static long F() { return ++N; } }",
        true,
    );
    accepted("csharp", "static class A { static long K { get; } = 3; public static long F() { return K; } }", false);
}

#[test]
fn csharp_a_static_of_another_type_is_refused() {
    refused(
        "csharp",
        "static class A { public static long F() { return ++Other.n; } }\nstatic class Other { public static long n = 0; }",
        "line 2: `n` is a `static` of `Other`, not of `A`, the class of `F`",
    );
    // a constant, or a `readonly` one (a cache), is not state
    accepted(
        "csharp",
        "static class A { public static long F() { return Other.K; } }\nstatic class Other { public const long K = 1; }",
        false,
    );
    accepted(
        "csharp",
        "static class A { static long n; public static long F() { return ++n; } }\nstatic class Other { public static readonly long K = 1; }",
        true,
    );
}

#[test]
fn lua_a_helper_that_changes_a_local_of_the_top_is_refused() {
    refused(
        "lua",
        "local count = 0\nlocal function bump() count = count + 1 end\n---@return integer result\nfunction f()\n    bump()\n    return count\nend\n",
        "`bump` changes `count`, which is `local` at the top",
    );
    // a function in the function too
    refused(
        "lua",
        "local count = 0\n---@return integer result\nfunction f()\n    local g = function() count = count + 1 end\n    g()\n    return count\nend\n",
        "`f` changes `count`, which is `local` at the top",
    );
    // what it changes of its own isn't the top's
    accepted(
        "lua",
        "local K = 2\nlocal function h(a, b) local c = a; for i = 1, b do c = c + i end; return c end\n---@return integer result\nfunction f()\n    return h(1, K)\nend\n",
        false,
    );
}

#[test]
fn lua_a_global_set_by_a_helper_is_state() {
    accepted(
        "lua",
        "local function bump() count = (count or 0) + 1 end\n---@return integer result\nfunction f()\n    bump()\n    return count\nend\n",
        true,
    );
    accepted(
        "lua",
        "---@return integer result\nfunction f()\n    local function g() count = (count or 0) + 1 end\n    g()\n    return count\nend\n",
        true,
    );
}

#[test]
fn lua_what_is_set_in_g_is_refused() {
    let says = "what's set in `_G` isn't the square's state";
    refused(
        "lua",
        "---@return integer result\nfunction f()\n    _G.count = (_G.count or 0) + 1\n    return _G.count\nend\n",
        says,
    );
    refused("lua", "---@return integer result\nfunction f()\n    _G[\"count\"] = 1\n    return 1\nend\n", says);
    refused("lua", "---@return integer result\nfunction f()\n    rawset(_G, \"count\", 1)\n    return 1\nend\n", says);
    // reading it is fine
    accepted("lua", "---@return integer result\nfunction f()\n    return _G.math.floor(1.5)\nend\n", false);
}

#[test]
fn csharp_a_static_of_a_type_lineit_doesnt_know_is_refused() {
    let says = "which lineit doesn't know: state goes in a `static` field of a type it knows";
    refused(
        "csharp",
        "static class A { static HashSet<long> seen = new(); public static long F(long x) { seen.Add(x); return seen.Count; } }",
        "line 1: `seen` is a `static` of type `HashSet<long>`, which lineit doesn't know",
    );
    refused("csharp", "static class A { static Random r = new(); public static long F() { return r.Next(); } }", says);
    refused(
        "csharp",
        "static class A { static Dictionary<long, long> memo = new(); public static long F(long x) { memo[x] = x; return x; } }",
        says,
    );
    refused(
        "csharp",
        "static class A { static (long, long) t; public static long F() { t.Item1++; return t.Item1; } }",
        says,
    );
    // in another class too
    refused(
        "csharp",
        "static class A { public static long F() { return 1; } }\nstatic class Other { static Random r; }",
        "`r` is a `static` of type `Random`",
    );
    // a `readonly` one (a cache it only fills) and a `const` are allowed, of any type
    accepted(
        "csharp",
        "static class A { static readonly HashSet<long> seen = new(); const long K = 1; public static long F(long x) { seen.Add(x); return seen.Count + K; } }",
        false,
    );
    // the types it knows are state
    accepted(
        "csharp",
        "static class A { static long n; static List<long> xs = new(); static long[] arr; static Dictionary<string, long> m = new(); static string s; public static long F() { return ++n; } }",
        true,
    );
}

#[test]
fn python_a_closure_at_the_top_that_the_function_calls_is_refused() {
    let mk =
        "def _mk():\n    n = 0\n    def inc():\n        nonlocal n\n        n += 1\n        return n\n    return inc\n";
    refused(
        "python",
        &format!("{mk}_inc = _mk()\ndef f() -> int:\n    return _inc()\n"),
        "line 8: `_inc` is what `_mk(...)` returns, and `f` calls it",
    );
    // in any name
    for name in ["inc", "_", "INC"] {
        refused(
            "python",
            &format!("{mk}{name} = _mk()\ndef f() -> int:\n    return {name}()\n"),
            &format!("`{name}` is what `_mk(...)` returns"),
        );
    }
    // from a helper of its own
    refused(
        "python",
        &format!("{mk}_inc = _mk()\ndef _h() -> int:\n    return _inc()\ndef f() -> int:\n    return _h()\n"),
        "and `_h` calls it",
    );
    // an iterator that moves
    refused(
        "python",
        "import itertools\n_c = itertools.count()\ndef f() -> int:\n    return next(_c)\n",
        "`_c` is what `itertools.count(...)` returns",
    );
    refused(
        "python",
        "from itertools import count as cnt\n_c = cnt()\ndef f() -> int:\n    return next(_c)\n",
        "`_c` is what `cnt(...)` returns",
    );
    refused(
        "python",
        "_it = iter([1, 2])\ndef f() -> int:\n    return next(_it)\n",
        "`_it` is what `iter(...)` returns",
    );
    // a function of the code is called, an import, a constant, or a value that isn't called
    accepted("python", "def _inc(x):\n    return x + 1\ndef f() -> int:\n    return _inc(1)\n", false);
    accepted("python", "import math\nfrom math import floor\ndef f() -> int:\n    return floor(math.sqrt(4))\n", false);
    accepted("python", &format!("{mk}_inc = _mk()\ndef f() -> int:\n    return 1\n"), false);
    accepted("python", "from collections import Counter\n_c = Counter()\ndef f() -> int:\n    return len(_c)\n", false);
    accepted("python", "import itertools\n_c = itertools.count()\ndef f() -> int:\n    return 1\n", false);
    // a variable of the function's own of that name is another one
    accepted("python", &format!("{mk}_inc = _mk()\ndef f() -> int:\n    _inc = lambda: 1\n    return _inc()\n"), false);
}

#[test]
fn lua_a_closure_at_the_top_that_the_function_calls_is_refused() {
    let mk = "local function mk()\n    local n = 0\n    return function() n = n + 1; return n end\nend\n";
    refused(
        "lua",
        &format!("{mk}local inc = mk()\n---@return integer result\nfunction f()\n    return inc()\nend\n"),
        "line 5: `inc` is what `mk(...)` returns, and `f` calls it",
    );
    // from a helper, with the maker a function value, or a coroutine
    refused(
        "lua",
        "local mk = function() local n = 0; return function() n = n + 1; return n end end\nlocal inc = mk()\nlocal function h() return inc() end\n---@return integer result\nfunction f()\n    return h()\nend\n",
        "`inc` is what `mk(...)` returns, and `h` calls it",
    );
    refused(
        "lua",
        "local gen = coroutine.wrap(function() while true do coroutine.yield(1) end end)\n---@return integer result\nfunction f()\n    return gen()\nend\n",
        "`gen` is what `coroutine.wrap(...)` returns",
    );
    // a function of the code, a library one, a value that isn't called, a local of its own
    accepted(
        "lua",
        "local function inc(x) return x + 1 end\n---@return integer result\nfunction f()\n    return inc(1)\nend\n",
        false,
    );
    accepted(
        "lua",
        "local floor = math.floor\nlocal k = tostring(3)\n---@return integer result\nfunction f()\n    return floor(2.5) + #k\nend\n",
        false,
    );
    accepted(
        "lua",
        &format!("{mk}local inc = mk()\n---@return integer result\nfunction f()\n    return 1\nend\n"),
        false,
    );
    accepted(
        "lua",
        &format!(
            "{mk}local inc = mk()\n---@return integer result\nfunction f()\n    local inc = function() return 1 end\n    return inc()\nend\n"
        ),
        false,
    );
}

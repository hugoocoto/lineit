-- JavaScript for code squares, run by Node.js: a language added with a
-- file. Copy it to lineit's `languages` folder (next to `modules`, in its
-- data folder: ~/.local/share/lineit/languages on Linux), or keep it in a
-- `languages` folder next to a canvas.
--
-- In a square:
--
--   var count = 0;  // a `var` at the top: the square's state
--
--   /**
--    * Halves even numbers; counts and passes on the odd ones.
--    * @param {int} n
--    * @param {(half: int) => void} even
--    * @param {(calls: int, n: int) => void} odd
--    */
--   export function collatz(n, even, odd) {
--       Shared.total += n;
--       if (n % 2 === 0) return even(n / 2);
--       count += 1;
--       return odd(count, n);
--   }
--
-- The function is the one `export`ed. Its types are JSDoc's: `int`,
-- `number` (a float), `string`, `boolean`, `T[]`, `Object<string, T>`, a
-- record's name; a parameter with none takes what's wired. `@returns {T}`
-- is `result`; `@returns {{a: int, b: string}}` returns an object with
-- those outputs. A parameter whose type is `(name: T, ...) => void` is an
-- exit. `Shared.NAME` is the variable ::NAME. `console.log` prints.
return {
        name = "javascript",
        title = "JavaScript",
        extensions = { "js", "mjs" },
        commands = { "node" },
        args = { "-e" },
        -- colors, if a compiled grammar and its highlight queries are there
        -- (here, nvim-treesitter's; elsewhere, no colors)
        grammar = "~/.local/share/nvim/site/parser/javascript.so",
        highlights = {
                "~/.local/share/nvim/site/pack/core/opt/nvim-treesitter/runtime/queries/ecma/highlights.scm",
                "~/.local/share/nvim/site/pack/core/opt/nvim-treesitter/runtime/queries/javascript/highlights.scm",
        },
        template = [==[/**
 * Twice x.
 * @param {int} x
 * @returns {int}
 */
export function double(x) {
    return x * 2;
}
]==],
        host = [==[
const vm = require("vm");
const readline = require("readline");

const send = (m) => process.stdout.write(JSON.stringify(m) + "\n");
// `export function` is how the code says which function it is; run, it's a function
const runnable = (src) => src.replace(/\bexport\s+(?=function\b)/g, "");

function lineitType(t) {
    t = t.trim();
    let m;
    if (t === "int" || t === "integer") return "int";
    if (t === "number" || t === "float") return "float";
    if (t === "string") return "str";
    if (t === "boolean") return "bool";
    if ((m = t.match(/^(.*)\[\]$/))) return "list<" + lineitType(m[1]) + ">";
    if ((m = t.match(/^Array<(.*)>$/))) return "list<" + lineitType(m[1]) + ">";
    if ((m = t.match(/^Object<\s*string\s*,(.*)>$/))) return "map<" + lineitType(m[1]) + ">";
    return t;
}

// "name: T, name: T" as [[name, type]...], at the top level of brackets
function fields(s) {
    const out = [];
    let depth = 0, start = 0;
    const parts = [];
    for (let i = 0; i < s.length; i++) {
        const c = s[i];
        if ("<({[".includes(c)) depth++;
        else if (">)}]".includes(c)) depth--;
        else if (c === "," && depth === 0) { parts.push(s.slice(start, i)); start = i + 1; }
    }
    parts.push(s.slice(start));
    for (const p of parts) {
        if (!p.trim()) continue;
        const k = p.indexOf(":");
        if (k < 0) out.push([null, lineitType(p)]);
        else out.push([p.slice(0, k).trim(), lineitType(p.slice(k + 1))]);
    }
    return out;
}

function lineOf(e) {
    const m = String(e.stack || "").match(/:(\d+)/);
    return m ? "line " + m[1] + ": " : "";
}

function signature(src) {
    try {
        new vm.Script(runnable(src), { filename: "code" });
    } catch (e) {
        return { error: lineOf(e) + e.name + ": " + e.message };
    }
    const fns = [...src.matchAll(/(?:\/\*\*((?:[^*]|\*(?!\/))*)\*\/\s*)?export\s+function\s+(\w+)\s*\(([^)]*)\)/g)];
    if (fns.length === 0) return { error: "there's no function: `export function name(x) { ... }`" };
    if (fns.length > 1) return { error: "it runs one function, and " + fns.map((f) => f[2]).join(", ") + " are exported: export only it" };
    const [, doc = "", name, plist] = fns[0];
    const lines = doc.split("\n").map((l) => l.replace(/^\s*\*?\s?/, ""));
    const text = lines.filter((l) => l && !l.startsWith("@")).join(" ").trim();
    const types = {};
    let returns = [];
    // whether it returns an object with its outputs, or the one output
    let object = false;
    for (const l of lines) {
        let m;
        if ((m = l.match(/^@param\s+\{(.*)\}\s+\[?(\w+)/))) types[m[2]] = m[1];
        else if ((m = l.match(/^@returns?\s+\{(.*)\}\s*(\w+)?/))) {
            const t = m[1].trim();
            const obj = t.match(/^\{(.*)\}$/);
            object = !!obj;
            returns = obj ? fields(obj[1]) : [[m[2] || null, lineitType(t)]];
        }
    }
    const params = [];
    for (const raw of plist.split(",").map((p) => p.trim()).filter((p) => p)) {
        const [pname, def] = raw.split("=").map((s) => s.trim());
        const t = types[pname];
        const exit = t && t.match(/^\((.*)\)\s*=>\s*void$/);
        if (exit) params.push({ name: pname, exit: fields(exit[1]) });
        else params.push({ name: pname, type: t ? lineitType(t) : null, default: def === undefined ? null : def.replace(/^["']|["']$/g, "") });
    }
    // its state: the variables running it makes (not its functions)
    const ctx = vm.createContext({ Shared: {}, console: { log() {} } });
    try { new vm.Script(runnable(src)).runInContext(ctx); } catch (e) { return { error: lineOf(e) + e.name + ": " + e.message }; }
    const state = Object.keys(ctx).filter((k) => k !== "Shared" && k !== "console" && typeof ctx[k] !== "function");
    const shared = {};
    for (const m of src.matchAll(/\bShared\.(\w+)(\s*(?:[-+*/%]?=(?!=)|\+\+|--))?/g)) shared[m[1]] = shared[m[1]] || !!m[2];
    return { name, doc: text, params, returns, object, state, shared: Object.entries(shared) };
}

let load = null;
let script = null;
const squares = [];
let printed = [];
const answer = (m) => { if (printed.length) m.printed = printed; printed = []; send(m); };

function call(req) {
    const ctx = squares[req.sq];
    const i = req.i;
    for (const [g] of load.shared) ctx.Shared[g] = i["::" + g];
    const fire = [];
    const args = load.params.map((p) => p.exit === null ? i[p.name] : (...v) => {
        const values = {};
        p.exit.forEach((pin, k) => { values[pin] = v[k]; });
        fire.push([p.name, values]);
    });
    const r = ctx[load.name](...args);
    const set = {};
    if (load.signature.object) load.returns.forEach((pin) => { set[pin] = r[pin]; });
    else if (load.returns.length === 1) set[load.returns[0]] = r;
    for (const [g, sets] of load.shared) if (sets) set["::" + g] = ctx.Shared[g];
    if (load.then) fire.push(["then", {}]);
    return { set, fire };
}

const rl = readline.createInterface({ input: process.stdin });
rl.on("line", (line) => {
    const req = JSON.parse(line);
    try {
        if (req.op === "signature") {
            send(signature(req.source));
            process.exit(0);
        } else if (req.op === "load") {
            load = req;
            script = new vm.Script(runnable(load.source), { filename: load.file });
            answer({});
        } else if (req.op === "new") {
            const ctx = vm.createContext({ Shared: {}, console: { log: (...a) => printed.push(a.map(String).join(" ")) } });
            script.runInContext(ctx);
            squares.push(ctx);
            answer({ sq: squares.length - 1 });
        } else if (req.op === "call") {
            answer(call(req));
        } else if (req.op === "state") {
            const ctx = squares[req.sq];
            answer({ state: Object.fromEntries(load.state.filter((k) => k in ctx).map((k) => [k, ctx[k]])) });
        } else if (req.op === "set_state") {
            Object.assign(squares[req.sq], req.state);
            answer({});
        }
    } catch (e) {
        const at = String(e.stack || "").match(new RegExp(load ? load.file.replace(/[.]/g, "\\.") + ":(\\d+)" : "$^"));
        answer({ error: (at ? load.file + ":" + at[1] + ": " : "") + e.name + ": " + e.message });
    }
});
]==],
}

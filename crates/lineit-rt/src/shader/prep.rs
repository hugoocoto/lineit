//! What is done to the text of a shader before the parser sees it: the
//! preprocessor (`#define`, `#undef`, `#if`, `#ifdef`, `#ifndef`, `#elif`,
//! `#else`, `#endif`), and two things the grammar can't read: numbers like
//! `1e-3` (it asks for a dot and a sign) and arrays like `float[3] a` (it
//! reads `float a[3]` only). Lines stay where they were, so the errors
//! name the right one.

use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Id(String),
    Num(String),
    Sp(String),
    Comment(String),
    Other(char),
}

impl Tok {
    fn text(&self) -> String {
        match self {
            Tok::Id(s) | Tok::Num(s) | Tok::Sp(s) | Tok::Comment(s) => s.clone(),
            Tok::Other(c) => c.to_string(),
        }
    }
}

struct Macro {
    /// None: it has no parentheses (`#define PI 3.14`).
    params: Option<Vec<String>>,
    body: Vec<Tok>,
}

/// A conditional being read: if its lines count now, if one of its
/// branches was taken already, and if the one around it counts.
struct Cond {
    now: bool,
    taken: bool,
    outer: bool,
}

pub fn run(src: &str) -> Result<String, String> {
    let mut macros: HashMap<String, Macro> = HashMap::new();
    let mut conds: Vec<Cond> = Vec::new();
    let mut in_block = false;
    let mut out = String::with_capacity(src.len());
    for (n, line) in src.lines().enumerate() {
        let n = n + 1;
        let active = conds.iter().all(|c| c.now);
        let directive = !in_block && line.trim_start().starts_with('#');
        if directive {
            let rest = line.trim_start()[1..].trim_start();
            let word: String = rest.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
            let arg = rest[word.len()..].trim();
            match word.as_str() {
                "ifdef" | "ifndef" => {
                    let defined = macros.contains_key(arg.split_whitespace().next().unwrap_or(""));
                    let now = active && (defined == (word == "ifdef"));
                    conds.push(Cond { now, taken: now, outer: active });
                }
                "if" => {
                    let now = active && cond_value(arg, &macros).map_err(|e| format!("line {n}: {e}"))? != 0;
                    conds.push(Cond { now, taken: now, outer: active });
                }
                "elif" => {
                    let Some(c) = conds.last() else { return Err(format!("line {n}: #elif without #if")) };
                    let (taken, outer) = (c.taken, c.outer);
                    let now = outer && !taken && cond_value(arg, &macros).map_err(|e| format!("line {n}: {e}"))? != 0;
                    let c = conds.last_mut().unwrap();
                    c.now = now;
                    c.taken |= now;
                }
                "else" => {
                    let Some(c) = conds.last_mut() else { return Err(format!("line {n}: #else without #if")) };
                    c.now = c.outer && !c.taken;
                    c.taken = true;
                }
                "endif" => {
                    if conds.pop().is_none() {
                        return Err(format!("line {n}: #endif without #if"));
                    }
                }
                _ if !active => {}
                "define" => {
                    let (name, m) = define(arg).map_err(|e| format!("line {n}: {e}"))?;
                    macros.insert(name, m);
                }
                "undef" => {
                    macros.remove(arg.split_whitespace().next().unwrap_or(""));
                }
                "include" => return Err(format!("line {n}: #include is not supported")),
                // #pragma, #error...: nothing to do
                _ => {}
            }
            out.push('\n');
            continue;
        }
        if !active {
            // a comment that starts here still has to end
            lex(line, &mut in_block);
            out.push('\n');
            continue;
        }
        let toks = lex(line, &mut in_block);
        let toks = expand(toks, &macros, &mut Vec::new()).map_err(|e| format!("line {n}: {e}"))?;
        let toks = fix(toks);
        for t in &toks {
            out.push_str(&t.text());
        }
        out.push('\n');
    }
    if !conds.is_empty() {
        return Err("#if without #endif".into());
    }
    Ok(out)
}

fn define(arg: &str) -> Result<(String, Macro), String> {
    let name: String = arg.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    if name.is_empty() {
        return Err("#define needs a name".into());
    }
    let mut rest = &arg[name.len()..];
    let mut params = None;
    if let Some(r) = rest.strip_prefix('(') {
        let Some(close) = r.find(')') else { return Err("#define: a ( without its )".into()) };
        params = Some(r[..close].split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect());
        rest = &r[close + 1..];
    }
    let mut in_block = false;
    let mut body: Vec<Tok> = lex(rest, &mut in_block).into_iter().filter(|t| !matches!(t, Tok::Comment(_))).collect();
    while matches!(body.first(), Some(Tok::Sp(_))) {
        body.remove(0);
    }
    while matches!(body.last(), Some(Tok::Sp(_))) {
        body.pop();
    }
    Ok((name, Macro { params, body }))
}

fn lex(s: &str, in_block: &mut bool) -> Vec<Tok> {
    let cs: Vec<char> = s.chars().collect();
    let mut toks = Vec::new();
    let mut i = 0;
    while i < cs.len() {
        if *in_block {
            let start = i;
            while i < cs.len() && !(cs[i] == '*' && cs.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            if i < cs.len() {
                i += 2;
                *in_block = false;
            }
            toks.push(Tok::Comment(cs[start..i].iter().collect()));
            continue;
        }
        let c = cs[i];
        let next = cs.get(i + 1).copied();
        if c == '/' && next == Some('/') {
            toks.push(Tok::Comment(cs[i..].iter().collect()));
            break;
        } else if c == '/' && next == Some('*') {
            *in_block = true;
            toks.push(Tok::Comment("/*".into()));
            i += 2;
        } else if c.is_whitespace() {
            let start = i;
            while i < cs.len() && cs[i].is_whitespace() {
                i += 1;
            }
            toks.push(Tok::Sp(cs[start..i].iter().collect()));
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < cs.len() && (cs[i].is_ascii_alphanumeric() || cs[i] == '_') {
                i += 1;
            }
            toks.push(Tok::Id(cs[start..i].iter().collect()));
        } else if c.is_ascii_digit() || (c == '.' && next.is_some_and(|d| d.is_ascii_digit())) {
            let start = i;
            i = number_end(&cs, i);
            toks.push(Tok::Num(cs[start..i].iter().collect()));
        } else {
            toks.push(Tok::Other(c));
            i += 1;
        }
    }
    toks
}

fn number_end(cs: &[char], mut i: usize) -> usize {
    if cs[i] == '0' && matches!(cs.get(i + 1), Some('x' | 'X')) {
        i += 2;
        while i < cs.len() && cs[i].is_ascii_hexdigit() {
            i += 1;
        }
        if matches!(cs.get(i), Some('u' | 'U')) {
            i += 1;
        }
        return i;
    }
    while i < cs.len() && cs[i].is_ascii_digit() {
        i += 1;
    }
    if cs.get(i) == Some(&'.') {
        i += 1;
        while i < cs.len() && cs[i].is_ascii_digit() {
            i += 1;
        }
    }
    if matches!(cs.get(i), Some('e' | 'E')) {
        let mut j = i + 1;
        if matches!(cs.get(j), Some('+' | '-')) {
            j += 1;
        }
        if cs.get(j).is_some_and(|d| d.is_ascii_digit()) {
            while j < cs.len() && cs[j].is_ascii_digit() {
                j += 1;
            }
            i = j;
        }
    }
    if matches!(cs.get(i), Some('f' | 'F' | 'u' | 'U')) {
        i += 1;
    }
    i
}

/// `1e-3` as the grammar wants it: `1.0e-3`; `2E5`: `2.0e+5`.
fn fix_number(s: &str) -> String {
    if s.starts_with("0x") || s.starts_with("0X") {
        return s.to_string();
    }
    let Some(e) = s.find(['e', 'E']) else { return s.to_string() };
    let (mantissa, exp) = s.split_at(e);
    let mut out = mantissa.to_string();
    if !mantissa.contains('.') {
        out.push_str(".0");
    }
    out.push_str(&exp[..1]);
    if !exp[1..].starts_with(['+', '-']) {
        out.push('+');
    }
    out.push_str(&exp[1..]);
    out
}

fn expand(toks: Vec<Tok>, macros: &HashMap<String, Macro>, hide: &mut Vec<String>) -> Result<Vec<Tok>, String> {
    if hide.len() > 16 {
        return Err("macros inside macros go too deep".into());
    }
    let mut out = Vec::with_capacity(toks.len());
    let mut i = 0;
    while i < toks.len() {
        let Tok::Id(name) = &toks[i] else {
            out.push(toks[i].clone());
            i += 1;
            continue;
        };
        let Some(m) = macros.get(name).filter(|_| !hide.contains(name)) else {
            out.push(toks[i].clone());
            i += 1;
            continue;
        };
        match &m.params {
            None => {
                hide.push(name.clone());
                out.extend(expand(m.body.clone(), macros, hide)?);
                hide.pop();
                i += 1;
            }
            Some(params) => {
                // a function-like macro is only called with its parentheses
                let mut j = i + 1;
                while matches!(toks.get(j), Some(Tok::Sp(_))) {
                    j += 1;
                }
                if toks.get(j) != Some(&Tok::Other('(')) {
                    out.push(toks[i].clone());
                    i += 1;
                    continue;
                }
                let (mut depth, mut args, mut cur) = (0, Vec::new(), Vec::new());
                let mut k = j;
                loop {
                    let Some(t) = toks.get(k) else { return Err(format!("{name}(: no closing )")) };
                    k += 1;
                    match t {
                        Tok::Other('(') => {
                            depth += 1;
                            if depth > 1 {
                                cur.push(t.clone());
                            }
                        }
                        Tok::Other(')') => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                            cur.push(t.clone());
                        }
                        Tok::Other(',') if depth == 1 => args.push(std::mem::take(&mut cur)),
                        _ => cur.push(t.clone()),
                    }
                }
                args.push(cur);
                if params.is_empty() && args.len() == 1 && args[0].iter().all(|t| matches!(t, Tok::Sp(_))) {
                    args.clear();
                }
                if args.len() != params.len() {
                    return Err(format!("{name} takes {} arguments, not {}", params.len(), args.len()));
                }
                let mut body = Vec::new();
                for t in &m.body {
                    match t {
                        Tok::Id(p) if params.contains(p) => {
                            let a = &args[params.iter().position(|x| x == p).unwrap()];
                            body.extend(a.iter().cloned());
                        }
                        _ => body.push(t.clone()),
                    }
                }
                hide.push(name.clone());
                out.extend(expand(body, macros, hide)?);
                hide.pop();
                i = k;
            }
        }
    }
    Ok(out)
}

/// The numbers and the arrays the grammar can't read.
fn fix(toks: Vec<Tok>) -> Vec<Tok> {
    let toks: Vec<Tok> = toks
        .into_iter()
        .map(|t| match t {
            Tok::Num(s) => Tok::Num(fix_number(&s)),
            t => t,
        })
        .collect();
    let mut out = Vec::with_capacity(toks.len());
    let mut i = 0;
    while i < toks.len() {
        // Id [ Num ] Sp Id, and no ( after: a declaration `float[3] a`
        if let (Some(Tok::Id(ty)), Some(Tok::Other('[')), Some(Tok::Num(size)), Some(Tok::Other(']'))) =
            (toks.get(i), toks.get(i + 1), toks.get(i + 2), toks.get(i + 3))
        {
            let mut j = i + 4;
            while matches!(toks.get(j), Some(Tok::Sp(_))) {
                j += 1;
            }
            let name = match toks.get(j) {
                Some(Tok::Id(name)) if j > i + 4 => Some(name),
                _ => None,
            };
            if let Some(name) = name {
                let mut after = j + 1;
                while matches!(toks.get(after), Some(Tok::Sp(_))) {
                    after += 1;
                }
                if toks.get(after) != Some(&Tok::Other('(')) {
                    out.push(Tok::Id(ty.clone()));
                    out.push(Tok::Sp(" ".into()));
                    out.push(Tok::Id(name.clone()));
                    out.push(Tok::Other('['));
                    out.push(Tok::Num(size.clone()));
                    out.push(Tok::Other(']'));
                    i = j + 1;
                    continue;
                }
            }
        }
        out.push(toks[i].clone());
        i += 1;
    }
    out
}

/// The value of the condition of an `#if`: `defined(X)`, numbers, macros
/// that are numbers, `! && || == != < > <= >=` and parentheses.
fn cond_value(arg: &str, macros: &HashMap<String, Macro>) -> Result<i64, String> {
    let mut in_block = false;
    let toks: Vec<Tok> =
        lex(arg, &mut in_block).into_iter().filter(|t| !matches!(t, Tok::Sp(_) | Tok::Comment(_))).collect();
    let mut p = Cond2 { toks: &toks, i: 0, macros };
    let v = p.or()?;
    if p.i < toks.len() {
        return Err("#if: can't read this condition".into());
    }
    Ok(v)
}

struct Cond2<'a> {
    toks: &'a [Tok],
    i: usize,
    macros: &'a HashMap<String, Macro>,
}

impl Cond2<'_> {
    fn pair(&mut self, a: char, b: char) -> bool {
        if self.toks.get(self.i) == Some(&Tok::Other(a)) && self.toks.get(self.i + 1) == Some(&Tok::Other(b)) {
            self.i += 2;
            true
        } else {
            false
        }
    }

    fn or(&mut self) -> Result<i64, String> {
        let mut v = self.and()?;
        while self.pair('|', '|') {
            let r = self.and()?;
            v = (v != 0 || r != 0) as i64;
        }
        Ok(v)
    }

    fn and(&mut self) -> Result<i64, String> {
        let mut v = self.cmp()?;
        while self.pair('&', '&') {
            let r = self.cmp()?;
            v = (v != 0 && r != 0) as i64;
        }
        Ok(v)
    }

    fn cmp(&mut self) -> Result<i64, String> {
        let l = self.unary()?;
        let r = |s: &mut Self| s.unary();
        Ok(if self.pair('=', '=') {
            (l == r(self)?) as i64
        } else if self.pair('!', '=') {
            (l != r(self)?) as i64
        } else if self.pair('<', '=') {
            (l <= r(self)?) as i64
        } else if self.pair('>', '=') {
            (l >= r(self)?) as i64
        } else if self.toks.get(self.i) == Some(&Tok::Other('<')) {
            self.i += 1;
            (l < r(self)?) as i64
        } else if self.toks.get(self.i) == Some(&Tok::Other('>')) {
            self.i += 1;
            (l > r(self)?) as i64
        } else {
            l
        })
    }

    fn unary(&mut self) -> Result<i64, String> {
        match self.toks.get(self.i).cloned() {
            Some(Tok::Other('!')) => {
                self.i += 1;
                Ok((self.unary()? == 0) as i64)
            }
            Some(Tok::Other('(')) => {
                self.i += 1;
                let v = self.or()?;
                if self.toks.get(self.i) != Some(&Tok::Other(')')) {
                    return Err("#if: a ( without its )".into());
                }
                self.i += 1;
                Ok(v)
            }
            Some(Tok::Num(n)) => {
                self.i += 1;
                Ok(n.trim_end_matches(['u', 'U']).parse::<i64>().unwrap_or(0))
            }
            Some(Tok::Id(id)) if id == "defined" => {
                self.i += 1;
                let paren = self.toks.get(self.i) == Some(&Tok::Other('('));
                if paren {
                    self.i += 1;
                }
                let Some(Tok::Id(name)) = self.toks.get(self.i).cloned() else {
                    return Err("#if: defined needs a name".into());
                };
                self.i += 1;
                if paren {
                    if self.toks.get(self.i) != Some(&Tok::Other(')')) {
                        return Err("#if: a ( without its )".into());
                    }
                    self.i += 1;
                }
                Ok(self.macros.contains_key(&name) as i64)
            }
            Some(Tok::Id(id)) => {
                self.i += 1;
                // a macro that is a number; anything else is 0
                Ok(self
                    .macros
                    .get(&id)
                    .and_then(|m| m.body.first())
                    .and_then(|t| if let Tok::Num(n) = t { n.parse::<i64>().ok() } else { None })
                    .unwrap_or(0))
            }
            _ => Err("#if: can't read this condition".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_and_arrays() {
        assert_eq!(run("float a = 1e-3 + 2E5 + 0x1e3;").unwrap().trim(), "float a = 1.0e-3 + 2.0E+5 + 0x1e3;");
        assert_eq!(
            run("float[3] a = float[3](1.0, 2.0, 3.0);").unwrap().trim(),
            "float a[3] = float[3](1.0, 2.0, 3.0);"
        );
        assert_eq!(run("// 1e-3 float[3] a\n").unwrap().trim(), "// 1e-3 float[3] a");
    }

    #[test]
    fn macros_and_conditions() {
        let src =
            "#define PI 3.14\n#define SQ(x) ((x)*(x))\n#ifdef PI\nfloat a = SQ(PI);\n#else\nfloat a = 0.0;\n#endif\n";
        assert_eq!(run(src).unwrap().lines().nth(3).unwrap(), "float a = ((3.14)*(3.14));");
        assert_eq!(run(src).unwrap().lines().count(), 7);
        let src = "#define N 2\n#if N > 1 && !defined(X)\nyes\n#elif 1\nno\n#endif\n";
        assert_eq!(run(src).unwrap().lines().nth(2).unwrap(), "yes");
        assert_eq!(run(src).unwrap().lines().nth(4).unwrap(), "");
        assert!(run("#include \"a\"\n").is_err());
        assert!(run("#if 1\n").is_err());
    }
}

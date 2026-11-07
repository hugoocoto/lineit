//! Strings. Positions and lengths count characters (Unicode scalar
//! values), not bytes, so `substring` never splits one.

use crate::show::Show;

pub fn length(s: &str) -> i64 {
    s.chars().count() as i64
}

pub fn upper(s: &str) -> String {
    s.to_uppercase()
}

pub fn lower(s: &str) -> String {
    s.to_lowercase()
}

pub fn concat(a: &str, b: &str) -> String {
    let mut s = String::with_capacity(a.len() + b.len());
    s.push_str(a);
    s.push_str(b);
    s
}

/// Where NEEDLE first is in S, in characters, or -1.
pub fn find(s: &str, needle: &str) -> i64 {
    match s.find(needle) {
        Some(b) => s[..b].chars().count() as i64,
        None => -1,
    }
}

/// LEN characters from START. A negative START counts from the end; what
/// falls outside the string is left out.
pub fn substring(s: &str, start: i64, len: i64) -> String {
    let n = length(s);
    let start = if start < 0 { (n + start).max(0) } else { start.min(n) };
    let len = len.clamp(0, n - start);
    s.chars().skip(start as usize).take(len as usize).collect()
}

/// An empty SEP splits into characters.
pub fn split(s: &str, sep: &str) -> Vec<String> {
    if sep.is_empty() { s.chars().map(String::from).collect() } else { s.split(sep).map(String::from).collect() }
}

pub fn join(parts: &[String], sep: &str) -> String {
    parts.join(sep)
}

pub fn replace(s: &str, from: &str, to: &str) -> String {
    if from.is_empty() { s.to_string() } else { s.replace(from, to) }
}

pub fn trim(s: &str) -> String {
    s.trim().to_string()
}

pub fn repeat(s: &str, n: i64) -> String {
    s.repeat(n.clamp(0, 1 << 20) as usize)
}

/// FMT with each `{}` replaced by the next of ARGS, as `io/print` would
/// print it. `{{` and `}}` are a brace. Missing args leave `{}` empty.
pub fn format(fmt: &str, args: &[&dyn Show]) -> String {
    let mut out = String::with_capacity(fmt.len() + 8 * args.len());
    let mut args = args.iter();
    let mut chars = fmt.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                out.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                out.push('}');
            }
            '{' if chars.peek() == Some(&'}') => {
                chars.next();
                if let Some(a) = args.next() {
                    a.show(&mut out);
                }
            }
            c => out.push(c),
        }
    }
    out
}

/// How many `{}` FMT has.
pub fn format_count(fmt: &str) -> usize {
    let mut n = 0;
    let mut chars = fmt.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, chars.peek()) {
            ('{', Some('{')) | ('}', Some('}')) => {
                chars.next();
            }
            ('{', Some('}')) => {
                chars.next();
                n += 1;
            }
            _ => {}
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings() {
        assert_eq!(substring("héllo", 1, 3), "éll");
        assert_eq!(substring("hello", -3, 10), "llo");
        assert_eq!(substring("hello", 9, 1), "");
        assert_eq!(find("héllo", "l"), 2);
        assert_eq!(find("hello", "z"), -1);
        assert_eq!(split("a,b,,c", ","), ["a", "b", "", "c"]);
        assert_eq!(split("ab", ""), ["a", "b"]);
        assert_eq!(format("{} + {} = {}{{}}", &[&1i64, &2.5f64, &"x".to_string()]), "1 + 2.5 = x{}");
        assert_eq!(format_count("{} {{}} {}"), 2);
    }
}

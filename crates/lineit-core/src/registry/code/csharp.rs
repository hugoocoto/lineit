//! Code squares in C#, run by the machine's .NET SDK (`dotnet run`, which
//! needs .NET 10 or newer: it runs a `.cs` file as it is).
//!
//! ```csharp
//! delegate void Odd(long calls, long n);
//!
//! static class Collatz
//! {
//!     static long count = 0; // the square's state
//!
//!     /// <summary>Halves even numbers; counts and passes on the odd ones.</summary>
//!     public static void Run(long n, Action<long> even, Odd odd)
//!     {
//!         Shared.total += n;
//!         if (n % 2 == 0) { even(n / 2); return; }
//!         count++;
//!         odd(count, n);
//!     }
//! }
//! ```
//!
//! - The function is the one `public static` method; its helpers aren't
//!   `public`.
//! - Types are C#'s: `int` and `long` (and the other integers), `double`
//!   and `float`, `bool`, `string`, `T[]` and `List<T>` (`IList`,
//!   `IEnumerable`...), `Dictionary<string, T>`, or a record's name (a
//!   class of the code with its fields). A parameter's default (`k = 2`) is
//!   what its field has at first.
//! - What it returns: one value is `result`, a tuple `out1`, `out2`... (or
//!   its element names), a class, struct or record of the code the names
//!   of its public fields and properties.
//! - A parameter of type `Action<T, ...>` is an exit: `NAME.value` for
//!   one value, `NAME.1`, `NAME.2`... for several. One of a `delegate` of
//!   the code gives its parameters' names, as the values'; `Action<(long
//!   half, string tag)>` the tuple's element names, and `Action<Pt>`, with
//!   `Pt` a class, struct or record of the code, its public fields and
//!   properties'.
//! - Its state: the `static` fields of its class that aren't `const` or
//!   `readonly`. Each square runs the code in an assembly context of its
//!   own, so each has its own.
//! - `Shared.NAME` is the variable `::NAME`, wherever in the code it's used:
//!   lineit gives the code a `Shared` class with a static field for each.
//! - `#:package Name@version` lines at the top (`dotnet run`'s directives)
//!   are kept, so NuGet packages (MessagePack...) can be used.
//!
//! The code goes in a file as it is, with [`HOST`] and the `Shared` class
//! after it (`#line` makes `dotnet run` say the code's lines in its
//! errors). The SDK builds it once and keeps the build; [`HOST`] answers the
//! requests of [`lineit_rt::process`].

use std::path::PathBuf;

use lineit_rt::Type;
use serde_json::json;
use tree_sitter::Node;

use super::{Function, Language, Param, Role, descendants, exit_values, return_values};
use crate::ir::ExtKind;
use crate::registry::SigCx;

pub struct CSharp;

impl Language for CSharp {
    fn name(&self) -> &str {
        "csharp"
    }

    fn title(&self) -> &str {
        "C#"
    }

    fn extensions(&self) -> Vec<String> {
        vec!["cs".into()]
    }

    fn highlights(&self) -> Option<(tree_sitter::Language, &str)> {
        Some((tree_sitter_c_sharp::LANGUAGE.into(), tree_sitter_c_sharp::HIGHLIGHTS_QUERY))
    }

    fn template(&self) -> &str {
        "static class Square\n{\n    /// <summary>Twice x.</summary>\n    public static long Twice(long x)\n    {\n        return x * 2;\n    }\n}\n"
    }

    fn read(&self, source: &str, cx: &dyn SigCx) -> Result<Function, String> {
        read(source, cx).map(|(f, _)| f)
    }

    fn ext(&self, f: &Function) -> Result<ExtKind, String> {
        let (_, read) = read(&f.source, &super::FunctionRecords(f))?;
        let mode = read.mode;
        let file = f.origin("cs", true);
        let params: Vec<_> = f
            .params
            .iter()
            .map(|p| match &p.role {
                Role::Input(_) => json!({ "name": p.name, "exit": null }),
                Role::Exit(values) => {
                    let shape = read.shapes.iter().find(|(n, _)| *n == p.name).map_or("args", |(_, s)| *s);
                    json!({ "name": p.name, "exit": values.iter().map(|(pin, _)| pin).collect::<Vec<_>>(), "shape": shape })
                }
            })
            .collect();
        let load = json!({
            "name": f.name,
            "file": file,
            "mode": mode,
            "params": params,
            "returns": f.returns.iter().map(|(pin, _)| pin).collect::<Vec<_>>(),
            "shared": f.shared.iter().map(|(g, set)| json!([g, set])).collect::<Vec<_>>(),
            "then": f.ticked() && f.exits().next().is_none(),
            "state": f.state,
        });
        let path = write_file(f, &file)?;
        Ok(ExtKind::Process {
            name: f.name.clone(),
            commands: vec!["dotnet".into()],
            // quiet: no build output (warnings) on the way
            args: vec!["run".into(), "-v".into(), "q".into()],
            // what it does the first time it runs, that isn't running the code
            env: [
                ("DOTNET_NOLOGO", "1"),
                ("DOTNET_CLI_TELEMETRY_OPTOUT", "1"),
                ("DOTNET_SKIP_FIRST_TIME_EXPERIENCE", "1"),
                ("DOTNET_GENERATE_ASPNET_CERTIFICATE", "false"),
            ]
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .to_vec(),
            lock: path.parent().map(|d| d.to_string_lossy().into_owned()),
            host: path.to_string_lossy().into_owned(),
            load: load.to_string(),
        })
    }
}

/// The oldest .NET SDK that runs a `.cs` file as it is.
const MIN_SDK: u32 = 10;

/// Whether the machine has a .NET SDK that runs C# squares: `dotnet` and
/// version [`MIN_SDK`] or newer. Asked once.
pub fn sdk_ready() -> bool {
    static READY: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *READY.get_or_init(|| {
        let out = std::process::Command::new("dotnet").arg("--version").output();
        let version = out.ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).into_owned());
        version.and_then(|v| v.split('.').next()?.trim().parse::<u32>().ok()).is_some_and(|major| major >= MIN_SDK)
    })
}

/// What `dotnet run` is told about the file: what the host needs, and no
/// warnings. These lines come before the code's, which `#line` numbers from 1.
const SETTINGS: &str = "#:property ImplicitUsings=enable
#:property InvariantGlobalization=true
#:property Nullable=disable
#:property PublishAot=false
#:property WarningLevel=0
#:property DebugType=embedded
";

/// The program the code is run by: it reads what to load, then answers
/// requests (see [`lineit_rt::process`]). It loads the code's assembly again
/// in a context of its own for each square, which gives each its statics.
/// What the code prints comes with each answer, not mixed with them.
pub const HOST: &str = include_str!("csharp_host.cs");

/// The file `dotnet run` runs: SETTINGS, the code, the `Shared` class and
/// [`HOST`]. Kept where lineit keeps the crates of code squares, a folder
/// for each text of code.
fn write_file(f: &Function, file: &str) -> Result<PathBuf, String> {
    let mut shared = String::from("static class Shared\n{\n");
    for ((name, _), t) in f.shared.iter().zip(&f.shared_types) {
        let (ty, init) = cs_name(t)?;
        shared.push_str(&format!("    public static {ty} {name}{init};\n"));
    }
    shared.push_str("}\n");
    let text =
        format!("{SETTINGS}#line 1 \"{file}\"\n{}\n#line default\n{shared}{HOST}", f.source.trim_end_matches('\n'));
    let h = format!("{:016x}", crate::codegen::fnv(text.as_bytes()));
    let dir = crate::paths::cache_dir().join("code").join(format!("lineit-cs-{h}"));
    let path = dir.join("code.cs");
    if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        // written whole or not at all: another process can start it any time
        let tmp = dir.join(format!("code.cs.{}.tmp", std::process::id()));
        std::fs::write(&tmp, &text).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    super::square_used(&dir);
    Ok(path)
}

/// The C# type of a lineit type, and what a static of it starts with.
fn cs_name(t: &Type) -> Result<(String, &'static str), String> {
    Ok(match t {
        Type::Int => ("long".into(), ""),
        Type::Float => ("double".into(), ""),
        Type::Bool => ("bool".into(), ""),
        Type::Str => ("string".into(), " = \"\""),
        Type::List(e) => (format!("System.Collections.Generic.List<{}>", cs_name(e)?.0), " = new()"),
        Type::Map(e) => (format!("System.Collections.Generic.Dictionary<string, {}>", cs_name(e)?.0), " = new()"),
        Type::Record(r) => (r.name.clone(), ""),
        t => return Err(format!("a variable of type {t} can't be used in C#")),
    })
}

/// What a type in C# is: a lineit type, or an exit's values.
enum CsType {
    T(Type),
    /// Its values, and how the code gives them: `args` (the delegate's
    /// parameters), `tuple` (one tuple's elements) or `fields` (those of
    /// one class, struct or record of the code).
    Exit(Vec<(Option<String>, Type)>, &'static str),
}

/// What the code declares that its types name: its delegates, and the
/// classes, structs and records.
struct Decls<'a> {
    delegates: Vec<(&'a str, Node<'a>)>,
    types: Vec<(&'a str, Node<'a>)>,
}

impl<'a> Decls<'a> {
    fn type_of(&self, name: &str) -> Option<Node<'a>> {
        self.types.iter().find(|(n, _)| *n == name).map(|(_, n)| *n)
    }
}

/// The elements of a tuple type: their names, if they have them, and types.
fn tuple_elements<'a>(t: Node<'a>, src: &str) -> Vec<(Option<String>, Node<'a>)> {
    let mut c = t.walk();
    t.named_children(&mut c)
        .filter(|e| e.kind() == "tuple_element")
        .filter_map(|e| {
            Some((
                e.child_by_field_name("name").map(|n| src[n.byte_range()].to_string()),
                e.child_by_field_name("type")?,
            ))
        })
        .collect()
}

fn cs_type(n: Node, src: &str, cx: &dyn SigCx, d: &Decls) -> Result<CsType, String> {
    let text = |n: Node| &src[n.byte_range()];
    let unknown = || format!("`{}` isn't a type lineit knows", text(n));
    let plain = |n: Node| match cs_type(n, src, cx, d)? {
        CsType::T(t) => Ok(t),
        CsType::Exit(..) => Err(format!("`{}`: an exit is a parameter of its own", text(n))),
    };
    Ok(match n.kind() {
        "predefined_type" => CsType::T(match text(n) {
            "int" | "long" | "short" | "byte" | "sbyte" | "uint" | "ulong" | "ushort" => Type::Int,
            "double" | "float" | "decimal" => Type::Float,
            "bool" => Type::Bool,
            "string" => Type::Str,
            _ => return Err(unknown()),
        }),
        "nullable_type" => return cs_type(n.child_by_field_name("type").ok_or_else(unknown)?, src, cx, d),
        // `System.Action` is `Action`
        "qualified_name" => return cs_type(n.child_by_field_name("name").ok_or_else(unknown)?, src, cx, d),
        "array_type" => {
            if n.child_by_field_name("rank").is_some_and(|r| text(r).contains(',')) {
                return Err(unknown());
            }
            CsType::T(Type::List(Box::new(plain(n.child_by_field_name("type").ok_or_else(unknown)?)?)))
        }
        "identifier" => match text(n) {
            "Action" => CsType::Exit(Vec::new(), "args"),
            name => match d.delegates.iter().find(|(dn, _)| *dn == name) {
                Some((_, dl)) => {
                    let mut v = Vec::new();
                    if let Some(ps) = dl.child_by_field_name("parameters") {
                        let mut c = ps.walk();
                        for p in ps.named_children(&mut c).filter(|p| p.kind() == "parameter") {
                            let (Some(t), Some(pn)) = (p.child_by_field_name("type"), p.child_by_field_name("name"))
                            else {
                                return Err(unknown());
                            };
                            v.push((Some(text(pn).to_string()), plain(t).map_err(|e| format!("`{name}`: {e}"))?));
                        }
                    }
                    CsType::Exit(v, "args")
                }
                None => CsType::T(Type::Record(cx.record(name).ok_or_else(unknown)?)),
            },
        },
        "generic_name" => {
            let head = n.named_child(0).map(text).unwrap_or_default();
            let mut c = n.walk();
            let args: Vec<Node> = n
                .named_children(&mut c)
                .find(|c| c.kind() == "type_argument_list")
                .map(|l| {
                    let mut c = l.walk();
                    l.named_children(&mut c).collect()
                })
                .unwrap_or_default();
            match (head, args.as_slice()) {
                ("List" | "IList" | "IReadOnlyList" | "IEnumerable" | "ICollection" | "IReadOnlyCollection", [t]) => {
                    CsType::T(Type::List(Box::new(plain(*t)?)))
                }
                ("Dictionary" | "IDictionary" | "IReadOnlyDictionary", [k, v]) => {
                    if !matches!(plain(*k)?, Type::Str) {
                        return Err(format!("`{}`: a dictionary's keys are string here", text(n)));
                    }
                    CsType::T(Type::Map(Box::new(plain(*v)?)))
                }
                // one tuple, or one class of the code: its names are the values'
                ("Action", [one]) if one.kind() == "tuple_type" => {
                    let mut v = Vec::new();
                    for (name, t) in tuple_elements(*one, src) {
                        v.push((name, plain(t)?));
                    }
                    CsType::Exit(v, "tuple")
                }
                ("Action", [one]) if one.kind() == "identifier" && d.type_of(text(*one)).is_some() => {
                    let decl = d.type_of(text(*one)).expect("just found");
                    let mut v = Vec::new();
                    for (name, t) in members(decl, src) {
                        v.push((Some(name), plain(t).map_err(|e| format!("`{}`: {e}", text(*one)))?));
                    }
                    CsType::Exit(v, "fields")
                }
                ("Action", values) => {
                    let mut v = Vec::new();
                    for value in values {
                        v.push((None, plain(*value)?));
                    }
                    CsType::Exit(v, "args")
                }
                _ => return Err(unknown()),
            }
        }
        _ => return Err(unknown()),
    })
}

/// A default value as the text of a field: `2`, `1.5f` as `1.5`, a string
/// as it is, escapes, `@"verbatim"`, `"""raw"""` and `"a" + "b"` read.
fn default_text(n: Node, src: &str) -> Option<String> {
    let t = &src[n.byte_range()];
    Some(match n.kind() {
        "integer_literal" if !t.starts_with("0x") && !t.starts_with("0b") => {
            t.trim_end_matches(['l', 'L', 'u', 'U']).replace('_', "")
        }
        "real_literal" => t.trim_end_matches(['f', 'F', 'd', 'D', 'm', 'M']).replace('_', ""),
        "boolean_literal" => t.to_string(),
        "string_literal" => {
            // UTF-16, as C# says them: an escape can be half of a pair
            let mut units: Vec<u16> = Vec::new();
            let mut c = n.walk();
            for part in n.named_children(&mut c) {
                let p = &src[part.byte_range()];
                match part.kind() {
                    "string_literal_content" => units.extend(p.encode_utf16()),
                    "escape_sequence" => units.extend(escape(p)?),
                    _ => return None,
                }
            }
            String::from_utf16_lossy(&units)
        }
        "verbatim_string_literal" => t.strip_prefix("@\"")?.strip_suffix('"')?.replace("\"\"", "\""),
        "raw_string_literal" => {
            let mut c = n.walk();
            let content = n.named_children(&mut c).find(|c| c.kind() == "raw_string_content");
            let text = content.map_or("", |c| &src[c.byte_range()]);
            // more than one line has its indentation taken off: not read
            if text.contains('\n') {
                return None;
            }
            text.to_string()
        }
        "binary_expression" if n.child_by_field_name("operator").is_some_and(|o| &src[o.byte_range()] == "+") => {
            let (l, r) = (n.child_by_field_name("left")?, n.child_by_field_name("right")?);
            let is_str =
                |n: Node| matches!(n.kind(), "string_literal" | "verbatim_string_literal" | "raw_string_literal");
            if !is_str(l) || !is_str(r) {
                return None;
            }
            default_text(l, src)? + &default_text(r, src)?
        }
        "prefix_unary_expression" => t.replace(' ', ""),
        _ => return None,
    })
}

/// The UTF-16 unit or units of the escape E (`\n`, `\u00e9`, `\x41`).
fn escape(e: &str) -> Option<Vec<u16>> {
    let body = e.strip_prefix('\\')?;
    let mut chars = body.chars();
    let c = match chars.next()? {
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        '0' => '\0',
        'a' => '\x07',
        'b' => '\x08',
        'f' => '\x0c',
        'v' => '\x0b',
        c @ ('\\' | '\'' | '"') => c,
        'u' | 'U' | 'x' => {
            let code = u32::from_str_radix(chars.as_str(), 16).ok()?;
            // a lone surrogate (half of a pair) is a unit of its own
            return match char::from_u32(code) {
                Some(c) => Some(c.encode_utf16(&mut [0; 2]).to_vec()),
                None => u16::try_from(code).ok().map(|u| vec![u]),
            };
        }
        _ => return None,
    };
    Some(c.encode_utf16(&mut [0; 2]).to_vec())
}

/// The text of a doc comment's lines (`<summary>...`): the summary's if it
/// has one, else all of it.
fn doc_text(lines: &[String]) -> String {
    use quick_xml::events::Event;
    let squeeze = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let xml = format!("<doc>{}</doc>", lines.join("\n"));
    let mut reader = quick_xml::Reader::from_str(&xml);
    let (mut all, mut summary, mut inside, mut has) = (String::new(), String::new(), false, false);
    loop {
        let text = match reader.read_event() {
            Ok(Event::Start(e)) if e.name().as_ref() == b"summary" => {
                (inside, has) = (true, true);
                continue;
            }
            Ok(Event::End(e)) if e.name().as_ref() == b"summary" => {
                inside = false;
                continue;
            }
            Ok(Event::Text(t)) => t.decode().map(|t| t.into_owned()).unwrap_or_default(),
            // `&lt;`, `&#60;`
            Ok(Event::GeneralRef(r)) => match r.resolve_char_ref() {
                Ok(Some(c)) => c.to_string(),
                _ => r
                    .decode()
                    .ok()
                    .and_then(|n| quick_xml::escape::resolve_predefined_entity(&n))
                    .unwrap_or("")
                    .to_string(),
            },
            Ok(Event::Eof) => break,
            // not XML (a `<` that is text): as written
            Err(_) => return squeeze(&lines.join(" ")),
            Ok(_) => continue,
        };
        all.push_str(&text);
        if inside {
            summary.push_str(&text);
        }
    }
    squeeze(if has { &summary } else { &all })
}

/// Whether N has the modifier M (`public`, `static`...).
fn has_mod(n: Node, src: &str, m: &str) -> bool {
    let mut c = n.walk();
    n.children(&mut c).any(|c| c.kind() == "modifier" && &src[c.byte_range()] == m)
}

/// What a type of the code gives when it's returned: its public fields and
/// properties, in order, with their types (`record P(long a)`'s parameters
/// too).
fn members<'a>(t: Node<'a>, src: &str) -> Vec<(String, Node<'a>)> {
    let mut v = Vec::new();
    let mut c = t.walk();
    for list in t.children(&mut c).filter(|c| c.kind() == "parameter_list") {
        let mut c = list.walk();
        for p in list.named_children(&mut c).filter(|c| c.kind() == "parameter") {
            if let (Some(n), Some(ty)) = (p.child_by_field_name("name"), p.child_by_field_name("type")) {
                v.push((src[n.byte_range()].to_string(), ty));
            }
        }
    }
    let Some(body) = t.child_by_field_name("body") else { return v };
    let mut c = body.walk();
    for m in body.named_children(&mut c) {
        if !has_mod(m, src, "public") || has_mod(m, src, "static") {
            continue;
        }
        match m.kind() {
            "field_declaration" => {
                let mut c = m.walk();
                for d in m.named_children(&mut c).filter(|c| c.kind() == "variable_declaration") {
                    let Some(ty) = d.child_by_field_name("type") else { continue };
                    let mut c = d.walk();
                    for n in d.named_children(&mut c).filter(|c| c.kind() == "variable_declarator") {
                        if let Some(n) = n.child_by_field_name("name") {
                            v.push((src[n.byte_range()].to_string(), ty));
                        }
                    }
                }
            }
            "property_declaration" => {
                if let (Some(n), Some(ty)) = (m.child_by_field_name("name"), m.child_by_field_name("type")) {
                    v.push((src[n.byte_range()].to_string(), ty));
                }
            }
            _ => {}
        }
    }
    v
}

/// What the glue needs that a [`Function`] doesn't say.
#[derive(Debug, Default)]
struct Read {
    /// How what it returns is read: `none`, `one` (a value), `tuple` or
    /// `fields` (of a type of the code).
    mode: &'static str,
    /// Per exit: how the code gives its values (see [`CsType::Exit`]).
    shapes: Vec<(String, &'static str)>,
}

/// The function in SOURCE, and what the glue needs.
fn read(original: &str, cx: &dyn SigCx) -> Result<(Function, Read), String> {
    // `dotnet run`'s directives (`#:package X@1`) aren't C#: they're read as
    // comments (as long, so the code's places stay)
    let masked: String = original
        .split_inclusive('\n')
        .map(|l| if l.starts_with("#:") || l.starts_with("#!") { format!("//{}", &l[2..]) } else { l.to_string() })
        .collect();
    let source = masked.as_str();
    let tree = super::parse(source, tree_sitter_c_sharp::LANGUAGE.into(), "C#")?;
    let root = tree.root_node();
    let text = |n: Node| &source[n.byte_range()];
    let all = descendants(root);

    let mut d = Decls { delegates: Vec::new(), types: Vec::new() };
    for n in &all {
        let Some(name) = n.child_by_field_name("name").map(text) else { continue };
        match n.kind() {
            "delegate_declaration" => d.delegates.push((name, *n)),
            "class_declaration" | "struct_declaration" | "record_declaration" | "record_struct_declaration" => {
                d.types.push((name, *n))
            }
            _ => {}
        }
    }

    let found: Vec<Node> = all
        .iter()
        .filter(|n| {
            n.kind() == "method_declaration" && has_mod(**n, source, "public") && has_mod(**n, source, "static")
        })
        .copied()
        .collect();
    let f = match found.as_slice() {
        [f] => *f,
        [] => return Err("there's no function: a `public static` method (the others aren't `public`)".into()),
        fs => {
            let names: Vec<&str> = fs.iter().filter_map(|f| f.child_by_field_name("name")).map(text).collect();
            return Err(format!(
                "it runs one function, and {} are `public static`: the others aren't",
                names.join(", ")
            ));
        }
    };
    let name = text(f.child_by_field_name("name").ok_or("the function has no name")?).to_string();
    if has_mod(f, source, "async") {
        return Err(format!("`{name}` is `async`: it returns its value, not a Task"));
    }

    // its doc: the `///` lines right before it
    let mut doc = Vec::new();
    let mut prev = f.prev_named_sibling();
    while let Some(p) = prev.filter(|p| p.kind() == "comment" && text(*p).starts_with("///")) {
        doc.push(text(p).trim_start_matches('/').trim().to_string());
        prev = p.prev_named_sibling();
    }
    doc.reverse();
    let doc = if doc.is_empty() { String::new() } else { doc_text(&doc) };

    // its parameters
    let mut params = Vec::new();
    let mut shapes = Vec::new();
    if let Some(ps) = f.child_by_field_name("parameters") {
        let mut c = ps.walk();
        for p in ps.named_children(&mut c).filter(|p| p.kind() == "parameter") {
            let pname_node = p.child_by_field_name("name").ok_or("a parameter with no name")?;
            let pname = text(pname_node).to_string();
            let mut c = p.walk();
            if p.children(&mut c).any(|m| m.kind() == "modifier") {
                return Err(format!(
                    "`{pname}`: a parameter is a plain one (no `ref`, `out`, `in`, `params` or `this`)"
                ));
            }
            let ty = p.child_by_field_name("type").ok_or_else(|| format!("`{pname}` has no type"))?;
            let role = match cs_type(ty, source, cx, &d).map_err(|e| format!("`{pname}`: {e}"))? {
                CsType::T(t) => Role::Input(Some(t)),
                CsType::Exit(v, shape) => {
                    shapes.push((pname.clone(), shape));
                    Role::Exit(exit_values(&pname, v))
                }
            };
            // its default comes after its name
            let mut c = p.walk();
            let default = p.named_children(&mut c).skip_while(|c| *c != pname_node).nth(1);
            let default = default.and_then(|v| default_text(v, source));
            params.push(Param { name: pname, role, default });
        }
    }

    // what it returns
    let (returns, mode) = match f.child_by_field_name("returns") {
        None => (Vec::new(), "none"),
        Some(r) if text(r) == "void" => (Vec::new(), "none"),
        Some(r) if r.kind() == "tuple_type" => {
            let mut v = Vec::new();
            for (n, ty) in tuple_elements(r, source) {
                match cs_type(ty, source, cx, &d)? {
                    CsType::T(t) => v.push((n, t)),
                    CsType::Exit(..) => return Err("it returns values, not exits".into()),
                }
            }
            (return_values(v), "tuple")
        }
        Some(r) if r.kind() == "identifier" && d.type_of(text(r)).is_some() => {
            let decl = d.type_of(text(r)).expect("just found");
            let mut v = Vec::new();
            for (n, ty) in members(decl, source) {
                match cs_type(ty, source, cx, &d).map_err(|e| format!("`{}.{n}`: {e}", text(r)))? {
                    CsType::T(t) => v.push((Some(n), t)),
                    CsType::Exit(..) => return Err("it returns values, not exits".into()),
                }
            }
            if v.is_empty() {
                return Err(format!("`{}` has no public fields or properties to return", text(r)));
            }
            (return_values(v), "fields")
        }
        Some(r) => match cs_type(r, source, cx, &d)? {
            CsType::T(t) => (return_values(vec![(None, t)]), "one"),
            CsType::Exit(..) => return Err("it returns values, not exits".into()),
        },
    };

    // its state: the statics of the type it's in
    let mut state = Vec::new();
    let owner = std::iter::successors(f.parent(), |p| p.parent()).find(|p| {
        matches!(
            p.kind(),
            "class_declaration" | "struct_declaration" | "record_declaration" | "record_struct_declaration"
        )
    });
    if let Some(body) = owner.and_then(|c| c.child_by_field_name("body")) {
        let mut c = body.walk();
        for m in body.named_children(&mut c).filter(|m| m.kind() == "field_declaration") {
            if !has_mod(m, source, "static") || has_mod(m, source, "const") || has_mod(m, source, "readonly") {
                continue;
            }
            let mut c = m.walk();
            for v in m.named_children(&mut c).filter(|c| c.kind() == "variable_declaration") {
                // only those whose type lineit knows
                let known = v
                    .child_by_field_name("type")
                    .is_some_and(|t| matches!(cs_type(t, source, cx, &d), Ok(CsType::T(_))));
                let mut c = v.walk();
                for n in v.named_children(&mut c).filter(|c| c.kind() == "variable_declarator") {
                    if let (true, Some(n)) = (known, n.child_by_field_name("name")) {
                        state.push(text(n).to_string());
                    }
                }
            }
        }
    }

    // the variables the code uses, in its helpers and initializers too
    let mut shared: Vec<(String, bool)> = Vec::new();
    for n in all.iter().copied() {
        if n.kind() != "member_access_expression" {
            continue;
        }
        if let (Some(o), Some(a)) = (n.child_by_field_name("expression"), n.child_by_field_name("name"))
            && text(o) == "Shared"
        {
            let set = n.parent().is_some_and(|p| match p.kind() {
                "assignment_expression" => p.child_by_field_name("left") == Some(n),
                "postfix_unary_expression" | "prefix_unary_expression" => {
                    let t = text(p);
                    t.starts_with("++") || t.starts_with("--") || t.ends_with("++") || t.ends_with("--")
                }
                _ => false,
            });
            let g = text(a).to_string();
            match shared.iter_mut().find(|(s, _)| *s == g) {
                Some(s) => s.1 |= set,
                None => shared.push((g, set)),
            }
        }
    }

    let function = Function {
        name,
        doc,
        params,
        returns,
        state,
        shared,
        shared_types: Vec::new(),
        source: original.to_string(),
        file: None,
    };
    Ok((function, Read { mode, shapes }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lineit_rt::ext::{Ext, Run};
    use lineit_rt::{RecordType, Value};
    use std::sync::Arc;

    struct Cx;
    impl SigCx for Cx {
        fn var(&self, name: &str) -> Option<Type> {
            (name == "::total").then_some(Type::Int)
        }
        fn record(&self, _: &str) -> Option<Arc<RecordType>> {
            None
        }
        fn read(&self, path: &str) -> Result<String, String> {
            Err(format!("{path}: no files here"))
        }
    }

    const COLLATZ: &str = r#"using System.Text;

delegate void Odd(long calls, long n);

static class Collatz
{
    static long count = 0;
    const long LIMIT = 3;

    /// <summary>Halves even numbers;
    /// counts the odd ones.</summary>
    public static void Run(long n, Action<long> even, Odd odd)
    {
        Shared.total += n;
        Console.WriteLine($"got {n}");
        if (n % 2 == 0) { even(n / 2); return; }
        count++;
        odd(count, n);
    }
}
"#;

    #[test]
    fn reads() {
        let (f, _) = read(COLLATZ, &Cx).unwrap();
        assert_eq!(f.name, "Run");
        assert_eq!(f.doc, "Halves even numbers; counts the odd ones.");
        assert_eq!(f.params[1].role, Role::Exit(vec![("even.value".into(), Type::Int)]));
        assert_eq!(f.params[2].role, Role::Exit(vec![("calls".into(), Type::Int), ("n".into(), Type::Int)]));
        assert_eq!(f.state, ["count"]);
        assert_eq!(f.shared, [("total".to_string(), true)]);
        assert!(f.ticked());

        let src = "record R(long sq, string text);\nstatic class A {\n  static string last; static readonly int K = 1;\n  static int H(int x) { return x; }\n  public static R F(int x, double k = -2.5, string s = \"hi\", bool b = true, long? n = 3L, List<string> xs = null, Dictionary<string, int> m = null, float[] arr = null) { var v = Shared.z; Shared.q++; return new R(x * x, \"ok\"); }\n}\n";
        let (f, r) = read(src, &Cx).unwrap();
        assert_eq!(r.mode, "fields");
        assert_eq!(f.returns, [("sq".to_string(), Type::Int), ("text".to_string(), Type::Str)]);
        let defaults: Vec<_> = f.params.iter().map(|p| p.default.as_deref()).collect();
        assert_eq!(defaults, [None, Some("-2.5"), Some("hi"), Some("true"), Some("3"), None, None, None]);
        assert_eq!(f.params[5].role, Role::Input(Some(Type::List(Box::new(Type::Str)))));
        assert_eq!(f.params[6].role, Role::Input(Some(Type::Map(Box::new(Type::Int)))));
        assert_eq!(f.params[7].role, Role::Input(Some(Type::List(Box::new(Type::Float)))));
        assert_eq!(f.state, ["last"]);
        assert_eq!(f.shared, [("z".to_string(), false), ("q".to_string(), true)]);

        let (f, r) =
            read("static class A { public static (long, string name) F() { return (1, \"a\"); } }", &Cx).unwrap();
        assert_eq!(r.mode, "tuple");
        assert_eq!(f.returns, [("out1".to_string(), Type::Int), ("name".to_string(), Type::Str)]);
        let (f, r) = read("static class A { public static string F(string s) => s; }", &Cx).unwrap();
        assert_eq!((r.mode, f.returns), ("one", vec![("result".to_string(), Type::Str)]));
        assert_eq!(read(CSharp.template(), &Cx).unwrap().0.doc, "Twice x.");
    }

    #[test]
    fn errors() {
        let err = |s: &str| read(s, &Cx).unwrap_err();
        assert!(err("class A { }").contains("there's no function"));
        assert!(
            err("class A { public static void F() {} public static void G() {} }").contains("F, G are `public static`")
        );
        assert!(err("class A { public static void F(Foo x) {} }").contains("`Foo` isn't a type"));
        assert!(err("class A { public static void F(ref int x) {} }").contains("a plain one"));
        assert!(err("class A { public static async void F() {} }").contains("`async`"));
        assert!(err("class A { public static int F( }").contains("line 1"));
        assert!(err("class A { public static void F(Dictionary<int, int> m) {} }").contains("keys are string"));
    }

    /// The module of F, started: None without .NET here.
    fn start(f: &Function) -> Option<Box<dyn Ext>> {
        if !sdk_ready() {
            eprintln!("skipped: no .NET {MIN_SDK} SDK");
            return None;
        }
        Some(crate::interp::open_ext(&f.name, &CSharp.ext(f).unwrap()).unwrap_or_else(|e| panic!("{e}")))
    }

    #[test]
    fn runs() {
        let (mut f, _) = read(COLLATZ, &Cx).unwrap();
        f.shared_types = vec![Type::Int];
        let Some(mut m) = start(&f) else { return };
        let (a, b) = (m.instance(&[]).unwrap(), m.instance(&[]).unwrap());
        let outs = [("even.value", &Type::Int), ("calls", &Type::Int), ("n", &Type::Int), ("::total", &Type::Int)];
        let call = |m: &mut Box<dyn Ext>, sq, n| {
            m.run(sq, Some("tick"), &[("n", Value::Int(n)), ("::total", Value::Int(100))], &outs).unwrap()
        };
        assert_eq!(
            call(&mut m, a, 4),
            Run {
                set: vec![None, None, None, Some(Value::Int(104))],
                fire: vec![("even".into(), vec![(0, Value::Int(2))])]
            }
        );
        call(&mut m, a, 3);
        assert_eq!(call(&mut m, a, 5).fire, [("odd".into(), vec![(1, Value::Int(2)), (2, Value::Int(5))])]);
        assert_eq!(m.state(a).unwrap(), [("count".to_string(), Value::Int(2))]);
        // each square has its own statics
        m.set_state(b, &[("count", Value::Int(41))]).unwrap();
        assert_eq!(call(&mut m, b, 1).fire[0].1[0], (1, Value::Int(42)));
        assert_eq!(m.state(a).unwrap(), [("count".to_string(), Value::Int(2))]);

        // errors are on the code's lines
        let (f, _) =
            read("static class A\n{\n    public static long F(long x)\n    {\n        var a = new long[1];\n        return a[x + 5];\n    }\n}\n", &Cx)
                .unwrap();
        let mut m = start(&f).unwrap();
        let sq = m.instance(&[]).unwrap();
        let e = m.eval(sq, &[("x", Value::Int(1))], &[("result", &Type::Int)]).unwrap_err();
        assert!(e.starts_with("F.cs:6: "), "{e}");

        // code .NET doesn't build: its error
        let (mut bad, _) = read("static class A { public static long F(long x) { return x; } }", &Cx).unwrap();
        bad.source = "static class A { public static long F(long x) { return y; } }".into();
        let e = crate::interp::open_ext("F", &CSharp.ext(&bad).unwrap()).err().unwrap();
        assert!(e.contains("error CS0103"), "{e}");
    }

    #[test]
    fn reads_more() {
        // exits that give a tuple, or an object of the code; the variables
        // of the helpers and the initializers; statics of a struct, with
        // several in a declaration
        let src = "class Pt { public long x; public string Name { get; set; } }
struct S
{
    static long a = Shared.start, b = 2;
    static string c;
    static long Help() { return Shared.help; }
    public static void F(Action<(long half, string tag)> t, Action<Pt> o, Action<(int, int)> u) { }
}
";
        let (f, r) = read(src, &Cx).unwrap();
        assert_eq!(f.params[0].role, Role::Exit(vec![("half".into(), Type::Int), ("tag".into(), Type::Str)]));
        assert_eq!(f.params[1].role, Role::Exit(vec![("x".into(), Type::Int), ("Name".into(), Type::Str)]));
        assert_eq!(f.params[2].role, Role::Exit(vec![("u.1".into(), Type::Int), ("u.2".into(), Type::Int)]));
        assert_eq!(r.shapes, [("t".to_string(), "tuple"), ("o".to_string(), "fields"), ("u".to_string(), "tuple")]);
        assert_eq!(f.state, ["a", "b", "c"]);
        assert_eq!(f.shared, [("start".to_string(), false), ("help".to_string(), false)]);
    }

    #[test]
    fn defaults() {
        let src = r#"class A { public static void F(string a = "x\n\u00e9\"y", string b = @"a""b", string c = """raw "q" """, string d = "p" + "q", string e = $"i{1}", string g = "\uD83D\uDE00!", int h = 0x10) {} }"#;
        let (f, _) = read(src, &Cx).unwrap();
        let d: Vec<_> = f.params.iter().map(|p| p.default.as_deref()).collect();
        assert_eq!(
            d,
            [Some("x\n\u{e9}\"y"), Some("a\"b"), Some("raw \"q\" "), Some("pq"), None, Some("\u{1F600}!"), None]
        );
    }

    #[test]
    fn runs_more() {
        let src = "class Pt { public long x; public string Name { get; set; } }
static class A
{
    public static (long, string) Tup(long n, Action<(long half, string tag)> t, Action<Pt> o)
    {
        t((n / 2, \"t\"));
        o(new Pt { x = n, Name = \"p\" });
        return (1, \"a\");
    }
}
";
        let (f, _) = read(src, &Cx).unwrap();
        let Some(mut m) = start(&f) else { return };
        let sq = m.instance(&[]).unwrap();
        let outs =
            [("half", &Type::Int), ("tag", &Type::Str), ("x", &Type::Int), ("Name", &Type::Str), ("out1", &Type::Int)];
        let run = m.run(sq, None, &[("n", Value::Int(8))], &outs).unwrap();
        assert_eq!(
            run.fire,
            [
                ("t".into(), vec![(0, Value::Int(4)), (1, Value::Str("t".into()))]),
                ("o".into(), vec![(2, Value::Int(8)), (3, Value::Str("p".into()))]),
            ]
        );
        drop(m);

        // a null where an object should be, and NaN, say what they are
        let class = "class Pt { public long x; }\n";
        let (f, _) =
            read(&format!("{class}static class B {{ public static Pt Make(long n) {{ return null; }} }}"), &Cx)
                .unwrap();
        let mut m = start(&f).unwrap();
        let sq = m.instance(&[]).unwrap();
        let e = m.eval(sq, &[("n", Value::Int(0))], &[("x", &Type::Int)]).unwrap_err();
        assert!(e.contains("`x`: it gave null"), "{e}");
        let (f, _) = read("static class C { public static double Nan(long n) { return 0.0 / n; } }", &Cx).unwrap();
        let mut m = start(&f).unwrap();
        let sq = m.instance(&[]).unwrap();
        let e = m.eval(sq, &[("n", Value::Int(0))], &[("result", &Type::Float)]).unwrap_err();
        assert!(e.contains("NaN and Infinity"), "{e}");
        // and the square goes on
        assert_eq!(m.eval(sq, &[("n", Value::Int(2))], &[("result", &Type::Float)]).unwrap(), [Value::Float(0.0)]);
    }

    /// A NuGet package in the square (the first line, `#:package`): MessagePack
    /// is what C# squares are for. Skipped when it can't be had (no network).
    #[test]
    fn packages() {
        let src = "#:package MessagePack@3.1.4
using MessagePack;

[MessagePackObject]
public class P { [Key(0)] public long A { get; set; } }

static class M
{
    public static long Size(long a)
    {
        return MessagePackSerializer.Serialize(new P { A = a }).Length;
    }
}
";
        let (f, _) = read(src, &Cx).unwrap();
        if !sdk_ready() {
            return;
        }
        let kind = CSharp.ext(&f).unwrap();
        let mut m = match crate::interp::open_ext("Size", &kind) {
            Ok(m) => m,
            // restoring it needs the network
            Err(e) if e.contains("NU1301") || e.contains("NU1101") || e.contains("service index") => {
                eprintln!("skipped: no NuGet here ({e})");
                return;
            }
            Err(e) => panic!("{e}"),
        };
        let sq = m.instance(&[]).unwrap();
        // an array of one (1 byte), and 5 (1)
        assert_eq!(m.eval(sq, &[("a", Value::Int(5))], &[("result", &Type::Int)]).unwrap(), [Value::Int(2)]);
    }
}

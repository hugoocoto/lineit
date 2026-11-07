//! Packages: modules to share, in a `.lipkg` file (a zip) of a folder like
//!
//! ```text
//! mypackage/
//!     package.lua     return { name = "mypackage", version = "0.1.0", description = "...", ... }
//!     modules/        Lua modules, subgraphs (.graph.lua), record types (.type.lua),
//!                     and the pins of Rust modules (.native.lua)
//!     native/         optional: the cargo crate of the Rust modules
//! ```
//!
//! `lineit new-package NAME` makes one to fill in, `lineit pack DIR` zips
//! it, and `lineit install FILE` puts it where lineit finds it
//! ([`crate::paths::packages_dir`]). Its modules are `NAME/...` in the
//! search. Everything in it is source, so it works on every OS as it is;
//! Rust modules are built on the machine that runs them, by cargo. A
//! package is code: only install the ones you trust.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::registry::Registry;

pub const EXT: &str = "lipkg";

#[derive(Clone, Debug, PartialEq)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub description: String,
}

pub fn read_manifest(source: &str) -> Result<Manifest, String> {
    crate::file::with_data(source, "package.lua", |t| {
        let get = |k: &str| t.get::<Option<String>>(k).map_err(|_| format!("`{k}` should be a string"));
        let name = get("name")?.ok_or("it needs a `name`")?;
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err(format!("`{name}`: a package's name is letters, digits, - and _"));
        }
        Ok(Manifest {
            name,
            version: get("version")?.unwrap_or_else(|| "0.1.0".into()),
            description: get("description")?.unwrap_or_default(),
        })
    })
}

/// The `[package] name` of the crate in DIR.
pub fn crate_name(dir: &Path) -> Result<String, String> {
    let p = dir.join("Cargo.toml");
    let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    let mut in_package = false;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_package = l == "[package]";
        } else if in_package && let Some(v) = l.strip_prefix("name").map(str::trim).and_then(|r| r.strip_prefix('=')) {
            return Ok(v.trim().trim_matches('"').to_string());
        }
    }
    Err(format!("{}: no name in [package]", p.display()))
}

/// The files of a package folder, relative, with `/`: what goes in it.
fn files(dir: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    fn walk(dir: &Path, rel: &str, out: &mut Vec<(String, PathBuf)>) -> Result<(), String> {
        let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            // hidden files, and what cargo builds
            if name.starts_with('.') || name == "target" || name == "Cargo.lock" && rel.is_empty() {
                continue;
            }
            let r = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
            let p = e.path();
            if p.is_dir() {
                walk(&p, &r, out)?;
            } else {
                out.push((r, p));
            }
        }
        Ok(())
    }
    let mut v = Vec::new();
    walk(dir, "", &mut v)?;
    v.sort();
    Ok(v)
}

/// Zips the package in DIR into OUT (`NAME-VERSION.lipkg` next to DIR if
/// None), after checking its manifest and that its modules load.
pub fn pack(dir: &Path, out: Option<&Path>) -> Result<PathBuf, String> {
    let src = std::fs::read_to_string(dir.join("package.lua"))
        .map_err(|e| format!("{}: {e} (a package needs a package.lua)", dir.join("package.lua").display()))?;
    let m = read_manifest(&src)?;
    let mut reg = Registry::default();
    reg.load_dir(&dir.join("modules"), &format!("{}/", m.name));
    if !reg.errors.is_empty() {
        return Err(format!("some modules don't load:\n  {}", reg.errors.join("\n  ")));
    }
    if reg.modules.is_empty() && reg.records.is_empty() {
        return Err(format!("{} has no modules", dir.join("modules").display()));
    }
    let out = match out {
        Some(o) => o.to_path_buf(),
        None => dir.parent().unwrap_or(Path::new(".")).join(format!("{}-{}.{EXT}", m.name, m.version)),
    };
    let tmp = out.with_extension("tmp");
    let file = std::fs::File::create(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);
    let zerr = |e: zip::result::ZipError| e.to_string();
    for (rel, path) in files(dir)? {
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        zip.start_file(&rel, opts).map_err(zerr)?;
        zip.write_all(&bytes).map_err(|e| e.to_string())?;
    }
    zip.finish().map_err(zerr)?;
    std::fs::rename(&tmp, &out).map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(out)
}

/// Reads FILE's manifest, checking it's a package.
fn manifest_of(file: &Path) -> Result<Manifest, String> {
    let f = std::fs::File::open(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let mut zip = zip::ZipArchive::new(f).map_err(|e| format!("{}: not a package ({e})", file.display()))?;
    let mut m = zip.by_name("package.lua").map_err(|_| format!("{}: no package.lua in it", file.display()))?;
    let mut src = String::new();
    m.read_to_string(&mut src).map_err(|e| format!("{}: {e}", file.display()))?;
    read_manifest(&src)
}

/// Unzips FILE into DIR, if it isn't there yet.
fn extract(file: &Path, dir: &Path) -> Result<(), String> {
    if dir.join("package.lua").exists() {
        return Ok(());
    }
    let f = std::fs::File::open(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let mut zip = zip::ZipArchive::new(f).map_err(|e| format!("{}: {e}", file.display()))?;
    let tmp = dir.with_extension("tmp");
    let _ = std::fs::remove_dir_all(&tmp);
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| format!("{}: {e}", file.display()))?;
        // a name that would go outside the folder isn't extracted
        let Some(rel) = entry.enclosed_name() else { continue };
        let to = tmp.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&to).map_err(|e| format!("{}: {e}", to.display()))?;
            continue;
        }
        if let Some(p) = to.parent() {
            std::fs::create_dir_all(p).map_err(|e| format!("{}: {e}", p.display()))?;
        }
        let mut bytes = Vec::new();
        // the zip checks each file against its CRC as it's read
        entry.read_to_end(&mut bytes).map_err(|e| format!("{}: {e}", file.display()))?;
        std::fs::write(&to, bytes).map_err(|e| format!("{}: {e}", to.display()))?;
    }
    let _ = std::fs::remove_dir_all(dir);
    std::fs::rename(&tmp, dir).map_err(|e| format!("{}: {e}", dir.display()))
}

/// Puts FILE in the packages folder, replacing an older one of the same
/// package.
pub fn install(file: &Path) -> Result<PathBuf, String> {
    let m = manifest_of(file)?;
    let dir = crate::paths::packages_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == EXT) && manifest_of(&p).is_ok_and(|o| o.name == m.name) {
                let _ = std::fs::remove_file(&p);
            }
        }
    }
    let to = dir.join(format!("{}-{}.{EXT}", m.name, m.version));
    std::fs::copy(file, &to).map_err(|e| format!("{}: {e}", to.display()))?;
    Ok(to)
}

/// Loads the packages in [`crate::paths::packages_dir`]: `.lipkg` files,
/// and folders with a `package.lua` (a package being made).
pub fn load_installed(r: &mut Registry) {
    let dir = crate::paths::packages_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else { return };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for p in paths {
        let loaded = if p.is_dir() {
            std::fs::read_to_string(p.join("package.lua"))
                .map_err(|e| e.to_string())
                .and_then(|s| read_manifest(&s))
                .map(|m| (m, p.clone()))
        } else if p.extension().is_some_and(|x| x == EXT) {
            manifest_of(&p).and_then(|m| {
                // unzipped once per version of the file
                let bytes = std::fs::read(&p).map_err(|e| e.to_string())?;
                let hash = bytes.iter().fold(0xcbf29ce484222325u64, |h, b| (h ^ *b as u64).wrapping_mul(0x100000001b3));
                let to =
                    crate::paths::cache_dir().join("packages").join(format!("{}-{}-{hash:016x}", m.name, m.version));
                extract(&p, &to)?;
                Ok((m, to))
            })
        } else {
            continue;
        };
        match loaded {
            Ok((m, at)) => r.load_dir(&at.join("modules"), &format!("{}/", m.name)),
            Err(e) => r.errors.push(format!("{}: {e}", p.display())),
        }
    }
}

const SCAFFOLD_LUA: &str = r#"-- A Lua module. Its pins are inputs and outputs: { "name", "type" },
-- with a default for a field; types are int, float, bool, str, list<T>,
-- map<T>, a record type, or tick. A pure module has eval(i), which gets
-- the inputs by name and returns the outputs. One with tick pins has
-- tick(self, input, i) instead, which returns the outputs it sets and the
-- tick outputs to fire: see the README.
return {
        doc = "Says hello to someone",
        inputs = { { "name", "str", default = "world" } },
        outputs = { { "greeting", "str" } },
        eval = function(i)
                return { greeting = "Hello, " .. i.name .. "!" }
        end,
}
"#;

const SCAFFOLD_NATIVE_SPEC: &str = r#"-- The pins of a Rust module: `native` is its type in native/src/lib.rs.
-- `eval = true` for a pure one; one with tick inputs is ticked.
return {
        native = "Reverse",
        doc = "The text, backwards (in Rust)",
        inputs = { { "text", "str", default = "lineit" } },
        outputs = { { "result", "str" } },
        eval = true,
}
"#;

const SCAFFOLD_LIB: &str = r#"//! The Rust modules of this package. Each is a type that implements
//! `Native`, listed in `export_modules!` at the end; its pins are in
//! ../modules/NAME.native.lua.

use lineit_rt::native::{Native, Values};
use lineit_rt::Value;

pub struct Reverse;

impl Native for Reverse {
    fn new(_params: &Values) -> Self {
        Reverse
    }

    fn eval(&mut self, inputs: &Values) -> Result<Values, String> {
        let text: String = inputs["text"].str().chars().rev().collect();
        Ok(Values::from([("result".to_string(), Value::Str(text.into()))]))
    }
}

lineit_rt::export_modules!(Reverse);
"#;

/// Makes the folder NAME for a new package, with a Lua module, and a Rust
/// one too if NATIVE.
pub fn scaffold(dir: &Path, native: bool) -> Result<PathBuf, String> {
    let name = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    read_manifest(&format!("return {{ name = {} }}", crate::file::quote(&name)))?;
    if dir.exists() {
        return Err(format!("{} is already there", dir.display()));
    }
    let w = |rel: &str, text: &str| -> Result<(), String> {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| format!("{}: {e}", p.display()))?;
        std::fs::write(&p, text).map_err(|e| format!("{}: {e}", p.display()))
    };
    w(
        "package.lua",
        &format!(
            "-- lineit package: `lineit pack .` here makes {name}-0.1.0.lipkg\nreturn {{\n        name = {},\n        version = \"0.1.0\",\n        description = \"What it's for\",\n        author = \"\",\n        license = \"\",\n}}\n",
            crate::file::quote(&name)
        ),
    )?;
    w("modules/hello.lua", SCAFFOLD_LUA)?;
    w(
        "README.md",
        &format!(
            "# {name}\n\nA lineit package. Its modules are in `modules/`: Lua modules (`.lua`), subgraphs\n(`.graph.lua`, canvases with `graph/input` and `graph/output` squares), record\ntypes (`.type.lua`){}.\n\n    lineit pack .              # makes {name}-0.1.0.lipkg\n    lineit install {name}-0.1.0.lipkg\n\nWhile you make it, put this folder (or a link to it) in lineit's packages\nfolder and the editor loads it as it is, and again when it changes.\n",
            if native { " and the pins of Rust modules (`.native.lua`, the code is in `native/`)" } else { "" }
        ),
    )?;
    if native {
        let crate_name = format!("{}-native", name.to_lowercase().replace('_', "-"));
        w(
            "native/Cargo.toml",
            &format!(
                "[package]\nname = \"{crate_name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\ncrate-type = [\"cdylib\", \"rlib\"]\n\n[dependencies]\n# lineit builds it with the lineit-rt it comes with\nlineit-rt = \"0.1\"\n"
            ),
        )?;
        w("native/src/lib.rs", SCAFFOLD_LIB)?;
        w("modules/reverse.native.lua", SCAFFOLD_NATIVE_SPEC)?;
    }
    Ok(dir.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaffold_pack_load() {
        let root = std::env::temp_dir().join(format!("lineit-pkg-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let dir = scaffold(&root.join("greet"), false).unwrap();
        let file = pack(&dir, None).unwrap();
        assert_eq!(file.file_name().unwrap(), "greet-0.1.0.lipkg");
        assert_eq!(manifest_of(&file).unwrap().name, "greet");
        let out = root.join("x");
        extract(&file, &out).unwrap();
        let mut r = Registry::default();
        r.load_dir(&out.join("modules"), "greet/");
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert!(r.get("greet/hello").is_some());
        assert!(scaffold(&root.join("bad name"), false).is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }
}

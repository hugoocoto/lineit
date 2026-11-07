//! Fonts installed, by name, with no library: the folders where each OS
//! keeps them, and the names inside each font file (its `name` table), read
//! with a few small reads instead of the whole file.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// A face of a font: its family names (a font can have a few: "Iosevka
/// NFM" and "Iosevka Nerd Font Mono"), its full name ("Iosevka NFM Bold"),
/// its style ("Regular", "Bold"...) and its file.
#[derive(Clone, Debug)]
pub struct Face {
    pub families: Vec<String>,
    pub full: String,
    pub style: String,
    pub path: PathBuf,
}

fn dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
    let mut v = Vec::new();
    if cfg!(windows) {
        let win = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:\\Windows"));
        v.push(win.join("Fonts"));
        if let Some(l) = std::env::var_os("LOCALAPPDATA") {
            v.push(PathBuf::from(l).join("Microsoft\\Windows\\Fonts"));
        }
    } else if cfg!(target_os = "macos") {
        v.push(PathBuf::from("/System/Library/Fonts"));
        v.push(PathBuf::from("/Library/Fonts"));
        if let Some(h) = &home {
            v.push(h.join("Library/Fonts"));
        }
    } else {
        v.push(PathBuf::from("/usr/share/fonts"));
        v.push(PathBuf::from("/usr/local/share/fonts"));
        if let Some(d) = std::env::var_os("XDG_DATA_HOME") {
            v.push(PathBuf::from(d).join("fonts"));
        }
        if let Some(h) = &home {
            v.push(h.join(".local/share/fonts"));
            v.push(h.join(".fonts"));
        }
    }
    v
}

fn u16_at(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*b.get(i)?, *b.get(i + 1)?]))
}

fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_be_bytes([*b.get(i)?, *b.get(i + 1)?, *b.get(i + 2)?, *b.get(i + 3)?]))
}

fn read_at(f: &mut File, at: u64, len: usize) -> Option<Vec<u8>> {
    f.seek(SeekFrom::Start(at)).ok()?;
    let mut b = vec![0; len];
    f.read_exact(&mut b).ok()?;
    Some(b)
}

/// The names of the font at OFFSET of F (a whole .ttf, or one of a .ttc):
/// its family names and its style.
fn names(f: &mut File, offset: u64) -> Option<(Vec<String>, String, String)> {
    let head = read_at(f, offset, 12)?;
    let tables = u16_at(&head, 4)? as usize;
    let dir = read_at(f, offset + 12, tables * 16)?;
    let (mut at, mut len) = (0u64, 0usize);
    for t in 0..tables {
        if &dir[t * 16..t * 16 + 4] == b"name" {
            at = u32_at(&dir, t * 16 + 8)? as u64;
            len = u32_at(&dir, t * 16 + 12)? as usize;
        }
    }
    if len == 0 || len > 1 << 20 {
        return None;
    }
    let name = read_at(f, at, len)?;
    let count = u16_at(&name, 2)? as usize;
    let strings = u16_at(&name, 4)? as usize;
    let (mut families, mut style, mut typo_style, mut full) = (Vec::new(), String::new(), String::new(), String::new());
    for r in 0..count {
        let rec = 6 + r * 12;
        let (platform, id) = (u16_at(&name, rec)?, u16_at(&name, rec + 6)?);
        let (slen, soff) = (u16_at(&name, rec + 8)? as usize, u16_at(&name, rec + 10)? as usize);
        let Some(raw) = name.get(strings + soff..strings + soff + slen) else { continue };
        let text = match platform {
            // Windows and Unicode: UTF-16, big end first
            0 | 3 => String::from_utf16_lossy(
                &raw.as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes(*c)).collect::<Vec<_>>(),
            ),
            1 => raw.iter().map(|b| *b as char).collect(),
            _ => continue,
        };
        match id {
            1 | 16 => {
                if !text.is_empty() && !families.contains(&text) {
                    families.push(text);
                }
            }
            4 => full = text,
            2 => style = text,
            17 => typo_style = text,
            _ => {}
        }
    }
    if !typo_style.is_empty() {
        style = typo_style;
    }
    (!families.is_empty()).then_some((families, style, full))
}

fn faces_of(path: &Path, out: &mut Vec<Face>) {
    let Ok(mut f) = File::open(path) else { return };
    let Some(head) = read_at(&mut f, 0, 12) else { return };
    let offsets: Vec<u64> = if &head[0..4] == b"ttcf" {
        let n = u32_at(&head, 8).unwrap_or(0).min(64) as usize;
        let Some(table) = read_at(&mut f, 12, n * 4) else { return };
        (0..n).filter_map(|i| u32_at(&table, i * 4).map(|o| o as u64)).collect()
    } else {
        vec![0]
    };
    for o in offsets {
        if let Some((families, style, full)) = names(&mut f, o) {
            out.push(Face { families, full, style, path: path.to_path_buf() });
        }
    }
}

fn scan(dir: &Path, depth: u32, out: &mut Vec<Face>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            if depth < 6 {
                scan(&p, depth + 1, out);
            }
        } else if p
            .extension()
            .is_some_and(|x| matches!(x.to_string_lossy().to_lowercase().as_str(), "ttf" | "otf" | "ttc"))
        {
            faces_of(&p, out);
        }
    }
}

/// Every face installed. Looked for once (a moment, with many fonts), then
/// kept.
pub fn installed() -> &'static [Face] {
    static FACES: OnceLock<Vec<Face>> = OnceLock::new();
    FACES.get_or_init(|| {
        let mut v = Vec::new();
        for d in dirs() {
            scan(&d, 0, &mut v);
        }
        v
    })
}

/// The families installed, sorted, each once.
pub fn families() -> Vec<String> {
    let mut v: Vec<String> = installed().iter().flat_map(|f| f.families.iter().cloned()).collect();
    v.sort_by_key(|s| s.to_lowercase());
    v.dedup();
    v
}

/// The file of the font NAME: a path to one, the name of a family installed
/// (in any case: its Regular face if it has one), or the full name of a face.
pub fn find(name: &str) -> Option<PathBuf> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let p = Path::new(name);
    if p.is_file() {
        return Some(p.to_path_buf());
    }
    let want = name.to_lowercase();
    let matching: Vec<&Face> =
        installed().iter().filter(|f| f.families.iter().any(|n| n.to_lowercase() == want)).collect();
    if matching.is_empty() {
        return installed().iter().find(|f| f.full.to_lowercase() == want).map(|f| f.path.clone());
    }
    let rank = |s: &str| match s.to_lowercase().as_str() {
        "regular" => 0,
        "book" | "normal" | "roman" => 1,
        "medium" => 2,
        _ => 3,
    };
    matching.into_iter().min_by_key(|f| (rank(&f.style), f.style.len())).map(|f| f.path.clone())
}

#[cfg(test)]
mod tests {
    #[test]
    fn finds_by_name() {
        // fonts differ on every machine: this only checks what's there
        for f in super::installed().iter().take(50) {
            let found = super::find(&f.families[0]).expect("a family installed is found");
            assert!(super::installed().iter().any(|g| g.path == found && g.families.contains(&f.families[0])));
        }
        assert!(super::find("no font is called this").is_none());
    }
}

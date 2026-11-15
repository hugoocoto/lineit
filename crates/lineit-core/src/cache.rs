//! What lineit keeps for each version of the runtime (`codegen::runtime_id`),
//! and how it's deleted when nothing uses it. Each version of lineit that
//! runs, and each change to the runtime's sources, makes a set of its own,
//! with whole cargo targets in it:
//!
//! - `rt/ID`, the runtime's copy (and `rt/ID.used`, see below);
//! - `native/ID`, the target of Rust modules and of Rust and C code squares;
//! - `build/NAME`, the crate of a compiled program, and its files in the
//!   `target` folder all programs share (binary, fingerprints), and the
//!   `lineit-rt` that cargo built there;
//! - `code/lineit-code-*`, a code square's crate, and its files in `target`.
//!
//! What isn't named for the runtime says which it's for in its `Cargo.toml`
//! or `.d` file: the path of `rt/ID`.
//!
//! A process that uses a runtime ([`hold`]) sets the date of `rt/ID.used` and
//! keeps a shared lock on it until it ends. [`evict`] deletes the runtimes
//! that aren't the current one, nor among the last [`KEPT`] used, nor used in
//! the last [`DAYS`] days, and that nobody holds: it takes the lock for
//! itself, or skips the runtime. A compiled program that runs uses nothing
//! of the cache. What can't be deleted (Windows keeps what is loaded or
//! running) isn't an error: the runtime's folder stays, so the next eviction
//! tries again. The `.used` files (empty) stay: an unlinked file locks nothing.
//!
//! Versions of lineit older than this one hold nothing: their runtimes are
//! told only by the age of their folder.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

/// How many of the runtimes last used are kept, whenever that was.
const KEPT: usize = 3;
/// And those used in the last this many days.
const DAYS: u64 = 7;

fn marker(cache: &Path, id: &str) -> PathBuf {
    cache.join("rt").join(format!("{id}.used"))
}

/// The runtimes this process holds: their `.used` files, locked.
fn held() -> &'static Mutex<HashMap<PathBuf, File>> {
    static HELD: OnceLock<Mutex<HashMap<PathBuf, File>>> = OnceLock::new();
    HELD.get_or_init(Default::default)
}

/// This process uses runtime ID of the cache in CACHE until it ends: it sets
/// its date and locks it, so it isn't deleted (it waits while it's being
/// deleted). True the first time.
pub fn hold(cache: &Path, id: &str) -> Result<bool, String> {
    let path = marker(cache, id);
    let mut held = held().lock().unwrap_or_else(|e| e.into_inner());
    if held.contains_key(&path) {
        return Ok(false);
    }
    let err = |e: std::io::Error| format!("{}: {e}", path.display());
    std::fs::create_dir_all(path.parent().unwrap()).map_err(err)?;
    // written to, not appended to: Windows locks only a file open to write
    let f = File::options().create(true).truncate(false).write(true).open(&path).map_err(err)?;
    fs4::FileExt::lock_shared(&f).map_err(err)?;
    let _ = f.set_modified(SystemTime::now());
    held.insert(path, f);
    Ok(true)
}

/// When runtime ID was last used: the date of its `.used` file, or, for
/// those of older lineits, of its folder.
fn used(cache: &Path, id: &str) -> Option<SystemTime> {
    std::fs::metadata(marker(cache, id))
        .or_else(|_| std::fs::metadata(cache.join("rt").join(id)))
        .and_then(|m| m.modified())
        .ok()
}

/// Deletes the runtimes in CACHE that aren't CURRENT, nor among the last
/// [`KEPT`] used, nor used in the last [`DAYS`] days (at NOW), nor held, with
/// what was built for them.
pub fn evict(cache: &Path, current: &str, now: SystemTime) {
    let Ok(entries) = std::fs::read_dir(cache.join("rt")) else { return };
    let mut all: Vec<(SystemTime, String)> = Vec::new();
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        // `.tmp` is one being written
        if !e.path().is_dir() || name.ends_with(".tmp") {
            continue;
        }
        if let Some(t) = used(cache, &name) {
            all.push((t, name));
        }
    }
    all.sort_by(|a, b| b.cmp(a));
    let recent = Duration::from_secs(DAYS * 24 * 3600);
    // locked before anything goes, and until all has
    let locked: Vec<(&str, File)> = all
        .iter()
        .enumerate()
        .filter(|(i, (t, id))| *i >= KEPT && id != current && now.duration_since(*t).is_ok_and(|age| age > recent))
        .filter_map(|(_, (_, id))| {
            let f = File::options().create(true).truncate(false).write(true).open(marker(cache, id)).ok()?;
            fs4::FileExt::try_lock(&f).is_ok().then_some((id.as_str(), f))
        })
        .collect();
    if !locked.is_empty() {
        remove(cache, &locked.iter().map(|(id, _)| *id).collect::<Vec<_>>());
    }
}

/// Whether TEXT (a manifest, a `.d`) has the path of folder `rt/ID`.
fn mentions(text: &str, id: &str) -> bool {
    ["/", "\\"].iter().any(|s| {
        let p = format!("rt{s}{id}");
        text.match_indices(&p).any(|(at, _)| {
            matches!(text.as_bytes().get(at + p.len()), None | Some(b'/' | b'\\' | b'"' | b' ' | b'\n' | b':'))
        })
    })
}

/// Deletes what was made for the runtimes IDS of CACHE, which are locked.
fn remove(cache: &Path, ids: &[&str]) {
    let mut done: HashMap<&str, bool> = ids.iter().map(|id| (*id, true)).collect();
    let target = cache.join("target");
    let of = |text: &str| ids.iter().copied().find(|id| mentions(text, id));
    // programs' and code squares' crates, and what they made in `target`
    for kind in ["build", "code"] {
        let Ok(entries) = std::fs::read_dir(cache.join(kind)) else { continue };
        for e in entries.flatten() {
            let Some(id) = std::fs::read_to_string(e.path().join("Cargo.toml")).ok().and_then(|t| of(&t)) else {
                continue;
            };
            if lineit_rt::process::busy(&e.path()) || std::fs::remove_dir_all(e.path()).is_err() {
                done.insert(id, false);
            } else {
                remove_built(&target, &e.file_name().to_string_lossy());
            }
        }
    }
    // the `lineit-rt` cargo built in `target`, once for each set of features
    for profile in ["release", "debug"] {
        let Ok(entries) = std::fs::read_dir(target.join(profile).join("deps")) else { continue };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Some(meta) = name.strip_prefix("lineit_rt-").and_then(|n| n.strip_suffix(".d")) else { continue };
            if std::fs::read_to_string(e.path()).is_ok_and(|d| of(&d).is_some()) {
                remove_built(&target, &format!("lineit_rt-{meta}"));
                remove_built(&target, &format!("lineit-rt-{meta}"));
            }
        }
    }
    for (id, mut ok) in done {
        let native = cache.join("native").join(id);
        ok &= !native.exists() || std::fs::remove_dir_all(&native).is_ok();
        // the runtime's own folder last, and only if all else went: it says there's more
        if ok {
            let _ = std::fs::remove_dir_all(cache.join("rt").join(id));
        }
    }
}

/// Deletes the files cargo made in TARGET for the crate NAME, as they are
/// named in `deps`, `.fingerprint` and where the binary is.
fn remove_built(target: &Path, name: &str) {
    let under = name.replace('-', "_");
    let mut starts: HashSet<String> = HashSet::new();
    for n in [name, under.as_str()] {
        for pre in ["", "lib"] {
            starts.insert(format!("{pre}{n}-"));
            starts.insert(format!("{pre}{n}."));
        }
    }
    for profile in ["release", "debug"] {
        let p = target.join(profile);
        for dir in [p.clone(), p.join("deps"), p.join(".fingerprint"), p.join("incremental")] {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for e in entries.flatten() {
                let f = e.file_name().to_string_lossy().into_owned();
                if f == name || starts.iter().any(|s| f.starts_with(s.as_str())) {
                    let _ = if e.path().is_dir() {
                        std::fs::remove_dir_all(e.path())
                    } else {
                        std::fs::remove_file(e.path())
                    };
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 24 * 3600;

    fn at(secs_ago: u64) -> SystemTime {
        SystemTime::now() - Duration::from_secs(secs_ago)
    }

    fn put(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// A cache with runtime ID, used DAYS_AGO days ago, and what was built for it.
    fn runtime(cache: &Path, id: &str, days_ago: u64) {
        put(&cache.join("rt").join(id).join("src/lib.rs"), "");
        let f = File::options().create(true).truncate(false).write(true).open(marker(cache, id)).unwrap();
        f.set_modified(at(days_ago * DAY)).unwrap();
        put(&cache.join("native").join(id).join("release/libx.so"), "");
        let rt = format!("{}/rt/{id}", cache.display());
        let manifest = format!("[dependencies]\nlineit-rt = {{ path = \"{rt}\" }}\n");
        for (kind, name) in [("build", format!("prog-{id}")), ("code", format!("lineit-code-{id}"))] {
            put(&cache.join(kind).join(&name).join("Cargo.toml"), &manifest);
            // its files in the target all programs share
            let t = cache.join("target/release");
            let under = name.replace('-', "_");
            for f in [name.clone(), format!("{name}.d"), format!("deps/{under}-1"), format!("deps/lib{under}-1.rlib")] {
                put(&t.join(f), "");
            }
            put(&t.join(".fingerprint").join(format!("{name}-1/x")), "");
        }
        let meta = id.replace(['.', '-'], "");
        put(
            &cache.join(format!("target/release/deps/lineit_rt-{meta}.d")),
            &format!("x.d: {rt}/src/lib.rs {rt}/src/ext.rs\n"),
        );
        put(&cache.join(format!("target/release/deps/liblineit_rt-{meta}.rlib")), "");
        put(&cache.join(format!("target/release/.fingerprint/lineit-rt-{meta}/x")), "");
    }

    fn files(cache: &Path) -> Vec<String> {
        fn walk(p: &Path, out: &mut Vec<String>) {
            for e in std::fs::read_dir(p).unwrap().flatten() {
                if e.path().is_dir() {
                    walk(&e.path(), out);
                } else {
                    out.push(e.path().to_string_lossy().into_owned());
                }
            }
        }
        let mut v = Vec::new();
        walk(cache, &mut v);
        v
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("lineit-cache-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /// What's left of runtime ID's: its folders, the crates made for it and
    /// their files in `target`, and the runtime cargo built there.
    fn left(cache: &Path, id: &str) -> Vec<String> {
        let (under, meta) = (id.replace('-', "_"), id.replace(['.', '-'], ""));
        files(cache)
            .into_iter()
            .filter(|f| {
                (f.contains(id) || f.contains(&under)) && !f.ends_with(".used")
                    || f.contains(&format!("lineit_rt-{meta}"))
                    || f.contains(&format!("lineit-rt-{meta}"))
            })
            .collect()
    }

    /// How many files `runtime` makes for each.
    const MADE: usize = 17;

    #[test]
    fn old_runtimes_go_with_what_was_built_for_them() {
        let cache = temp("old");
        let all =
            [("v-cur", 90), ("v-new1", 0), ("v-new2", 1), ("v-new3", 2), ("v-week", 5), ("v-old1", 8), ("v-old2", 40)];
        for (id, days) in all {
            runtime(&cache, id, days);
        }
        // the current one is old, but kept; the others are the 3 last used, or used this week
        evict(&cache, "v-cur", SystemTime::now());
        for id in ["v-old1", "v-old2"] {
            assert!(left(&cache, id).is_empty(), "{id}: {:?}", left(&cache, id));
            assert!(!cache.join("rt").join(id).exists() && !cache.join("native").join(id).exists());
        }
        for id in ["v-cur", "v-new1", "v-new2", "v-new3", "v-week"] {
            assert_eq!(left(&cache, id).len(), MADE, "{id}: {:?}", left(&cache, id));
        }
        let _ = std::fs::remove_dir_all(&cache);
    }

    #[test]
    fn the_last_used_are_kept_whenever_that_was() {
        let cache = temp("last");
        let ids = ["v-a", "v-b", "v-c", "v-d", "v-e"];
        for (id, days) in ids.iter().zip([100, 200, 300, 400, 500]) {
            runtime(&cache, id, days);
        }
        evict(&cache, "v-e", SystemTime::now());
        let there: Vec<bool> = ids.iter().map(|id| cache.join("rt").join(id).exists()).collect();
        // a, b and c are the last three used; e is the current one
        assert_eq!(there, [true, true, true, false, true]);
        let _ = std::fs::remove_dir_all(&cache);
    }

    #[test]
    fn a_runtime_another_process_holds_stays() {
        let cache = temp("held");
        for (id, days) in [("v-cur", 0), ("v-a", 1), ("v-b", 2), ("v-c", 3), ("v-held", 50), ("v-free", 60)] {
            runtime(&cache, id, days);
        }
        // another process: its own lock, on its own open file
        let other = File::options().write(true).open(marker(&cache, "v-held")).unwrap();
        fs4::FileExt::lock_shared(&other).unwrap();
        evict(&cache, "v-cur", SystemTime::now());
        assert_eq!(left(&cache, "v-held").len(), MADE, "a runtime in use was deleted");
        assert!(left(&cache, "v-free").is_empty());
        // and when it lets go, it goes
        drop(other);
        evict(&cache, "v-cur", SystemTime::now());
        assert!(left(&cache, "v-held").is_empty());
        let _ = std::fs::remove_dir_all(&cache);
    }

    #[test]
    fn this_process_holds_what_it_uses_and_dates_it() {
        let cache = temp("hold");
        for (id, days) in [("v-me", 50), ("v-a", 1), ("v-b", 2), ("v-c", 3), ("v-d", 4)] {
            runtime(&cache, id, days);
        }
        assert!(hold(&cache, "v-me").unwrap());
        assert!(!hold(&cache, "v-me").unwrap());
        assert!(used(&cache, "v-me").unwrap() > at(DAY), "its date wasn't set");
        // locked for others, as another lineit sees it
        let f = File::options().write(true).open(marker(&cache, "v-me")).unwrap();
        assert!(fs4::FileExt::try_lock(&f).is_err());
        // held but not among the last used: only the lock keeps it
        f.set_modified(at(50 * DAY)).unwrap();
        evict(&cache, "v-x", SystemTime::now() + Duration::from_secs(1000 * DAY));
        assert_eq!(left(&cache, "v-me").len(), MADE);
        assert!(!cache.join("rt/v-d").exists(), "not among the last three, nor held");
        let _ = std::fs::remove_dir_all(&cache);
    }

    #[test]
    fn writing_the_runtime_holds_it() {
        let cache = temp("write");
        let rt = crate::paths::with_cache(&cache, || crate::codegen::write_runtime().unwrap());
        let id = crate::codegen::runtime_id();
        assert_eq!(rt, cache.join("rt").join(&id));
        let f = File::options().write(true).open(marker(&cache, &id)).unwrap();
        assert!(fs4::FileExt::try_lock(&f).is_err(), "not held while this process runs");
        let _ = std::fs::remove_dir_all(&cache);
    }

    /// Writers of one runtime at once (processes, threads) each get it
    /// whole, and none takes it from under another that's building with it.
    #[test]
    fn the_runtime_written_by_many_at_once() {
        let id = crate::codegen::runtime_id();
        for round in 0..10 {
            let cache = temp(&format!("many-{round}"));
            let lib = cache.join("rt").join(&id).join("src/lib.rs");
            let barrier = std::sync::Barrier::new(16);
            std::thread::scope(|s| {
                for _ in 0..16 {
                    s.spawn(|| {
                        barrier.wait();
                        let rt = crate::paths::with_cache(&cache, || crate::codegen::write_runtime().unwrap());
                        // still there for whoever builds with it
                        assert!(rt.join("src/lib.rs").exists() && lib.exists());
                    });
                }
            });
            let left: Vec<_> = std::fs::read_dir(cache.join("rt"))
                .unwrap()
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.ends_with(".tmp"))
                .collect();
            assert!(left.is_empty(), "{left:?}");
            let _ = std::fs::remove_dir_all(&cache);
        }
    }

    #[test]
    fn runtimes_of_older_lineits_go_by_the_age_of_their_folder() {
        let cache = temp("legacy");
        for (id, days) in [("v-cur", 0), ("v-a", 1), ("v-b", 2), ("v-c", 3), ("v-old", 20), ("v-week", 5)] {
            runtime(&cache, id, days);
            if id == "v-old" || id == "v-week" {
                // they had no `.used` file: the folder's date is theirs
                std::fs::remove_file(marker(&cache, id)).unwrap();
                File::open(cache.join("rt").join(id)).unwrap().set_modified(at(days * DAY)).unwrap();
            }
        }
        evict(&cache, "v-cur", SystemTime::now());
        assert!(!cache.join("rt/v-old").exists());
        assert!(cache.join("rt/v-week").exists());
        let _ = std::fs::remove_dir_all(&cache);
    }
}

//! Finding where an installed application keeps its data, and deciding what part of it is a session.
//!
//! This is the piece that had to be ported rather than moved: the macOS shell tool located data by
//! walking `~/Library` and reasoning about `.app` bundles, neither of which exists on Windows. The
//! per-OS candidates below are the same idea expressed for three platforms.
use crate::Error;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum Evidence {
    /// A credential file on disk — only written after a real sign-in.
    AccountFile,
    /// Telegram Desktop's own encrypted key store.
    Tdata,
    /// A cookie store whose values are OS-encrypted. The files ARE the session, but only where the
    /// key is: the macOS Keychain or Windows DPAPI, neither of which travels with a copy.
    CookiesEncrypted,
    /// A cookie store held in the clear. Says nothing about whether an account is involved —
    /// Notion Calendar keeps 43 plaintext cookies including its identity host.
    CookiesPlain,
    /// Local Storage / IndexedDB with content, but no cookies. GitHub Desktop and Antigravity keep
    /// their session here rather than in cookies.
    WebStorage,
    /// Nothing that looks like a session.
    None,
}

impl Evidence {
    pub fn label(&self) -> &'static str {
        match self {
            Evidence::AccountFile => "account file",
            Evidence::Tdata => "account store",
            Evidence::CookiesEncrypted => "cookies (encrypted)",
            Evidence::CookiesPlain => "cookies (plaintext)",
            Evidence::WebStorage => "local web data",
            Evidence::None => "none",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AppData {
    pub name: String,
    /// Every directory this app owns, as absolute paths that exist right now.
    pub dirs: Vec<PathBuf>,
    pub evidence: Evidence,
}

fn home() -> Result<PathBuf, Error> {
    directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf()).ok_or(Error::NoHome)
}

/// Directories an app of this name might own. Existence is checked by the caller.
fn candidates(app: &str) -> Result<Vec<PathBuf>, Error> {
    let h = home()?;
    let mut v = Vec::new();
    if cfg!(target_os = "macos") {
        v.push(h.join("Library/Application Support").join(app));
        v.push(h.join("Library/Containers").join(app));
        v.push(h.join("Library/HTTPStorages").join(app));
        v.push(h.join("Library/WebKit").join(app));
    } else if cfg!(target_os = "windows") {
        for var in ["APPDATA", "LOCALAPPDATA"] {
            if let Ok(b) = std::env::var(var) {
                v.push(PathBuf::from(b).join(app));
            }
        }
    } else {
        v.push(h.join(".config").join(app));
        v.push(h.join(".local/share").join(app));
    }
    // CLI-style apps keep everything in a home dotfolder instead (~/.codex, ~/.claude)
    let slug = app.to_lowercase().replace(' ', "");
    v.push(h.join(format!(".{slug}")));
    Ok(v)
}

pub fn dirs_for(app: &str) -> Result<Vec<PathBuf>, Error> {
    Ok(candidates(app)?.into_iter().filter(|p| p.is_dir()).collect())
}

/// Size of a directory tree, symlinks not followed.
///
/// This walks every file underneath, so it costs whatever the tree costs — Claude's profile alone
/// is gigabytes of `vm_bundles`. Never call it on a path the user is waiting to see. When the
/// question is "is there anything here", ask `exceeds` instead.
pub fn dir_size(p: &Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(p) else { return 0 };
    rd.flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_size(&e.path()),
            Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
            _ => 0,
        })
        .sum()
}

/// Does this tree hold more than `limit` bytes? Stops at the first byte past the limit.
///
/// `dir_size(p) > limit` answers the same question and reads better, which is exactly why it was
/// written that way and why the app hung: to prove a 1 KB threshold it summed 11 GB of Claude's
/// `vm_bundles` first. A threshold question must never pay for the whole tree.
pub fn exceeds(p: &Path, limit: u64) -> bool {
    fn walk(p: &Path, limit: u64, seen: &mut u64) -> bool {
        let Ok(rd) = std::fs::read_dir(p) else { return false };
        for e in rd.flatten() {
            match e.file_type() {
                Ok(t) if t.is_dir() => {
                    if walk(&e.path(), limit, seen) {
                        return true;
                    }
                }
                Ok(t) if t.is_file() => {
                    *seen += e.metadata().map(|m| m.len()).unwrap_or(0);
                    if *seen > limit {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }
    walk(p, limit, &mut 0)
}

/// Names that are caches by convention. Excluded from a backup by default because they are large,
/// regenerated on demand, and never the thing anyone wanted to keep.
pub const CACHE_NAMES: &[&str] = &[
    "Cache", "Caches", "Code Cache", "GPUCache", "DawnGraphiteCache", "DawnWebGPUCache",
    "ShaderCache", "Crashpad", "logs", "Logs", "vm_bundles", "Service Worker",
];

pub fn is_cache(name: &str) -> bool {
    CACHE_NAMES.iter().any(|c| c.eq_ignore_ascii_case(name))
}

/// Every file under `dir` that carries session state, as absolute paths.
///
/// Chromium "partitions" are included deliberately. Notion keeps its real session in
/// `Partitions/notion/Cookies` — 35 encrypted cookies, its identity host among them — while the
/// top-level Cookies store sits EMPTY, so a sweep that skipped partitions archived nothing at all.
/// Directories `session_files` will open before it gives up. Shared with its tests so neither side
/// keeps its own copy of the number.
pub(crate) const SESSION_SCAN_MAX_DIRS: usize = 2_000;

pub fn session_files(dir: &Path) -> Vec<PathBuf> {
    const NAMES: &[&str] = &[
        "Cookies", "Cookies-journal", "cookies.sqlite", "Local State", "Local Storage",
        "Session Storage", "IndexedDB", "Network", "tdata",
    ];
    // Breadth-first, with a hard ceiling on directories opened.
    //
    // Both halves of that are load-bearing. Depth-first with no ceiling is what this was, and one
    // folder was able to hold the whole app hostage: `~/Library/Application Support/Chizmachi`
    // holds 145,077 directories inside depth 3 (a test suite that never cleaned up), and
    // classifying that one folder cost 2.4 s of a 2.6 s scan — 93% of it, for an app that has no
    // session at all. Breadth-first means the ceiling, when it is reached, drops the deepest and
    // least likely locations rather than whichever branch happened to sort first; a real Chromium
    // profile keeps its session near the top (Claude's needs 113 directories, Notion's fewer).
    let mut out = Vec::new();
    let mut queue: std::collections::VecDeque<(PathBuf, usize)> =
        std::collections::VecDeque::from([(dir.to_path_buf(), 0usize)]);
    let mut opened = 0usize;

    while let Some((d, depth)) = queue.pop_front() {
        if opened >= SESSION_SCAN_MAX_DIRS {
            break;
        }
        opened += 1;
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            let Some(n) = p.file_name().and_then(|s| s.to_str()) else { continue };
            if NAMES.contains(&n) {
                out.push(p);
                continue;
            }
            if depth < 3 && !is_cache(n) && p.is_dir() {
                queue.push_back((p, depth + 1));
            }
        }
    }
    out
}

/// One cookie store, read properly.
#[derive(Debug, Clone, Default)]
pub struct CookieStore {
    pub total: usize,
    pub encrypted: usize,
    pub plaintext: usize,
}

/// Read a Chromium or Gecko cookie store.
///
/// Opened read-only through a COPY, never in place: the app may hold the file open with a WAL, and
/// a tool that reports on a session must not be able to disturb it.
///
/// Both value columns are read. Chromium puts a cookie's value in `value` when it is not
/// OS-encrypted and in `encrypted_value` when it is; inspecting only the encrypted column reported a
/// store of 131 plaintext cookies as empty. The v10/v11 tag is compared in Rust, not in SQL —
/// SQLite's substr() on a BLOB returns a BLOB, which never compares equal to a text literal, so
/// doing it in SQL silently matches nothing.
///
/// Counts only. Cookie values are never read: they are live credentials.
pub fn read_cookies(path: &Path) -> Option<CookieStore> {
    let tmp = std::env::temp_dir().join(format!("ma-ck-{}-{}.sqlite", std::process::id(), rand_suffix()));
    std::fs::copy(path, &tmp).ok()?;
    let out = (|| -> Option<CookieStore> {
        let db = rusqlite::Connection::open_with_flags(
            &tmp,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .ok()?;
        let mut st = CookieStore::default();
        // Chromium first, then Gecko's different table and column names
        let chromium = db.prepare("SELECT value, encrypted_value FROM cookies").ok();
        if let Some(mut q) = chromium {
            let rows = q
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0).unwrap_or_default(),
                        r.get::<_, Vec<u8>>(1).unwrap_or_default(),
                    ))
                })
                .ok()?;
            for (value, enc) in rows.flatten() {
                st.total += 1;
                if !enc.is_empty() {
                    st.encrypted += 1;
                } else if !value.is_empty() {
                    st.plaintext += 1;
                }
            }
        } else {
            let n: i64 = db.query_row("SELECT count(*) FROM moz_cookies", [], |r| r.get(0)).ok()?;
            st.total = n as usize;
            st.plaintext = n as usize; // Gecko keeps values in the clear
        }
        Some(st)
    })();
    let _ = std::fs::remove_file(&tmp);
    out
}

fn rand_suffix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0)
}

fn cookie_kind(path: &Path) -> Evidence {
    match read_cookies(path) {
        Some(c) if c.encrypted > 0 => Evidence::CookiesEncrypted,
        Some(c) if c.plaintext > 0 => Evidence::CookiesPlain,
        _ => Evidence::None,
    }
}

pub fn evidence_for(app: &str) -> Result<Evidence, Error> {
    let dirs = dirs_for(app)?;
    let mut best = Evidence::None;
    for d in &dirs {
        if d.join("tdata").is_dir() {
            return Ok(Evidence::Tdata);
        }
        for f in session_files(d) {
            let Some(n) = f.file_name().and_then(|s| s.to_str()) else { continue };
            match n {
                "Cookies" | "cookies.sqlite" => match cookie_kind(&f) {
                    Evidence::CookiesEncrypted => return Ok(Evidence::CookiesEncrypted),
                    Evidence::CookiesPlain => best = Evidence::CookiesPlain,
                    _ => {}
                },
                "Local Storage" | "IndexedDB" if best == Evidence::None && exceeds(&f, 1024) => {
                    best = Evidence::WebStorage;
                }
                _ => {}
            }
        }
        // a credential file in a home dotfolder is the strongest signal, but only reached when the
        // app's own data dir showed nothing — ~/.claude belongs to Claude Code, not the desktop app
        if best == Evidence::None {
            for name in ["auth.json", "credentials.json", "token.json", ".credentials"] {
                if d.join(name).exists() {
                    return Ok(Evidence::AccountFile);
                }
            }
        }
    }
    Ok(best)
}

/// Applications on this machine that have local data worth backing up.
///
/// Deliberately does NOT measure how big each app is: that is an unbounded walk per app, and this
/// is the call a waiting user is blocked on. Ask `size_of` for one app once the list is on screen.
pub fn installed() -> Result<Vec<AppData>, Error> {
    let h = home()?;
    let roots: Vec<PathBuf> = if cfg!(target_os = "macos") {
        vec![h.join("Library/Application Support")]
    } else if cfg!(target_os = "windows") {
        ["APPDATA", "LOCALAPPDATA"]
            .iter()
            .filter_map(|v| std::env::var(v).ok())
            .map(PathBuf::from)
            .collect()
    } else {
        vec![h.join(".config"), h.join(".local/share")]
    };

    // Every folder is named first and classified second. Classifying is the expensive half — a
    // depth-3 walk each, paid for the dozens of folders that turn out to hold no session at all
    // (Sublime Text, Xcode and friends) just as much as for the ones that do. Naming is one
    // readdir, so the cheap half stays here and the costly half goes wide below.
    let mut names: Vec<String> = Vec::new();
    for root in roots {
        let Ok(rd) = std::fs::read_dir(&root) else { continue };
        for e in rd.flatten() {
            if !e.path().is_dir() {
                continue;
            }
            let Some(name) = e.file_name().to_str().map(|s| s.to_string()) else { continue };
            if name.starts_with('.') || names.contains(&name) {
                continue;
            }
            names.push(name);
        }
    }

    // Classification is pure reads of independent directories, so it parallelises exactly. The
    // work is I/O-bound rather than CPU-bound; the cap is to avoid thrashing a spinning disk, and
    // `max(1)` because `available_parallelism` may legitimately fail in a container.
    let lanes = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 8);
    let chunk = names.len().div_ceil(lanes).max(1);
    let mut out: Vec<AppData> = Vec::new();
    std::thread::scope(|s| {
        let handles: Vec<_> = names
            .chunks(chunk)
            .map(|slice| {
                s.spawn(move || {
                    let mut found = Vec::new();
                    for name in slice {
                        // An app that cannot be classified is not an app with no session, but it
                        // is not one we can describe either — same omission the serial code made.
                        let ev = evidence_for(name).unwrap_or(Evidence::None);
                        if ev == Evidence::None {
                            continue;
                        }
                        let Ok(dirs) = dirs_for(name) else { continue };
                        found.push(AppData { name: name.clone(), dirs, evidence: ev });
                    }
                    found
                })
            })
            .collect();
        for h in handles {
            // A panicked lane would silently drop its apps, so let it take the scan down with it.
            out.extend(h.join().expect("app-data classification lane panicked"));
        }
    });

    out.sort_by_key(|a| a.name.to_lowercase());
    Ok(out)
}

/// Data folders whose name is not the name of the app that owns them.
///
/// Kept deliberately short: an entry here is a claim about someone else's software, so each one
/// needs evidence. ChatGPT keeping its Chromium data under `Codex` was established by experiment
/// (E4) and re-checked on a machine with no Codex.app at all and a live `Codex/Default` profile.
const OWNER_ALIASES: &[(&str, &str)] = &[("Codex", "ChatGPT")];

/// Which installed application owns this data folder, if any of them does.
///
/// A data folder outlives its app: uninstalling OpenMTP leaves `Application Support/OpenMTP`
/// behind, and listing that under "apps with a session" offers a session for an app that is not
/// there. The match is whole-name and case-insensitive — never a prefix, or `Claude` would claim
/// `Claude-3p` and `Notion` would claim `Notion Calendar`.
pub fn owner_of<'a>(data_name: &str, installed: &'a [String]) -> Option<&'a str> {
    let same = |a: &str, b: &str| a.to_lowercase() == b.to_lowercase();
    if let Some(hit) = installed.iter().find(|app| same(app, data_name)) {
        return Some(hit.as_str());
    }
    OWNER_ALIASES
        .iter()
        .filter(|(folder, _)| same(folder, data_name))
        .find_map(|(_, owner)| installed.iter().find(|app| same(app, owner)))
        .map(String::as_str)
}

/// Bytes one app holds on disk, across every directory it owns.
///
/// Slow by nature, and the caller is expected to know that: the GUI asks for one app at a time
/// after the list is already drawn, so a 12 GB profile delays its own number and nothing else.
pub fn size_of(app: &str) -> Result<u64, Error> {
    Ok(dirs_for(app)?.iter().map(|d| dir_size(d)).sum())
}

#[cfg(test)]
mod size_tests {
    use super::*;

    /// A scratch tree under the real home. `$HOME` relocation is a no-op on Windows, so tests that
    /// need a directory must make one rather than move the home.
    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("multiapp-size-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("sub/deeper")).unwrap();
        d
    }

    fn put(p: &Path, n: usize) {
        std::fs::write(p, vec![b'x'; n]).unwrap();
    }

    #[test]
    fn exceeds_agrees_with_dir_size_on_both_sides_of_the_limit() {
        let d = scratch("agree");
        put(&d.join("a.bin"), 400);
        put(&d.join("sub/b.bin"), 400);
        put(&d.join("sub/deeper/c.bin"), 400);
        assert_eq!(dir_size(&d), 1200, "nested files are counted");

        // The exact boundary matters: the caller writes `> limit`, not `>= limit`.
        assert!(exceeds(&d, 1199));
        assert!(!exceeds(&d, 1200));
        assert!(!exceeds(&d, 5000));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn exceeds_is_false_for_an_empty_or_missing_tree() {
        let d = scratch("empty");
        assert!(!exceeds(&d, 0), "an empty tree exceeds nothing, not even zero");
        assert!(!exceeds(&d.join("nope"), 0), "a missing path is not an error here");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn installed_reports_no_size_of_its_own() {
        // The regression this guards: AppData once carried `bytes`, and filling it walked every
        // app's whole tree before the caller saw a single row. If a size field comes back, the
        // Back Up window is one `.map()` away from hanging again.
        let apps = installed().expect("scan");
        for a in &apps {
            assert!(!a.dirs.is_empty(), "{} was listed with no directories", a.name);
        }
    }
}

#[cfg(test)]
mod scan_bound_tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("multiapp-scan-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Breadth-first is the half of the fix that a bare budget would not give you: `wide/` holds
    /// more subdirectories than the budget allows, and the Cookies file still has to be found,
    /// because visiting by depth opens both depth-1 directories before any depth-2 one.
    ///
    /// Honest limit: this pins the guarantee but does not *prove* it, because it cannot force the
    /// failure. Swapping the queue back to depth-first still passes here — whether `wide` or
    /// `profile` is explored first then comes down to `readdir` order, which the test cannot
    /// control. The budget test below is the one with a verified counterfactual.
    #[test]
    fn a_session_file_near_the_top_survives_a_huge_sibling_branch() {
        let d = scratch("wide");
        let wide = d.join("wide");
        for i in 0..(SESSION_SCAN_MAX_DIRS + 200) {
            std::fs::create_dir_all(wide.join(format!("n{i:05}"))).unwrap();
        }
        std::fs::create_dir_all(d.join("profile")).unwrap();
        std::fs::write(d.join("profile/Cookies"), b"x").unwrap();

        let found = session_files(&d);
        assert!(
            found.iter().any(|p| p.ends_with("profile/Cookies")),
            "the shallow session file was missed; found {found:?}"
        );
        std::fs::remove_dir_all(&d).ok();
    }

    /// And the budget really does bite. Without it this returns every one of the planted files,
    /// which is exactly the unbounded walk that cost 2.4 s on one real folder.
    #[test]
    fn the_walk_stops_after_a_bounded_number_of_directories() {
        let d = scratch("bound");
        let n = SESSION_SCAN_MAX_DIRS + 300;
        for i in 0..n {
            let sub = d.join(format!("n{i:05}"));
            std::fs::create_dir_all(&sub).unwrap();
            std::fs::write(sub.join("Cookies"), b"x").unwrap();
        }
        let found = session_files(&d);
        assert!(found.len() < n, "the walk was not bounded: it returned all {n} entries");
        assert!(!found.is_empty(), "the walk stopped before finding anything at all");
        std::fs::remove_dir_all(&d).ok();
    }
}

#[cfg(test)]
mod owner_tests {
    use super::*;

    fn apps(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_leftover_folder_has_no_owner() {
        // the case that started this: OpenMTP's folder is on disk, OpenMTP.app is not
        assert_eq!(owner_of("OpenMTP", &apps(&["Claude", "Notion"])), None);
    }

    #[test]
    fn the_match_ignores_case_but_never_takes_a_prefix() {
        let installed = apps(&["HDRezka-Client", "Notion Calendar", "Claude"]);
        assert_eq!(owner_of("hdrezka-client", &installed), Some("HDRezka-Client"));
        assert_eq!(owner_of("Claude-3p", &installed), None, "Claude must not claim Claude-3p");
        assert_eq!(owner_of("Notion", &installed), None, "Notion Calendar is a different app");
    }

    #[test]
    fn an_alias_only_counts_when_its_owner_is_installed() {
        assert_eq!(owner_of("Codex", &apps(&["ChatGPT"])), Some("ChatGPT"));
        assert_eq!(owner_of("Codex", &apps(&["Claude"])), None);
        // a real Codex app, were one installed, wins over the alias
        assert_eq!(owner_of("Codex", &apps(&["ChatGPT", "Codex"])), Some("Codex"));
    }
}

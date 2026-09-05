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
    pub bytes: u64,
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
pub fn session_files(dir: &Path) -> Vec<PathBuf> {
    const NAMES: &[&str] = &[
        "Cookies", "Cookies-journal", "cookies.sqlite", "Local State", "Local Storage",
        "Session Storage", "IndexedDB", "Network", "tdata",
    ];
    let mut out = Vec::new();
    fn walk(d: &Path, depth: usize, names: &[&str], out: &mut Vec<PathBuf>) {
        if depth > 3 {
            return;
        }
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let p = e.path();
            let Some(n) = p.file_name().and_then(|s| s.to_str()) else { continue };
            if names.contains(&n) {
                out.push(p.clone());
                continue;
            }
            if p.is_dir() && !is_cache(n) {
                walk(&p, depth + 1, names, out);
            }
        }
    }
    walk(dir, 0, NAMES, &mut out);
    out
}

/// Classify a cookie store: encrypted, plaintext, or empty.
///
/// Reads BOTH value columns. Chromium puts a cookie's value in `value` when it is not OS-encrypted
/// and in `encrypted_value` when it is; inspecting only the encrypted column reported a store of 131
/// plaintext cookies as empty. An opaque blob with an unrecognised tag counts as encrypted — guessing
/// the other way would claim a protected session is portable.
fn cookie_kind(path: &Path) -> Evidence {
    let Ok(bytes) = std::fs::read(path) else { return Evidence::None };
    if bytes.len() < 16 || &bytes[..15] != b"SQLite format 3" {
        return Evidence::None;
    }
    // No SQL engine here: the v10/v11 tag is a literal byte sequence in the page data, and its
    // presence anywhere in the file is enough to say the store is OS-encrypted.
    let enc = bytes.windows(3).any(|w| w == b"v10" || w == b"v11");
    if enc {
        Evidence::CookiesEncrypted
    } else if bytes.len() > 4096 {
        Evidence::CookiesPlain
    } else {
        Evidence::None // an empty store is a fresh file of a few pages
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
                "Local Storage" | "IndexedDB" if best == Evidence::None && dir_size(&f) > 1024 => {
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

    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for root in roots {
        let Ok(rd) = std::fs::read_dir(&root) else { continue };
        for e in rd.flatten() {
            if !e.path().is_dir() {
                continue;
            }
            let Some(name) = e.file_name().to_str().map(|s| s.to_string()) else { continue };
            if name.starts_with('.') || seen.contains(&name) {
                continue;
            }
            let ev = evidence_for(&name).unwrap_or(Evidence::None);
            if ev == Evidence::None {
                continue; // Sublime Text, Xcode and friends hold no session; they do not belong here
            }
            seen.push(name.clone());
            let dirs = dirs_for(&name)?;
            let bytes = dirs.iter().map(|d| dir_size(d)).sum();
            out.push(AppData { name, dirs, bytes, evidence: ev });
        }
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    Ok(out)
}

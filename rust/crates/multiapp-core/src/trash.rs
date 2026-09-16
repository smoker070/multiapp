//! Multiapp's Trash: what was deleted, putting a profile back, and emptying it.
//!
//! Nothing in Multiapp deletes a profile outright — it is moved here — so this is the one place a
//! permanent deletion happens, and only on an explicit request. Entries come from two writers with
//! different names, and both have to be read:
//!
//! - `<app>__<name>__<YYYYMMDD-HHMMSS>` — the bash CLI, and this crate from now on
//! - `<app>-<name>-<YYYYMMDD-HHMMSS>`   — this crate before; ambiguous, since both halves may contain
//!   `-`, so the app is recovered by matching an app folder that still exists under Profiles/
//! - `restore-…`, `restore-backup-…`, `session-backup-…`, `app-import-…` — files a restore or import
//!   replaced, staged here instead of being overwritten
//! - a folder holding `RESTORE.tsv` — Claude session files moved aside, with their original paths
use crate::{appdata, paths, Error};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct TrashItem {
    /// The folder name inside Trash — the handle every other call takes.
    pub id: String,
    /// "profile" | "sessions" | "replaced" | "other"
    pub kind: String,
    pub app: Option<String>,
    pub name: Option<String>,
    /// "YYYY-MM-DD HH:MM", from the name; None when the name carries no time
    pub deleted_at: Option<String>,
    pub bytes: u64,
    /// sessions only: how many, and the first few titles
    pub sessions: usize,
    pub titles: Vec<String>,
    /// true only for a profile whose app and name are known — the two things a restore needs
    pub restorable: bool,
}

/// `…YYYYMMDD-HHMMSS` at the end of a name, split from what precedes it and its separator.
fn split_stamp(s: &str) -> Option<(&str, &str)> {
    if s.len() < 15 || !s.is_char_boundary(s.len() - 15) {
        return None;
    }
    let (head, stamp) = s.split_at(s.len() - 15);
    let b = stamp.as_bytes();
    let ok = b[8] == b'-' && b.iter().enumerate().all(|(i, c)| i == 8 || c.is_ascii_digit());
    if !ok {
        return None;
    }
    let head = head.strip_suffix("__").or_else(|| head.strip_suffix('-'))?;
    Some((head, stamp))
}

fn pretty_stamp(st: &str) -> String {
    format!("{}-{}-{} {}:{}", &st[0..4], &st[4..6], &st[6..8], &st[9..11], &st[11..13])
}

/// Session index files under a folder, and their titles.
fn session_titles(dir: &Path) -> (usize, Vec<String>) {
    fn walk(d: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let p = e.path();
            match e.file_type() {
                Ok(t) if t.is_dir() => walk(&p, out),
                Ok(t) if t.is_file() => {
                    let n = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
                    if n.starts_with("local_") && n.ends_with(".json") {
                        out.push(p);
                    }
                }
                _ => {}
            }
        }
    }
    let mut files = Vec::new();
    walk(dir, &mut files);
    let mut titles: Vec<String> = files
        .iter()
        .filter_map(|f| std::fs::read_to_string(f).ok())
        .filter_map(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .filter_map(|v| v.get("title").and_then(|t| t.as_str()).map(str::to_string))
        .collect();
    titles.sort();
    titles.truncate(5);
    (files.len(), titles)
}

fn classify(id: &str, dir: &Path) -> TrashItem {
    let mut it = TrashItem {
        id: id.to_string(),
        kind: "other".into(),
        app: None,
        name: None,
        deleted_at: None,
        bytes: appdata::dir_size(dir),
        sessions: 0,
        titles: Vec::new(),
        restorable: false,
    };
    let stamped = split_stamp(id);
    it.deleted_at = stamped.map(|(_, st)| pretty_stamp(st));

    if dir.join("RESTORE.tsv").is_file() {
        it.kind = "sessions".into();
        (it.sessions, it.titles) = session_titles(dir);
        return it;
    }
    for p in ["restore-backup-", "session-backup-", "app-import-", "restore-"] {
        if id.starts_with(p) {
            it.kind = "replaced".into();
            it.app = stamped.map(|(h, _)| h[p.len().min(h.len())..].to_string()).filter(|s| !s.is_empty());
            return it;
        }
    }
    let Some((head, _)) = stamped else { return it };

    // current form: <app>__<name>
    if let Some((app, name)) = head.split_once("__") {
        it.kind = "profile".into();
        it.app = Some(app.to_string());
        it.name = Some(name.to_string());
        it.restorable = !app.is_empty() && !name.is_empty();
        return it;
    }
    // old form: <app>-<name>. Only a profile folder is worth guessing at.
    if !dir.join("data").is_dir() {
        return it;
    }
    it.kind = "profile".into();
    let apps: Vec<String> = paths::profiles_root()
        .ok()
        .and_then(|r| std::fs::read_dir(r).ok())
        .map(|rd| rd.flatten().filter_map(|e| e.file_name().to_str().map(str::to_string)).collect())
        .unwrap_or_default();
    // the LONGEST matching app wins: "google-chrome-Ismail" must not become app "google"
    if let Some(app) = apps
        .iter()
        .filter(|a| head.len() > a.len() + 1 && head.starts_with(a.as_str()) && head.as_bytes()[a.len()] == b'-')
        .max_by_key(|a| a.len())
    {
        it.app = Some(app.clone());
        it.name = Some(head[app.len() + 1..].to_string());
        it.restorable = true;
    } else {
        it.name = Some(head.to_string());
    }
    it
}

/// The folder for one entry. `id` comes from a web view, so it must be a plain child of Trash —
/// never a path, never `..` — and it must exist.
pub fn item_path(id: &str) -> Result<PathBuf, Error> {
    if id.is_empty() || id == "." || id == ".." || id.contains(['/', '\\']) {
        return Err(Error::OutsideRoot(id.to_string()));
    }
    let p = paths::trash_root()?.join(id);
    paths::assert_inside_root(&p)?;
    if !p.is_dir() {
        return Err(Error::NotInTrash(id.to_string()));
    }
    Ok(p)
}

/// Everything in Trash, newest first.
pub fn list() -> Result<Vec<TrashItem>, Error> {
    let root = paths::trash_root()?;
    let Ok(rd) = std::fs::read_dir(&root) else { return Ok(Vec::new()) };
    let mut v: Vec<TrashItem> = rd
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| e.file_name().to_str().map(|n| classify(n, &e.path())))
        .collect();
    v.sort_by(|a, b| b.deleted_at.cmp(&a.deleted_at).then_with(|| a.id.cmp(&b.id)));
    Ok(v)
}

/// Put a deleted profile back where it was. Refuses when a profile of that name exists again, so a
/// restore can never overwrite one.
pub fn restore(id: &str) -> Result<PathBuf, Error> {
    let from = item_path(id)?;
    let it = classify(id, &from);
    let (Some(app), Some(name), true) = (it.app, it.name, it.restorable) else {
        return Err(Error::NotRestorable(id.to_string()));
    };
    paths::validate_name(&name)?;
    let data = paths::profile_data_dir(&app, &name)?;
    let to = data.parent().ok_or(Error::NoHome)?.to_path_buf();
    paths::assert_inside_root(&to)?;
    if to.exists() {
        return Err(Error::Exists(format!("{app}/{name}")));
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(&from, &to)?;
    Ok(to)
}

/// Permanently delete everything in Trash. Returns (entries, bytes) removed.
///
/// Each entry is checked against the Multiapp root before removal, so a symlink or a moved root can
/// never turn this into a delete somewhere else. The Trash folder itself is kept.
pub fn empty() -> Result<(usize, u64), Error> {
    let items = list()?;
    let (mut n, mut bytes) = (0, 0);
    for it in items {
        let p = item_path(&it.id)?;
        if std::fs::symlink_metadata(&p)?.file_type().is_symlink() {
            return Err(Error::OutsideRoot(p.display().to_string()));
        }
        std::fs::remove_dir_all(&p)?;
        n += 1;
        bytes += it.bytes;
    }
    Ok((n, bytes))
}

/// Open an entry in the platform file manager.
pub fn reveal(id: &str) -> Result<(), Error> {
    let p = item_path(id)?;
    let cmd = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(cmd).arg(&p).spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile;

    fn with_root<T>(f: impl FnOnce() -> T) -> T {
        let _g = paths::env_guard();
        let dir = std::env::temp_dir().join(format!("ma-trash-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::env::set_var("MULTIAPP_HOME", &dir);
        let out = f();
        let _ = std::fs::remove_dir_all(&dir);
        out
    }

    fn mk(rel: &str, file: &str, body: &str) {
        let d = paths::trash_root().unwrap().join(rel);
        std::fs::create_dir_all(&d).unwrap();
        if !file.is_empty() {
            let f = d.join(file);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, body).unwrap();
        }
    }

    /// The real names found in a real Trash on 2026-09-16, one of each writer.
    #[test]
    fn every_writer_is_read() {
        with_root(|| {
            std::fs::create_dir_all(paths::profiles_root().unwrap().join("google-chrome")).unwrap();
            std::fs::create_dir_all(paths::profiles_root().unwrap().join("google")).unwrap();
            mk("Google Chrome__Personal__20260912-172830", "data/Cookies", "x");
            mk("google-chrome-Ismail 2-20260906-143624", "data/Cookies", "x");
            mk("restore-backup-telegram-20260101-000000", "f", "x");
            mk("claude-session-copies__20260916-174249", "RESTORE.tsv", "#\n");
            mk("claude-session-copies__20260916-174249", "index/a/b/local_1.json", r#"{"title":"Tiverna Main"}"#);
            let v = list().unwrap();
            let by = |id: &str| v.iter().find(|i| i.id == id).unwrap().clone();

            let a = by("Google Chrome__Personal__20260912-172830");
            assert_eq!((a.kind.as_str(), a.app.as_deref(), a.name.as_deref(), a.restorable),
                       ("profile", Some("Google Chrome"), Some("Personal"), true));
            assert_eq!(a.deleted_at.as_deref(), Some("2026-09-12 17:28"));

            // old form: the LONGEST app folder wins, or "google" would take "chrome-Ismail 2" as the name
            let b = by("google-chrome-Ismail 2-20260906-143624");
            assert_eq!((b.app.as_deref(), b.name.as_deref(), b.restorable), (Some("google-chrome"), Some("Ismail 2"), true));

            assert_eq!(by("restore-backup-telegram-20260101-000000").kind, "replaced");

            let s = by("claude-session-copies__20260916-174249");
            assert_eq!((s.kind.as_str(), s.sessions, s.restorable), ("sessions", 1, false));
            assert_eq!(s.titles, vec!["Tiverna Main".to_string()]);

            // newest first
            assert_eq!(v[0].id, "claude-session-copies__20260916-174249");
        })
    }

    #[test]
    fn a_deleted_profile_comes_back_intact() {
        with_root(|| {
            profile::create("Edge", "work").unwrap();
            std::fs::write(paths::profile_data_dir("Edge", "work").unwrap().join("Cookies"), "login").unwrap();
            let staged = profile::delete_to_trash("Edge", "work", "20260101-000000").unwrap();
            let id = staged.file_name().unwrap().to_str().unwrap().to_string();

            let it = list().unwrap().into_iter().find(|i| i.id == id).unwrap();
            assert_eq!((it.app.as_deref(), it.name.as_deref(), it.restorable), (Some("Edge"), Some("work"), true));

            restore(&id).unwrap();
            let back = std::fs::read_to_string(paths::profile_data_dir("Edge", "work").unwrap().join("Cookies")).unwrap();
            assert_eq!(back, "login");
            assert!(list().unwrap().is_empty());
        })
    }

    #[test]
    fn restore_never_overwrites_a_profile_that_exists_again() {
        with_root(|| {
            profile::create("Edge", "work").unwrap();
            let id = profile::delete_to_trash("Edge", "work", "20260101-000000").unwrap()
                .file_name().unwrap().to_str().unwrap().to_string();
            profile::create("Edge", "work").unwrap();
            std::fs::write(paths::profile_data_dir("Edge", "work").unwrap().join("Cookies"), "new").unwrap();

            assert!(matches!(restore(&id), Err(Error::Exists(_))));
            assert_eq!(std::fs::read_to_string(paths::profile_data_dir("Edge", "work").unwrap().join("Cookies")).unwrap(), "new");
            assert!(item_path(&id).is_ok(), "the trashed copy must still be there");
        })
    }

    #[test]
    fn only_profiles_are_restored() {
        with_root(|| {
            mk("restore-backup-telegram-20260101-000000", "f", "x");
            mk("claude-session-copies__20260916-174249", "RESTORE.tsv", "#\n");
            assert!(matches!(restore("restore-backup-telegram-20260101-000000"), Err(Error::NotRestorable(_))));
            assert!(matches!(restore("claude-session-copies__20260916-174249"), Err(Error::NotRestorable(_))));
        })
    }

    #[test]
    fn an_id_cannot_leave_the_trash() {
        with_root(|| {
            mk("a__b__20260101-000000", "data/x", "x");
            profile::create("Edge", "keep").unwrap();
            for bad in ["", ".", "..", "../Profiles", "a/b", "..\\x", "missing"] {
                assert!(item_path(bad).is_err(), "{bad:?} must be refused");
                assert!(restore(bad).is_err(), "{bad:?} must be refused");
            }
        })
    }

    #[test]
    fn empty_removes_the_trash_contents_and_nothing_else() {
        with_root(|| {
            profile::create("Edge", "keep").unwrap();
            mk("a__b__20260101-000000", "data/x", "0123456789");
            mk("claude-session-copies__20260916-174249", "RESTORE.tsv", "#\n");
            let (n, bytes) = empty().unwrap();
            assert_eq!(n, 2);
            assert!(bytes >= 10);
            assert!(list().unwrap().is_empty());
            assert!(paths::trash_root().unwrap().is_dir(), "the Trash folder itself stays");
            assert!(paths::profile_data_dir("Edge", "keep").unwrap().is_dir(), "a live profile must survive");
        })
    }
}

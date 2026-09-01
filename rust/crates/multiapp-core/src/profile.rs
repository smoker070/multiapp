//! Profile lifecycle: create, list, launch, stop.
use crate::{launch, paths, proc, Error};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub app: String,
    pub name: String,
    pub running: bool,
    /// false when processes existed that we could not inspect (Windows elevation); destructive
    /// operations must refuse rather than assume the profile is idle.
    pub certain: bool,
    pub data_dir: PathBuf,
}

pub fn create(app: &str, name: &str) -> Result<PathBuf, Error> {
    paths::validate_name(name)?;
    let dir = paths::profile_data_dir(app, name)?;
    let profile_dir = dir.parent().ok_or(Error::NoHome)?.to_path_buf();
    if profile_dir.exists() {
        return Err(Error::Exists(format!("{app}/{name}")));
    }
    std::fs::create_dir_all(&dir)?;
    Ok(profile_dir)
}

pub fn list() -> Result<Vec<Profile>, Error> {
    let root = paths::profiles_root()?;
    let mut out = Vec::new();
    if !root.exists() {
        return Ok(out);
    }
    for app_ent in std::fs::read_dir(&root)? {
        let app_ent = app_ent?;
        if !app_ent.file_type()?.is_dir() {
            continue;
        }
        let app = app_ent.file_name().to_string_lossy().to_string();
        if app.starts_with('.') {
            continue; // .DS_Store and friends are not apps
        }
        for p_ent in std::fs::read_dir(app_ent.path())? {
            let p_ent = p_ent?;
            if !p_ent.file_type()?.is_dir() {
                continue;
            }
            let name = p_ent.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let data_dir = p_ent.path().join("data");
            let s = proc::sweep(&data_dir);
            out.push(Profile {
                app: app.clone(),
                name,
                running: s.is_running(),
                certain: s.is_certain(),
                data_dir,
            });
        }
    }
    out.sort_by(|a, b| (a.app.as_str(), a.name.as_str()).cmp(&(b.app.as_str(), b.name.as_str())));
    Ok(out)
}

pub fn launch_profile(app_ref: &str, app_key: &str, name: &str, extra: &[String]) -> Result<(), Error> {
    paths::validate_name(name)?;
    let dir = paths::profile_data_dir(app_key, name)?;
    let app = launch::resolve_app(app_ref)?;
    launch::launch(&app, &dir, extra)
}

pub fn stop(app_key: &str, name: &str, timeout_secs: u64) -> Result<bool, Error> {
    paths::validate_name(name)?;
    let dir = paths::profile_data_dir(app_key, name)?;
    if !dir.exists() {
        return Err(Error::NoProfile(format!("{app_key}/{name}")));
    }
    proc::quit_and_wait(&dir, std::time::Duration::from_secs(timeout_secs))
}

/// Bytes on disk for a profile. Walks the tree rather than trusting a cached figure, because the
/// number people care about is "how much would deleting this free", and Chromium profiles grow
/// without the tool being involved.
pub fn size_bytes(app: &str, name: &str) -> Result<u64, Error> {
    fn walk(p: &std::path::Path) -> u64 {
        let Ok(rd) = std::fs::read_dir(p) else { return 0 };
        rd.flatten()
            .map(|e| match e.file_type() {
                Ok(t) if t.is_dir() => walk(&e.path()),
                Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
                _ => 0, // symlinks are not followed: a link out of the profile is not its weight
            })
            .sum()
    }
    let dir = profile_dir(app, name)?;
    Ok(walk(&dir))
}

/// The profile's own directory (the parent of `data/`).
fn profile_dir(app: &str, name: &str) -> Result<PathBuf, Error> {
    let d = paths::profile_data_dir(app, name)?;
    let p = d.parent().ok_or(Error::NoHome)?.to_path_buf();
    paths::assert_inside_root(&p)?;
    Ok(p)
}

/// A profile must be idle before it is renamed, cloned onto, or removed. "unknown" counts as busy:
/// processes existed that could not be inspected, and acting on a live profile corrupts it.
fn require_idle(app: &str, name: &str) -> Result<(), Error> {
    let data = paths::profile_data_dir(app, name)?;
    let s = proc::sweep(&data);
    if s.is_running() {
        return Err(Error::Running(format!("{app}/{name}")));
    }
    if s.opaque > 0 {
        return Err(Error::Uncertain(format!("{app}/{name}"), s.opaque));
    }
    Ok(())
}

/// Rename a profile. Refuses if it is running, if the name is invalid, or if the target exists.
pub fn rename(app: &str, old: &str, new: &str) -> Result<PathBuf, Error> {
    paths::validate_name(new)?;
    require_idle(app, old)?;
    let from = profile_dir(app, old)?;
    let to = profile_dir(app, new)?;
    if !from.exists() {
        return Err(Error::NoProfile(format!("{app}/{old}")));
    }
    if to.exists() {
        return Err(Error::Exists(format!("{app}/{new}")));
    }
    std::fs::rename(&from, &to)?;
    Ok(to)
}

/// Copy a profile, logins and all. The source must be idle: copying a live Chromium profile yields
/// a half-written LevelDB that the copy then refuses to open.
pub fn clone_profile(app: &str, from_name: &str, to_name: &str) -> Result<PathBuf, Error> {
    paths::validate_name(to_name)?;
    require_idle(app, from_name)?;
    let from = profile_dir(app, from_name)?;
    let to = profile_dir(app, to_name)?;
    if !from.exists() {
        return Err(Error::NoProfile(format!("{app}/{from_name}")));
    }
    if to.exists() {
        return Err(Error::Exists(format!("{app}/{to_name}")));
    }
    copy_tree(&from, &to)?;
    Ok(to)
}

fn copy_tree(from: &std::path::Path, to: &std::path::Path) -> Result<(), Error> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)?.flatten() {
        let src = e.path();
        let dst = to.join(e.file_name());
        match e.file_type() {
            Ok(t) if t.is_dir() => copy_tree(&src, &dst)?,
            Ok(t) if t.is_file() => {
                std::fs::copy(&src, &dst)?;
            }
            // Symlinks are skipped rather than followed: following one can copy the entire home
            // directory into a profile, and a profile has no legitimate need of them.
            _ => {}
        }
    }
    Ok(())
}

/// Remove a profile by MOVING it into Multiapp's own Trash, never by deleting it.
///
/// A profile holds real logins; an accidental click must be recoverable. The staged copy keeps its
/// app and profile name plus a timestamp so it can be identified later, and the caller is told
/// exactly where it went.
pub fn delete_to_trash(app: &str, name: &str, stamp: &str) -> Result<PathBuf, Error> {
    require_idle(app, name)?;
    let from = profile_dir(app, name)?;
    if !from.exists() {
        return Err(Error::NoProfile(format!("{app}/{name}")));
    }
    let trash = paths::trash_root()?;
    std::fs::create_dir_all(&trash)?;
    let safe = format!("{app}-{name}-{stamp}").replace(['/', '\\', ':'], "-");
    let to = trash.join(safe);
    paths::assert_inside_root(&to)?;
    // A rename across the same volume is atomic and instant; a copy would double the disk use of a
    // profile that can run to gigabytes.
    match std::fs::rename(&from, &to) {
        Ok(()) => Ok(to),
        Err(_) => {
            copy_tree(&from, &to)?;
            std::fs::remove_dir_all(&from)?;
            Ok(to)
        }
    }
}

/// Open a profile's folder in the platform file manager.
pub fn reveal(app: &str, name: &str) -> Result<(), Error> {
    let dir = profile_dir(app, name)?;
    let cmd = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    // explorer returns a non-zero exit code even on success, so the status is deliberately ignored
    let _ = std::process::Command::new(cmd).arg(&dir).spawn()?;
    Ok(())
}

#[cfg(test)]
mod op_tests {
    use super::*;

    /// Each test gets its own root. MULTIAPP_HOME is process-wide, so these run serially by design —
    /// cargo runs tests in threads, and a shared env var would make them flake against each other.
    fn with_root<T>(f: impl FnOnce() -> T) -> T {
        // the SAME lock the paths tests use — a private one here would not stop them clobbering
        // MULTIAPP_HOME from another thread
        let _g = paths::env_guard();
        let dir = std::env::temp_dir().join(format!("ma-op-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::env::set_var("MULTIAPP_HOME", &dir);
        let out = f();
        let _ = std::fs::remove_dir_all(&dir);
        out
    }

    fn seed(app: &str, name: &str, marker: &str) {
        create(app, name).unwrap();
        let d = paths::profile_data_dir(app, name).unwrap();
        std::fs::write(d.join("Cookies"), marker).unwrap();
    }

    #[test]
    fn delete_moves_to_trash_and_keeps_the_data() {
        with_root(|| {
            seed("Edge", "gone", "a real login");
            let staged = delete_to_trash("Edge", "gone", "20260101-000000").unwrap();
            // the profile is out of the list...
            assert!(!list().unwrap().iter().any(|p| p.name == "gone"));
            // ...but every byte is still on disk, which is the whole point of not deleting
            let kept = std::fs::read_to_string(staged.join("data").join("Cookies")).unwrap();
            assert_eq!(kept, "a real login");
            assert!(staged.starts_with(paths::trash_root().unwrap()));
        })
    }

    #[test]
    fn clone_copies_contents_and_refuses_an_existing_target() {
        with_root(|| {
            seed("Edge", "src", "session");
            clone_profile("Edge", "src", "dst").unwrap();
            let d = paths::profile_data_dir("Edge", "dst").unwrap();
            assert_eq!(std::fs::read_to_string(d.join("Cookies")).unwrap(), "session");
            // the original must survive a clone
            assert!(paths::profile_data_dir("Edge", "src").unwrap().join("Cookies").exists());
            assert!(clone_profile("Edge", "src", "dst").is_err());
        })
    }

    #[test]
    fn rename_refuses_a_collision_and_leaves_both_intact() {
        with_root(|| {
            seed("Edge", "one", "1");
            seed("Edge", "two", "2");
            assert!(rename("Edge", "one", "two").is_err());
            assert_eq!(std::fs::read_to_string(paths::profile_data_dir("Edge", "one").unwrap().join("Cookies")).unwrap(), "1");
            assert_eq!(std::fs::read_to_string(paths::profile_data_dir("Edge", "two").unwrap().join("Cookies")).unwrap(), "2");
            rename("Edge", "one", "three").unwrap();
            assert!(paths::profile_data_dir("Edge", "three").unwrap().join("Cookies").exists());
        })
    }

    #[test]
    fn size_counts_what_is_there() {
        with_root(|| {
            seed("Edge", "big", "0123456789");
            assert_eq!(size_bytes("Edge", "big").unwrap(), 10);
        })
    }

    #[test]
    fn a_bad_name_cannot_escape_the_root() {
        with_root(|| {
            seed("Edge", "ok", "x");
            for bad in ["../escape", "..", "a/b"] {
                assert!(rename("Edge", "ok", bad).is_err(), "rename to {bad:?} must be refused");
                assert!(clone_profile("Edge", "ok", bad).is_err(), "clone to {bad:?} must be refused");
            }
        })
    }
}

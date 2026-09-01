//! Per-OS locations. One place, so a wrong root can't be introduced piecemeal.
use crate::Error;
use std::path::PathBuf;

/// Root for all Multiapp state.
///
/// Windows uses **LocalAppData**, not Roaming: profiles are gigabytes (Claude alone downloads ~11 GB
/// of Cowork bundles per profile), and Roaming is synced by enterprise roaming-profile policy — that
/// combination is how you fill a fileserver and make logins take ten minutes.
///
/// NOTE for Windows: the NSIS installer also installs into `%LOCALAPPDATA%\Multiapp`, so the program
/// and the profiles share a directory. That was tested rather than assumed — a canary file inside
/// Profiles/ survived a silent uninstall, because Tauri's uninstaller removes the files it installed
/// instead of the tree. It stays safe only while that remains true, so if the installer ever gains a
/// "remove all data" step, the profiles root must move out from under it first.
pub fn root() -> Result<PathBuf, Error> {
    if let Ok(over) = std::env::var("MULTIAPP_HOME") {
        if !over.is_empty() {
            return Ok(PathBuf::from(over));
        }
    }
    let base = directories::BaseDirs::new().ok_or(Error::NoHome)?;
    Ok(if cfg!(target_os = "macos") {
        base.home_dir().join("Library/Application Support/Multiapp")
    } else if cfg!(target_os = "windows") {
        base.data_local_dir().join("Multiapp")
    } else {
        base.data_local_dir().join("multiapp")
    })
}

pub fn profiles_root() -> Result<PathBuf, Error> {
    Ok(root()?.join("Profiles"))
}

pub fn trash_root() -> Result<PathBuf, Error> {
    Ok(root()?.join("Trash"))
}

/// The directory actually passed to `--user-data-dir=`.
pub fn profile_data_dir(app: &str, name: &str) -> Result<PathBuf, Error> {
    Ok(profiles_root()?.join(app).join(name).join("data"))
}

/// Refuse to touch anything outside our own root. Ported from the bash `assert_inside_root`, which
/// exists because every destructive path in this tool is one bad join away from someone's home dir.
pub fn assert_inside_root(p: &std::path::Path) -> Result<(), Error> {
    // BOTH sides go through the same resolution. Resolving only one of them is what broke this
    // twice: canonicalising just the root rejected new children under a symlinked root, and then
    // canonicalising just the path rejected everything under a root that does not exist yet.
    let root_c = resolve_existing_prefix(&root()?);
    let p_c = resolve_existing_prefix(p);
    if p_c.starts_with(&root_c) {
        Ok(())
    } else {
        Err(Error::OutsideRoot(p_c.display().to_string()))
    }
}

/// Resolve as much of `p` as exists on disk, then re-attach the part that does not.
///
/// Canonicalising only when the WHOLE path exists is wrong, and wrong precisely on the write path.
/// On macOS `/var` and `/tmp` are symlinks: the root resolves to `/private/var/...` while a
/// not-yet-created child stays `/var/...`, so a plain `starts_with` rejected paths that were plainly
/// inside the root — every create, clone and rename into a fresh directory. Resolving the existing
/// prefix puts both sides in the same form, and still resolves any symlinked ancestor, which is what
/// the check is defending against.
fn resolve_existing_prefix(p: &std::path::Path) -> PathBuf {
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    let mut cur = p.to_path_buf();
    loop {
        if let Ok(c) = cur.canonicalize() {
            let mut out = c;
            for part in tail.iter().rev() {
                out.push(part);
            }
            return out;
        }
        let Some(name) = cur.file_name().map(|n| n.to_os_string()) else {
            return p.to_path_buf(); // reached the root without finding anything that exists
        };
        let Some(parent) = cur.parent().map(|q| q.to_path_buf()) else {
            return p.to_path_buf();
        };
        tail.push(name);
        cur = parent;
    }
}

/// Profile names may contain spaces and underscores (macOS allowed them; the PowerShell port did not,
/// so a profile made on one OS was unusable on the other). Path separators are never allowed.
pub fn validate_name(name: &str) -> Result<(), Error> {
    let bad = name.is_empty()
        || name.len() > 41
        || !name.chars().next().map(|c| c.is_ascii_alphanumeric()).unwrap_or(false)
        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '_' || c == '-')
        || name.ends_with(' ');
    if bad {
        return Err(Error::BadName(name.to_string()));
    }
    Ok(())
}

/// MULTIAPP_HOME is process-global, so every test that sets it must serialise against every other
/// one. cargo runs tests on threads by default, and without this a test reads the root another test
/// had just replaced — which showed up as one failure in parallel and none when run with
/// --test-threads=1, the most misleading shape a test failure can take.
#[cfg(test)]
pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
pub(crate) fn env_guard() -> std::sync::MutexGuard<'static, ()> {
    ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_allow_spaces_and_underscores() {
        // macOS allowed these; the PowerShell port rejected them, so a profile made on one OS was
        // unusable on the other. One implementation, one rule.
        for ok in ["work", "Work Account", "work_2", "a", "A1-b_c d"] {
            assert!(validate_name(ok).is_ok(), "{ok:?} should be valid");
        }
    }

    #[test]
    fn names_reject_path_tricks_and_edges() {
        for bad in ["", " lead", "trail ", "../escape", "a/b", "a\\b", "-dash", "_under", "a:b"] {
            assert!(validate_name(bad).is_err(), "{bad:?} should be rejected");
        }
        assert!(validate_name(&"x".repeat(64)).is_err(), "over-long name should be rejected");
    }

    #[test]
    fn root_honours_the_env_override() {
        let _g = super::env_guard();
        std::env::set_var("MULTIAPP_HOME", "/tmp/multiapp-test-root");
        assert_eq!(root().unwrap(), std::path::PathBuf::from("/tmp/multiapp-test-root"));
        std::env::remove_var("MULTIAPP_HOME");
    }

    /// The regression: a root reached through a symlinked ancestor, with a child that does not
    /// exist yet. macOS puts every temp dir behind the /var -> /private/var symlink, so this is the
    /// ordinary case there, not an exotic one.
    #[test]
    fn containment_allows_a_new_child_under_a_symlinked_root() {
        let _g = super::env_guard();
        let base = std::env::temp_dir().join(format!("ma-sym-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("real")).unwrap();
        std::env::set_var("MULTIAPP_HOME", base.join("real"));
        let child = base.join("real").join("Profiles").join("App").join("brand-new");
        assert!(child.canonicalize().is_err(), "the child must NOT exist for this test to mean anything");
        assert!(assert_inside_root(&child).is_ok(), "a new child inside the root must be allowed");
        assert!(assert_inside_root(&base.join("elsewhere")).is_err(), "outside the root must still be refused");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn containment_guard_rejects_outside_paths() {
        let _g = super::env_guard();
        std::env::set_var("MULTIAPP_HOME", "/tmp/multiapp-test-root");
        assert!(assert_inside_root(std::path::Path::new("/tmp/multiapp-test-root/Profiles/a")).is_ok());
        assert!(assert_inside_root(std::path::Path::new("/tmp/somewhere-else")).is_err());
        assert!(assert_inside_root(std::path::Path::new("/")).is_err());
        std::env::remove_var("MULTIAPP_HOME");
    }

    #[test]
    fn data_dir_layout_is_stable() {
        let _g = super::env_guard();
        std::env::set_var("MULTIAPP_HOME", "/tmp/multiapp-test-root");
        let d = profile_data_dir("claude", "Work Account").unwrap();
        assert!(d.ends_with("Profiles/claude/Work Account/data") || d.ends_with(r"Profiles\claude\Work Account\data"));
        std::env::remove_var("MULTIAPP_HOME");
    }
}

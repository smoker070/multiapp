//! Per-OS locations. One place, so a wrong root can't be introduced piecemeal.
use crate::Error;
use std::path::PathBuf;

/// Root for all Multiapp state.
///
/// Windows uses **LocalAppData**, not Roaming: profiles are gigabytes (Claude alone downloads ~11 GB
/// of Cowork bundles per profile), and Roaming is synced by enterprise roaming-profile policy — that
/// combination is how you fill a fileserver and make logins take ten minutes.
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
    let root = root()?;
    let root_c = root.canonicalize().unwrap_or(root);
    // canonicalize fails on a path that does not exist yet, so fall back to the literal path
    let p_c = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    if p_c.starts_with(&root_c) {
        Ok(())
    } else {
        Err(Error::OutsideRoot(p_c.display().to_string()))
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
        std::env::set_var("MULTIAPP_HOME", "/tmp/multiapp-test-root");
        assert_eq!(root().unwrap(), std::path::PathBuf::from("/tmp/multiapp-test-root"));
        std::env::remove_var("MULTIAPP_HOME");
    }

    #[test]
    fn containment_guard_rejects_outside_paths() {
        std::env::set_var("MULTIAPP_HOME", "/tmp/multiapp-test-root");
        assert!(assert_inside_root(std::path::Path::new("/tmp/multiapp-test-root/Profiles/a")).is_ok());
        assert!(assert_inside_root(std::path::Path::new("/tmp/somewhere-else")).is_err());
        assert!(assert_inside_root(std::path::Path::new("/")).is_err());
        std::env::remove_var("MULTIAPP_HOME");
    }

    #[test]
    fn data_dir_layout_is_stable() {
        std::env::set_var("MULTIAPP_HOME", "/tmp/multiapp-test-root");
        let d = profile_data_dir("claude", "Work Account").unwrap();
        assert!(d.ends_with("Profiles/claude/Work Account/data") || d.ends_with(r"Profiles\claude\Work Account\data"));
        std::env::remove_var("MULTIAPP_HOME");
    }
}

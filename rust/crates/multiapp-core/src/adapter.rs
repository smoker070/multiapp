//! How a profile directory is handed to an app — and whether an app can take one at all.
//!
//! A profile is only worth having when it gives the app a SEPARATE LOGIN. That needs a switch the
//! app itself honours for "keep everything in this folder". Three families have one:
//!
//!  * Chromium / Electron      `--user-data-dir=<dir>`
//!  * Firefox                  `-profile <dir> -no-remote`
//!  * Telegram Desktop         `-many -workdir <dir>/`
//!
//! Everything else is refused, on purpose. A sandboxed app (Telegram for macOS, Gemini, anything
//! from the Mac App Store) keeps its login in a container the system keys by bundle id: a second
//! window is the same account. Offering a profile for it would be a lie that only shows after the
//! user has set it up.
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Chromium,
    Firefox,
    Tdesktop,
}

impl Kind {
    /// The arguments that point the app at `data_dir`.
    pub fn args(self, data_dir: &Path) -> Vec<String> {
        let d = data_dir.display().to_string();
        match self {
            Kind::Chromium => vec![format!("--user-data-dir={d}")], // equals form, always
            Kind::Firefox => vec!["-profile".into(), d, "-no-remote".into()],
            // tdesktop wants the trailing separator, and `-many` or it hands off to the running copy
            Kind::Tdesktop => vec!["-many".into(), "-workdir".into(), format!("{d}/")],
        }
    }
}

/// Does this command line belong to the profile at `data_dir`? Value match, never a prefix.
pub fn cmd_matches(cmd: &[String], data_dir: &str, same: impl Fn(&str, &str) -> bool) -> bool {
    cmd.iter().enumerate().any(|(i, a)| {
        if let Some(v) = a.strip_prefix("--user-data-dir=") {
            return same(v, data_dir);
        }
        matches!(a.as_str(), "-profile" | "--profile" | "-workdir")
            && cmd.get(i + 1).is_some_and(|v| same(v, data_dir))
    })
}

/// A helper process, not the one to ask to quit.
pub fn is_child(cmd: &[String]) -> bool {
    cmd.iter().any(|a| a.starts_with("--type=") || a == "-contentproc")
}

/// Which family an executable name belongs to, for Windows and Linux where the path IS the binary.
fn kind_by_exe(app: &Path) -> Kind {
    let stem = app.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
    match stem.as_str() {
        "firefox" | "firefox-esr" => Kind::Firefox,
        "telegram" | "telegram-desktop" => Kind::Tdesktop,
        _ => Kind::Chromium,
    }
}

/// How to launch `app` into a profile. `None` = this app cannot hold a separate login.
///
/// macOS reads the bundle; elsewhere the executable name decides and unknown names are treated
/// as Chromium, which is what every other launchable entry on those systems is.
pub fn kind_of(app: &Path) -> Option<Kind> {
    if !cfg!(target_os = "macos") {
        return Some(kind_by_exe(app));
    }
    // Mac App Store builds are sandboxed: the container is fixed, whatever the runtime.
    if app.join("Contents/_MASReceipt").exists() {
        return None;
    }
    let bid = plist(app, "CFBundleIdentifier").unwrap_or_default();
    if bid == "com.tdesktop.Telegram" {
        return Some(Kind::Tdesktop);
    }
    if bid.starts_with("org.mozilla.") {
        return Some(Kind::Firefox);
    }
    // Apps that carry the runtime but override where they keep data, so the switch is ignored.
    // (HDRezka-Client sets its own userData path; Arc manages its own profiles.)
    let stem = app.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    // ChatGPT honours the switch for its data, but its ACCOUNT sits in the Keychain and is shared
    // by every profile: signing out in one signs out all. Tested earlier in this project.
    if stem == "HDRezka-Client"
        || bid == "company.thebrowser.Browser"
        || bid.starts_with("com.openai.")
        || bid.starts_with("com.multiapp.")
    {
        return None;
    }
    let electron = app.join("Contents/Frameworks/Electron Framework.framework").exists();
    let browser = !electron && plist(app, "NSPrincipalClass").as_deref() == Some("BrowserCrApplication");
    if (electron || browser) && !sandboxed(app) {
        return Some(Kind::Chromium);
    }
    None
}

/// Should the New Profile list offer this app?
///
/// Stricter than `kind_of`: a separate profile is pointless for an app nobody signs in to. An
/// Electron app that has been used on this machine and left no trace of a session (no cookies, no
/// web storage, no account file) is such an app, and is left out. One that has never been opened
/// has no data to judge by, so it stays.
pub fn offered(app: &Path) -> Option<Kind> {
    let kind = kind_of(app)?;
    let electron = app.join("Contents/Frameworks/Electron Framework.framework").exists();
    if kind == Kind::Chromium && electron {
        let name = app.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let used = crate::appdata::dirs_for(name).map(|d| !d.is_empty()).unwrap_or(false);
        let session = crate::appdata::evidence_for(name).unwrap_or(crate::appdata::Evidence::None);
        if used && session == crate::appdata::Evidence::None {
            return None;
        }
    }
    Some(kind)
}

#[cfg(target_os = "macos")]
fn plist(app: &Path, key: &str) -> Option<String> {
    let out = std::process::Command::new("/usr/libexec/PlistBuddy")
        .arg("-c")
        .arg(format!("Print :{key}"))
        .arg(app.join("Contents/Info.plist"))
        .output()
        .ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}
#[cfg(not(target_os = "macos"))]
fn plist(_app: &Path, _key: &str) -> Option<String> {
    None
}

#[cfg(target_os = "macos")]
fn sandboxed(app: &Path) -> bool {
    std::process::Command::new("/usr/bin/codesign")
        .args(["-d", "--entitlements", "-", "--xml"])
        .arg(app)
        .output()
        .map(|o| {
            let s = String::from_utf8_lossy(&o.stdout);
            // <key>com.apple.security.app-sandbox</key><true/>
            s.split("com.apple.security.app-sandbox</key>")
                .nth(1)
                .is_some_and(|rest| rest.trim_start().starts_with("<true"))
        })
        .unwrap_or(false)
}
#[cfg(not(target_os = "macos"))]
fn sandboxed(_app: &Path) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }
    fn same(a: &str, b: &str) -> bool {
        a.trim_end_matches('/') == b.trim_end_matches('/')
    }

    #[test]
    fn each_family_gets_its_own_switch() {
        let d = Path::new("/p/data");
        assert_eq!(Kind::Chromium.args(d), v(&["--user-data-dir=/p/data"]));
        assert_eq!(Kind::Firefox.args(d), v(&["-profile", "/p/data", "-no-remote"]));
        assert_eq!(Kind::Tdesktop.args(d), v(&["-many", "-workdir", "/p/data/"]));
    }

    #[test]
    fn a_running_profile_is_recognised_in_every_family() {
        for k in [Kind::Chromium, Kind::Firefox, Kind::Tdesktop] {
            let mut cmd = v(&["/x/app"]);
            cmd.extend(k.args(Path::new("/p/work/data")));
            assert!(cmd_matches(&cmd, "/p/work/data", same), "{k:?}");
            assert!(!cmd_matches(&cmd, "/p/work2/data", same), "{k:?} must not match a sibling");
            assert!(!cmd_matches(&cmd, "/p/work", same), "{k:?} must not match a parent");
        }
    }

    #[test]
    fn helpers_are_not_the_main_process() {
        assert!(is_child(&v(&["x", "--type=renderer"])));
        assert!(is_child(&v(&["firefox", "-contentproc", "-profile", "/p"])));
        assert!(!is_child(&v(&["firefox", "-profile", "/p", "-no-remote"])));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_folder_that_is_not_an_app_gets_no_profile() {
        assert_eq!(kind_of(Path::new("/nonexistent/Nothing.app")), None);
    }
}

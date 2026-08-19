//! Launching an app into an isolated profile.
//!
//! The whole project rests on one flag: `--user-data-dir=<dir>`. The EQUALS form is mandatory —
//! Chromium's switch parser silently ignores the space-separated form, which cost a day to discover.
use crate::{paths, Error};
use std::path::Path;
use std::process::Command;

/// Resolve a user-supplied app reference to something launchable.
/// macOS: a `.app` bundle path. Windows/Linux: an executable path.
pub fn resolve_app(reference: &str) -> Result<std::path::PathBuf, Error> {
    let p = Path::new(reference);
    if p.exists() {
        return Ok(p.to_path_buf());
    }
    #[cfg(target_os = "macos")]
    for base in ["/Applications", "/Applications/Utilities"] {
        let cand = Path::new(base).join(format!("{reference}.app"));
        if cand.exists() {
            return Ok(cand);
        }
    }
    #[cfg(target_os = "windows")]
    {
        for var in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
            if let Ok(base) = std::env::var(var) {
                for sub in ["", "Programs"] {
                    let cand = Path::new(&base).join(sub).join(reference)
                        .join(format!("{reference}.exe"));
                    if cand.exists() {
                        return Ok(cand);
                    }
                }
            }
        }
    }
    Err(Error::AppNotFound(reference.to_string()))
}

pub fn launch(app: &Path, data_dir: &Path, extra: &[String]) -> Result<(), Error> {
    std::fs::create_dir_all(data_dir).map_err(Error::Io)?;
    let flag = format!("--user-data-dir={}", data_dir.display()); // equals form, always

    #[cfg(target_os = "macos")]
    {
        // macOS alone needs `open -n`: Launch Services would otherwise just focus the running copy.
        let mut c = Command::new("open");
        c.arg("-n").arg(app).arg("--args").arg(&flag).args(extra);
        let st = c.status().map_err(Error::Io)?;
        if !st.success() {
            return Err(Error::LaunchFailed(format!("open exited with {st}")));
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Windows and Linux have no one-instance-per-app rule to defeat: running it again is enough.
        Command::new(app).arg(&flag).args(extra).spawn().map_err(Error::Io)?;
    }
    let _ = paths::root();
    Ok(())
}

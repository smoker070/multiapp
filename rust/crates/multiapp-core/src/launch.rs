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
    if cfg!(target_os = "windows") {
        if let Some(p) = resolve_app_windows(reference) {
            return Ok(p);
        }
    }
    Err(Error::AppNotFound(reference.to_string()))
}

/// Windows install layouts, in the order they actually occur. Compiled on EVERY platform on purpose:
/// gating it with `#[cfg]` meant a typo here could only be discovered on a Windows machine, which is
/// the one machine this project could not reach.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn resolve_app_windows(reference: &str) -> Option<std::path::PathBuf> {
    let bases: Vec<std::path::PathBuf> = ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .map(std::path::PathBuf::from)
        .collect();

    // Chromium browsers hide the executable under Application\ and never match the name pattern
    // below — "Chrome" lives at Google\Chrome\Application\chrome.exe, not Chrome\Chrome.exe.
    const KNOWN: &[(&str, &str)] = &[
        ("edge", r"Microsoft\Edge\Application\msedge.exe"),
        ("msedge", r"Microsoft\Edge\Application\msedge.exe"),
        ("microsoft edge", r"Microsoft\Edge\Application\msedge.exe"),
        ("chrome", r"Google\Chrome\Application\chrome.exe"),
        ("google chrome", r"Google\Chrome\Application\chrome.exe"),
        ("brave", r"BraveSoftware\Brave-Browser\Application\brave.exe"),
    ];
    let lower = reference.to_lowercase();
    for (key, rel) in KNOWN {
        if lower == *key {
            for b in &bases {
                let cand = b.join(rel);
                if cand.exists() {
                    return Some(cand);
                }
            }
        }
    }
    // electron-builder per-user (%LOCALAPPDATA%\Programs\Notion\Notion.exe) and per-machine installs
    for b in &bases {
        for sub in ["", "Programs"] {
            let cand = b.join(sub).join(reference).join(format!("{reference}.exe"));
            if cand.exists() {
                return Some(cand);
            }
        }
    }
    None
}

pub fn launch(app: &Path, data_dir: &Path, extra: &[String]) -> Result<(), Error> {
    std::fs::create_dir_all(data_dir).map_err(Error::Io)?;
    let flag = format!("--user-data-dir={}", data_dir.display()); // equals form, always

    if cfg!(target_os = "macos") {
        // macOS alone needs `open -n`: Launch Services would otherwise just focus the running copy.
        // `open` normally returns immediately. It does NOT when there is no window server: CI hung
        // here for ten minutes. Bound the wait so a stuck LaunchServices is reported, not endured.
        let mut child = Command::new("open")
            .arg("-n").arg(app).arg("--args").arg(&flag).args(extra)
            .spawn().map_err(Error::Io)?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            match child.try_wait().map_err(Error::Io)? {
                Some(st) if st.success() => break,
                Some(st) => return Err(Error::LaunchFailed(format!("open exited with {st}"))),
                None if std::time::Instant::now() >= deadline => {
                    let _ = child.kill();
                    return Err(Error::LaunchFailed(
                        "`open` did not return within 30s — no window server?".into(),
                    ));
                }
                None => std::thread::sleep(std::time::Duration::from_millis(100)),
            }
        }
    } else {
        // Windows and Linux have no one-instance-per-app rule to defeat: running it again is enough.
        //
        // The child's stdio MUST be detached. Inheriting it ties the browser to whatever started
        // multiapp: launched over SSH, Edge held the connection's pipe open and ssh never returned —
        // a ten-minute hang that looked like a test-harness problem and was not. The same applies to
        // any terminal, script or CI step: the shell would appear to freeze until the browser quits.
        // macOS avoids this only because `open -n` hands the launch to LaunchServices.
        Command::new(app)
            .arg(&flag)
            .args(extra)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(Error::Io)?;
    }
    let _ = paths::root();
    Ok(())
}

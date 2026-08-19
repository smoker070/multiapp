//! End-to-end test against a REAL Chromium app: launch into an isolated profile, confirm the app
//! wrote there, confirm we can find its process by profile, then stop it gracefully.
//!
//! This is the test that answers "does Multiapp actually work on Windows?" — it runs on the
//! windows-latest CI runner against Edge, which is pre-installed and is Chromium.
//! It skips (rather than fails) when no suitable app is present, so `cargo test` stays green on a
//! bare machine.
use multiapp_core::{proc, profile};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// A Chromium-family app we can drive. Edge on Windows, Chrome/anything on macOS/Linux.
fn find_app() -> Option<PathBuf> {
    let candidates: Vec<PathBuf> = if cfg!(target_os = "windows") {
        ["ProgramFiles(x86)", "ProgramFiles"]
            .iter()
            .filter_map(|v| std::env::var(v).ok())
            .flat_map(|b| {
                [
                    Path::new(&b).join(r"Microsoft\Edge\Application\msedge.exe"),
                    Path::new(&b).join(r"Google\Chrome\Application\chrome.exe"),
                ]
            })
            .collect()
    } else if cfg!(target_os = "macos") {
        vec![
            PathBuf::from("/Applications/Google Chrome.app"),
            PathBuf::from("/Applications/OpenMTP.app"),
        ]
    } else {
        ["/usr/bin/google-chrome", "/usr/bin/chromium", "/usr/bin/chromium-browser"]
            .iter().map(PathBuf::from).collect()
    };
    candidates.into_iter().find(|p| p.exists())
}

#[test]
fn launch_isolates_then_stops_gracefully() {
    let Some(app) = find_app() else {
        eprintln!("skipping: no Chromium-family app installed on this machine");
        return;
    };

    // keep every artefact inside a temp root so the test can never touch real profiles
    let root = std::env::temp_dir().join(format!("multiapp-it-{}", std::process::id()));
    std::env::set_var("MULTIAPP_HOME", &root);
    let _ = std::fs::remove_dir_all(&root);

    let app_key = "testapp";
    let name = "it profile";           // deliberately contains a space
    let sibling = "it profile 2";      // and a name the first is a PREFIX of

    profile::create(app_key, name).expect("create");
    profile::create(app_key, sibling).expect("create sibling");

    let data = multiapp_core::paths::profile_data_dir(app_key, name).unwrap();
    let sib_data = multiapp_core::paths::profile_data_dir(app_key, sibling).unwrap();

    // --- launch -------------------------------------------------------------------------------
    let mut extra = vec!["--no-first-run".to_string(), "--no-default-browser-check".to_string()];
    if !cfg!(target_os = "macos") {
        // A CI runner has no real GPU; Chromium's GPU init can stall there.
        extra.push("--disable-gpu".to_string());
    }
    eprintln!("app  = {}", app.display());
    eprintln!("data = {}", data.display());
    profile::launch_profile(app.to_str().unwrap(), app_key, name, &extra).expect("launch");

    // give it time to start and write its profile
    let mut wrote = false;
    for _ in 0..40 {
        std::thread::sleep(Duration::from_millis(500));
        if std::fs::read_dir(&data).map(|d| d.count() > 0).unwrap_or(false) {
            wrote = true;
            break;
        }
    }
    if !wrote {
        // Distinguish the two very different failures: the app never started, versus it started but
        // ignored the flag. Without this a CI failure is unactionable from a log alone.
        let s = proc::sweep(&data);
        eprintln!("DIAGNOSTIC: no files in the profile dir after 20s");
        eprintln!("  processes bound to this profile : {:?}", s.pids);
        eprintln!("  profile dir exists              : {}", data.exists());
        panic!("the app wrote nothing into {} — it either never launched, or ignored --user-data-dir",
               data.display());
    }

    // --- the profile is found, and its PREFIX SIBLING is not --------------------------------
    let s = proc::sweep(&data);
    assert!(s.is_running(), "sweep did not find the process we just launched");
    let sib = proc::sweep(&sib_data);
    assert!(!sib.is_running(), "a profile whose name is a prefix was wrongly reported as running");

    // --- stop ---------------------------------------------------------------------------------
    let before = proc::sweep(&data).pids;
    let stopped = profile::stop(app_key, name, 30).expect("stop");
    if !stopped {
        // Graceful quit is WM_CLOSE on Windows (taskkill without /F) and SIGTERM elsewhere. A process
        // with no top-level window never receives WM_CLOSE, so if this fails on the CI runner the
        // finding is "graceful quit needs an escalation policy on Windows", not "the tool is broken".
        eprintln!("DIAGNOSTIC: graceful quit timed out after 30s");
        eprintln!("  pids before stop : {before:?}");
        eprintln!("  pids still alive : {:?}", proc::sweep(&data).pids);
        panic!("app did not exit after repeated graceful quit requests");
    }
    assert!(!proc::sweep(&data).is_running(), "processes still bound to the profile after stop");

    let _ = std::fs::remove_dir_all(&root);
}

//! Finding and stopping the processes that belong to a profile.
//!
//! Three things learned the hard way, all encoded here:
//!  1. Chromium propagates `--user-data-dir=` to every child process, so the flag's presence proves
//!     nothing about *which* profile owns a process — only the resolved value does.
//!  2. Matching that value as a substring is wrong: `…/work` is a prefix of `…/work2/data`, which
//!     made the shell version report stopped profiles as running.
//!  3. On Windows `sysinfo` returns an EMPTY command line for processes we lack rights to inspect.
//!     Empty must mean "unknown", never "not running" — otherwise we delete a live profile.
use crate::Error;
use std::path::Path;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

pub struct Sweep {
    pub pids: Vec<u32>,
    /// The app's main process. Chromium tags every child with `--type=`, so the one without it is
    /// the parent — and it is the only one worth asking to quit. Signalling the helpers instead just
    /// makes the parent respawn them.
    pub main: Option<u32>,
    /// Processes we could see but not read the command line of, AND whose executable could be
    /// this profile's app. Non-zero means "I cannot be sure this profile is stopped", and
    /// destructive operations must refuse.
    pub opaque: usize,
    /// Their executable names — so a refusal can say what it is waiting on instead of a bare count.
    pub opaque_names: Vec<String>,
}

impl Sweep {
    pub fn is_running(&self) -> bool {
        !self.pids.is_empty()
    }
    pub fn is_certain(&self) -> bool {
        self.opaque == 0
    }
}

fn arg_matches(arg: &str, data_dir: &str) -> bool {
    // exact value match, not a prefix: the flag is `--user-data-dir=<value>` and nothing may follow
    match arg.strip_prefix("--user-data-dir=") {
        Some(v) => paths_equal(v, data_dir),
        None => false,
    }
}

/// Compare paths without demanding they exist. macOS hands out `/tmp` and `/private/tmp` for the same
/// place, and Windows is case-insensitive, so a plain string compare gives false negatives.
fn paths_equal(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        let p = Path::new(s);
        let c = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        let s = c.to_string_lossy().trim_end_matches(['/', '\\']).to_string();
        if cfg!(target_os = "windows") { s.to_lowercase() } else { s }
    };
    norm(a) == norm(b)
}

/// Could a process with this executable name be an instance of `app`?
///
/// Asked only about processes whose arguments cannot be read — on Windows, an elevated process
/// of the same user. Counting EVERY such process as a possible match meant one elevated program
/// of any kind (Task Manager run as admin, an installer) made every profile of every app
/// "uncertain", and clone, rename and delete all refused. It surfaced when GitHub's Windows
/// image started carrying such a process.
///
/// The test is generous on purpose: any word of three letters or more from the app's key that
/// appears in the executable name. `Edge` catches `msedge.exe`, `google-chrome` catches
/// `chrome.exe`, `Telegram Desktop` catches `Telegram.exe`. A false match only makes us more
/// careful; a key with no usable word matches everything, which is the old, safe behaviour.
pub fn could_be(app_key: &str, process_name: &str) -> bool {
    let exe = process_name.to_lowercase();
    let exe = exe.strip_suffix(".exe").unwrap_or(&exe);
    let key = app_key.to_lowercase();
    let mut words = key.split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() >= 3).peekable();
    if words.peek().is_none() {
        return true;
    }
    words.any(|w| exe.contains(w))
}

/// The app a profile data directory belongs to, read from its layout
/// (`…/Profiles/<app>/<profile>/data`). Empty when the path is not shaped like that, which makes
/// `could_be` match everything — a path we do not understand gets the cautious answer.
fn app_of(data_dir: &Path) -> String {
    data_dir
        .parent()
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

pub fn sweep(data_dir: &Path) -> Sweep {
    let want = data_dir.to_string_lossy().to_string();
    let app = app_of(data_dir);
    let mut sys = System::new_with_specifics(
        RefreshKind::new().with_processes(ProcessRefreshKind::everything()),
    );
    sys.refresh_processes(ProcessesToUpdate::All, true);

    // Only processes belonging to US can ever be our profile, and only those are worth flagging as
    // uninspectable. Without this every stopped profile reads "unknown" on macOS, where root daemons
    // are permanently unreadable and always present.
    let me = sysinfo::get_current_pid().ok().and_then(|p| sys.process(p)).and_then(|p| p.user_id().cloned());

    let mut pids = Vec::new();
    let mut main: Option<u32> = None;
    let mut opaque = 0usize;
    let mut opaque_names = Vec::new();
    for (pid, p) in sys.processes() {
        let cmd = p.cmd();
        if cmd.is_empty() {
            // Visible process, unreadable arguments. On Windows this is how an elevated app of the
            // same user appears, and it is silent — count it rather than assuming innocence, but
            // only if it could be this app at all.
            let name = p.name().to_string_lossy().to_string();
            if me.is_some() && p.user_id() == me.as_ref() && could_be(&app, &name) {
                opaque += 1;
                opaque_names.push(name);
            }
            continue;
        }
        if cmd.iter().any(|a| arg_matches(&a.to_string_lossy(), &want)) {
            let is_child = cmd.iter().any(|a| a.to_string_lossy().starts_with("--type="));
            if !is_child {
                main = Some(pid.as_u32());
            }
            pids.push(pid.as_u32());
        }
    }
    Sweep { pids, main, opaque, opaque_names }
}

/// Ask the processes to quit and wait. Never force-kills.
///
/// Windows note: there is no equivalent of macOS's real quit event. `taskkill` without `/F` posts
/// WM_CLOSE to the top-level windows, which many Electron apps treat as "minimise to tray" — so the
/// process can legitimately survive. The caller must surface that instead of hanging.
pub fn quit_and_wait(data_dir: &Path, timeout: std::time::Duration) -> Result<bool, Error> {
    let s = sweep(data_dir);
    if s.pids.is_empty() {
        return Ok(true);
    }

    // Ask only the MAIN process to quit; it tears down its own helpers. Signalling the helpers
    // instead just makes the parent respawn them.
    //
    // The request is REPEATED while we wait. A single signal is not enough: an Electron app that is
    // still starting up drops it silently (verified — the first SIGTERM was delivered successfully,
    // reported Some(true), and was ignored; a second one seconds later ended it immediately).
    let deadline = std::time::Instant::now() + timeout;
    let mut next_signal = std::time::Instant::now();

    while std::time::Instant::now() < deadline {
        let s = sweep(data_dir);
        if !s.is_running() {
            return Ok(true);
        }
        if std::time::Instant::now() >= next_signal {
            let targets: Vec<u32> = match s.main {
                Some(m) => vec![m],
                None => s.pids.clone(),
            };
            request_quit(&targets);
            next_signal = std::time::Instant::now() + std::time::Duration::from_secs(3);
        }
        std::thread::sleep(std::time::Duration::from_millis(400));
    }
    Ok(false)
}

/// Politely ask these processes to exit. Never force-kills: unsaved work belongs to the user.
/// Windows' graceful close: `taskkill` WITHOUT /F, i.e. the polite half of Task Manager's "End task".
/// Compiled on every platform deliberately — see resolve_app_windows for why.
/// Note that many Electron apps treat the resulting WM_CLOSE as "minimise to tray", so the process may
/// legitimately survive; the caller reports that rather than escalating to a force kill.
#[cfg_attr(not(windows), allow(dead_code))]
fn request_quit_windows(pids: &[u32]) {
    for pid in pids {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string()])
            .output();
    }
}

/// Two whole functions rather than one with a runtime branch: on Windows the `cfg!(windows)` version
/// left a bare `return` as the final statement once the Unix arm was compiled out, and clippy's
/// needless_return rejected it under -D warnings. Found by the first CI run on Windows — this file
/// cannot be linted for Windows from a Mac.
#[cfg(windows)]
fn request_quit(pids: &[u32]) {
    request_quit_windows(pids);
}

#[cfg(not(windows))]
fn request_quit(pids: &[u32]) {
    let mut sys = System::new_with_specifics(
        RefreshKind::new().with_processes(ProcessRefreshKind::everything()),
    );
    sys.refresh_processes(ProcessesToUpdate::All, true);
    for pid in pids {
        if let Some(p) = sys.process(sysinfo::Pid::from_u32(*pid)) {
            let r = p.kill_with(sysinfo::Signal::Term); // SIGTERM — Electron flushes its DBs
            if std::env::var("MULTIAPP_DEBUG").is_ok() {
                eprintln!("[debug] SIGTERM pid {pid} -> {r:?}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_exact_profile_only() {
        // The bug this guards: `…/work` is a prefix of `…/work2/data`, so a substring match reported
        // stopped profiles as running and made delete/clone/rename refuse to touch them.
        let dir = "/tmp/Multiapp/Profiles/notion/work/data";
        assert!(arg_matches("--user-data-dir=/tmp/Multiapp/Profiles/notion/work/data", dir));
        assert!(!arg_matches("--user-data-dir=/tmp/Multiapp/Profiles/notion/work2/data", dir));
        assert!(!arg_matches("--user-data-dir=/tmp/Multiapp/Profiles/notion/work", dir));
    }

    #[test]
    fn ignores_other_flags() {
        let dir = "/tmp/p/data";
        assert!(!arg_matches("--disk-cache-dir=/tmp/p/data", dir));
        assert!(!arg_matches("--type=renderer", dir));
        assert!(!arg_matches("/Applications/Foo.app/Contents/MacOS/Foo", dir));
    }

    #[test]
    fn an_unreadable_process_only_counts_if_it_could_be_the_app() {
        // the processes the real apps run as
        assert!(could_be("Edge", "msedge.exe"));
        assert!(could_be("Microsoft Edge", "msedge.exe"));
        assert!(could_be("google-chrome", "chrome.exe"));
        assert!(could_be("Telegram Desktop", "Telegram.exe"));
        assert!(could_be("claude", "Claude"));
        // the case that broke CI: an unrelated elevated process must not block every profile
        assert!(!could_be("Edge", "Taskmgr.exe"));
        assert!(!could_be("notion", "svchost.exe"));
        // a key we cannot read anything from keeps the old, cautious answer
        assert!(could_be("", "Taskmgr.exe"));
        assert!(could_be("ab", "Taskmgr.exe"));
    }

    #[test]
    fn the_app_is_read_from_the_profile_layout() {
        let p = std::path::Path::new("/r/Profiles/google-chrome/work/data");
        assert_eq!(app_of(p), "google-chrome");
        assert_eq!(app_of(std::path::Path::new("data")), "");
    }

    #[test]
    fn a_sweep_of_a_nonexistent_dir_finds_nothing() {
        let s = sweep(std::path::Path::new("/tmp/multiapp-definitely-not-running-xyz/data"));
        assert!(!s.is_running(), "no process should claim this profile");
    }
}

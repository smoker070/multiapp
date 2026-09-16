// The Multiapp desktop UI.
//
// `windows_subsystem = "windows"` is what stops a console window appearing behind the UI on
// Windows. Without it a Tauri app still opens a terminal, which is exactly the complaint that
// prompted this program: "when I open it, it works like a terminal — that is unprofessional".
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use multiapp_core::{appdata, archive, launch, paths, profile};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize)]
struct ProfileDto {
    app: String,
    name: String,
    running: bool,
    /// false when processes existed that we could not inspect — the UI must not claim "stopped"
    certain: bool,
}

#[derive(Serialize)]
struct AppDto {
    name: String,
    path: String,
}

#[tauri::command]
fn list_profiles() -> Result<Vec<ProfileDto>, String> {
    let mut v: Vec<ProfileDto> = profile::list()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|p| ProfileDto { app: p.app, name: p.name, running: p.running, certain: p.certain })
        .collect();
    v.sort_by_key(|p| (p.app.to_lowercase(), p.name.to_lowercase()));
    Ok(v)
}

#[tauri::command]
fn create_profile(app: String, name: String) -> Result<String, String> {
    profile::create(&app, &name)
        .map(|p| p.display().to_string())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn launch_profile(app: String, name: String) -> Result<(), String> {
    profile::launch_profile(&app, &app, &name, &[]).map_err(|e| e.to_string())
}

/// Ask the app to quit. Never force-kills — a refusal is reported to the UI as a message, not as a
/// silent failure, and the profile stays exactly as it was.
#[tauri::command]
fn stop_profile(app: String, name: String) -> Result<bool, String> {
    profile::stop(&app, &name, 20).map_err(|e| e.to_string())
}

#[tauri::command]
fn profiles_root() -> Result<String, String> {
    paths::profiles_root().map(|p| p.display().to_string()).map_err(|e| e.to_string())
}

/// Apps on this machine that are plausible profile targets, so the New Profile dialog can offer a
/// list instead of asking someone to type an exact name. Deliberately permissive: `probe` and the
/// launch itself are what actually decide whether an app honours the flag.
#[tauri::command]
fn detect_apps() -> Vec<AppDto> {
    let mut out: Vec<AppDto> = Vec::new();
    let mut push = |name: String, path: PathBuf| {
        if path.exists() && !out.iter().any(|a| a.name == name) {
            out.push(AppDto { name, path: path.display().to_string() });
        }
    };

    if cfg!(target_os = "macos") {
        for dir in ["/Applications", "/Applications/Utilities"] {
            if let Ok(rd) = std::fs::read_dir(dir) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.extension().and_then(|s| s.to_str()) == Some("app") {
                        if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                            push(stem.to_string(), p.clone());
                        }
                    }
                }
            }
        }
    } else if cfg!(target_os = "windows") {
        // browsers first: they sit under an Application\ subdirectory and never match the
        // <Name>\<Name>.exe pattern that per-user Electron installs use
        for (label, rel) in [
            ("Microsoft Edge", r"Microsoft\Edge\Application\msedge.exe"),
            ("Google Chrome", r"Google\Chrome\Application\chrome.exe"),
            ("Brave", r"BraveSoftware\Brave-Browser\Application\brave.exe"),
        ] {
            for var in ["ProgramFiles(x86)", "ProgramFiles"] {
                if let Ok(b) = std::env::var(var) {
                    push(label.to_string(), PathBuf::from(b).join(rel));
                }
            }
        }
        for var in ["LOCALAPPDATA", "ProgramFiles", "ProgramFiles(x86)"] {
            let Ok(base) = std::env::var(var) else { continue };
            for sub in ["Programs", ""] {
                let root = PathBuf::from(&base).join(sub);
                let Ok(rd) = std::fs::read_dir(&root) else { continue };
                for e in rd.flatten() {
                    let dir = e.path();
                    if !dir.is_dir() {
                        continue;
                    }
                    let Some(nm) = dir.file_name().and_then(|s| s.to_str()) else { continue };
                    let exe = dir.join(format!("{nm}.exe"));
                    if exe.exists() {
                        push(nm.to_string(), exe);
                    }
                }
            }
        }
    } else {
        for c in ["/usr/bin/google-chrome", "/usr/bin/chromium", "/usr/bin/chromium-browser"] {
            let p = PathBuf::from(c);
            if let Some(nm) = p.file_name().and_then(|s| s.to_str()) {
                push(nm.to_string(), p.clone());
            }
        }
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    out
}

/// Does this app reference resolve to something launchable? Lets the dialog say so before a profile
/// is created, rather than failing at first launch.
#[tauri::command]
fn app_exists(app: String) -> bool {
    launch::resolve_app(&app).is_ok()
}


/// Where a Start Menu shortcut for this build would live.
#[cfg(windows)]
fn start_menu_lnk() -> Option<PathBuf> {
    let appdata = std::env::var("APPDATA").ok()?;
    Some(PathBuf::from(appdata).join(r"Microsoft\Windows\Start Menu\Programs\Multiapp.lnk"))
}

/// Is this build already in the Start Menu?
#[tauri::command]
fn in_start_menu() -> bool {
    #[cfg(windows)]
    {
        start_menu_lnk().map(|p| p.exists()).unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Put this build in the Start Menu, on request.
///
/// A portable .exe is not registered anywhere by Windows — only an installer creates Start Menu
/// entries — so running one from the Desktop leaves nothing in the Start Menu. This offers it as an
/// explicit action rather than doing it silently on first run, which would be a surprising thing for
/// a portable program to do to someone's machine.
///
/// The shortcut is written through WScript.Shell rather than by hand: a .lnk is a COM-serialised
/// binary format, and driving IShellLink from here would mean a pile of unsafe FFI for one file.
#[tauri::command]
fn add_to_start_menu() -> Result<String, String> {
    #[cfg(windows)]
    {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let lnk = start_menu_lnk().ok_or("APPDATA is not set")?;
        if let Some(dir) = lnk.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let script = format!(
            "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{lnk}');\
             $s.TargetPath='{exe}';$s.WorkingDirectory='{dir}';$s.IconLocation='{exe},0';\
             $s.Description='Run multiple isolated profiles of the same app';$s.Save()",
            lnk = lnk.display(),
            exe = exe.display(),
            dir = exe.parent().map(|p| p.display().to_string()).unwrap_or_default(),
        );
        let out = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
            .map_err(|e| e.to_string())?;
        if !lnk.exists() {
            return Err(format!(
                "could not create the shortcut: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(lnk.display().to_string())
    }
    #[cfg(not(windows))]
    {
        Err("Start Menu shortcuts are a Windows feature".into())
    }
}


#[tauri::command]
fn clone_profile(app: String, from: String, to: String) -> Result<(), String> {
    profile::clone_profile(&app, &from, &to).map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
fn rename_profile(app: String, old: String, new: String) -> Result<(), String> {
    profile::rename(&app, &old, &new).map(|_| ()).map_err(|e| e.to_string())
}

/// Remove a profile. It is MOVED to Multiapp's Trash, never deleted, and the caller is told where it
/// went so the UI can say so — a profile holds real logins and a misclick has to be recoverable.
#[tauri::command]
fn delete_profile(app: String, name: String, stamp: String) -> Result<String, String> {
    profile::delete_to_trash(&app, &name, &stamp)
        .map(|p| p.display().to_string())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn profile_size(app: String, name: String) -> Result<u64, String> {
    profile::size_bytes(&app, &name).map_err(|e| e.to_string())
}

#[tauri::command]
fn reveal_profile(app: String, name: String) -> Result<(), String> {
    profile::reveal(&app, &name).map_err(|e| e.to_string())
}

/// The macOS bash tool, if it is installed.
///
/// Backup, restore, export/import and session transfer live in that 1600-line script and are built
/// on macOS specifics — Keychain reasoning, `.app` bundles, `~/Library` layouts. They are not ported
/// to Rust and therefore do not exist on Windows, so the UI asks for this before offering them
/// rather than showing buttons that cannot work.
#[cfg(target_os = "macos")]
fn find_bash_cli() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let candidates = [
        PathBuf::from(&home).join(".local/bin/multiapp"),
        PathBuf::from("/usr/local/bin/multiapp"),
        PathBuf::from("/opt/homebrew/bin/multiapp"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

#[tauri::command]
fn advanced_available() -> bool {
    #[cfg(target_os = "macos")]
    {
        find_bash_cli().is_some()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Run one of the macOS-only commands.
///
/// A non-zero exit is an `Err`, not text. Handing back stdout regardless of the exit status is how
/// a failure becomes an answer: the same shape gave "no sessions found" for a week while the real
/// fault was an unrunnable python3. Everything that shells out goes through here.
fn advanced(args: &[&str]) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        // An allowlist, not a passthrough: these arguments come from a web view, and a UI bug must
        // not be able to turn into "run whatever you like".
        const ALLOWED: &[&str] = &[
            "migrate-list", "backup", "restore", "session-check", "session-backup",
            "session-restore", "app-export", "app-import", "list-installed", "sessions",
            "transfer", "accounts", "export", "import", "scan", "probe", "apps", "doctor",
        ];
        let first = args.first().copied().unwrap_or("");
        if !ALLOWED.contains(&first) {
            return Err(format!("'{first}' is not an allowed command"));
        }
        let cli = find_bash_cli().ok_or("the multiapp CLI is not installed")?;
        let out = std::process::Command::new(&cli)
            .args(args)
            .output()
            .map_err(|e| format!("could not run the multiapp CLI: {e}"))?;
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if out.status.success() {
            return Ok(stdout);
        }
        Err(if stderr.is_empty() {
            format!("`multiapp {first}` failed ({})", out.status)
        } else {
            stderr
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = args;
        Err("these commands are macOS-only for now".into())
    }
}

/// Run one of the macOS-only commands and hand back its output verbatim.
#[tauri::command]
fn run_advanced(args: Vec<String>) -> Result<String, String> {
    advanced(&args.iter().map(String::as_str).collect::<Vec<_>>())
}

#[derive(Serialize)]
struct SessionDto {
    /// 1-based position in this profile's list — the selector the CLI itself takes.
    index: usize,
    date: String,
    cwd: String,
    title: String,
    /// The account folder this session is filed under, `<id>/<id>`.
    account: String,
}

/// `.../claude-code-sessions/<a>/<b>/local_x.json` → `<a>/<b>`
fn account_of(index_file: &str) -> String {
    let p = std::path::Path::new(index_file);
    let b = p.parent();
    let a = b.and_then(|b| b.parent());
    match (a.and_then(|a| a.file_name()), b.and_then(|b| b.file_name())) {
        (Some(a), Some(b)) => format!("{}/{}", a.to_string_lossy(), b.to_string_lossy()),
        _ => String::new(),
    }
}

#[derive(Serialize)]
struct AccountDto {
    id: String,
    sessions: usize,
    last: String,
    /// The running Claude of this install wrote here since it started: the signed-in folder.
    live: bool,
    titles: String,
}

/// The account folders a profile keeps sessions in. Claude shows only the signed-in one, so after
/// switching accounts the old sessions are still on disk — in a folder the app no longer opens.
#[tauri::command]
fn claude_accounts(profile: String) -> Result<Vec<AccountDto>, String> {
    let out = advanced(&["accounts", "claude", &profile, "--raw"])?;
    Ok(out
        .lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            (f.len() >= 5).then(|| AccountDto {
                id: f[0].to_string(),
                sessions: f[1].parse().unwrap_or(0),
                last: f[2].to_string(),
                live: f[3] == "1",
                titles: f[4].to_string(),
            })
        })
        .collect())
}

/// Claude Code sessions saved in a profile ("main" is the unprofiled install).
///
/// Sessions are a special case: the index files live inside the profile, but the transcripts live
/// in the real `~/.claude`, outside it. That is why copying a session is a CLI command with its own
/// id-minting logic rather than a file copy, and why this screen exists at all.
#[tauri::command]
fn claude_sessions(profile: String) -> Result<Vec<SessionDto>, String> {
    let out = advanced(&["sessions", "claude", &profile, "--raw"])?;
    Ok(out
        .lines()
        .enumerate()
        .filter_map(|(i, line)| {
            let f: Vec<&str> = line.split('\t').collect();
            (f.len() >= 5).then(|| SessionDto {
                index: i + 1,
                date: f[2].to_string(),
                cwd: f[3].to_string(),
                title: f[4].to_string(),
                account: account_of(f[0]),
            })
        })
        .collect())
}

/// Copy sessions as INDEPENDENT copies (new ids, duplicated transcript). The originals are untouched.
/// `to_account` names the destination account folder; it is what makes a copy within one install
/// meaningful, and the CLI refuses a same-install copy without it.
#[tauri::command]
fn transfer_sessions(
    from: String,
    to: String,
    picks: String,
    to_account: Option<String>,
) -> Result<String, String> {
    let acct = to_account.filter(|a| !a.is_empty());
    if from == to && acct.is_none() {
        return Err("choose which account receives the copies".into());
    }
    let mut args = vec!["transfer", "claude", from.as_str(), to.as_str(), picks.as_str()];
    if let Some(a) = acct.as_deref() {
        args.extend(["--to-account", a]);
    }
    advanced(&args)
}

/// Write chosen sessions to an archive that can be carried to another machine.
#[tauri::command]
fn export_sessions(profile: String, out: String, picks: String) -> Result<String, String> {
    advanced(&["export", "claude", &profile, &out, &picks])
}

#[tauri::command]
fn import_sessions(profile: String, file: String) -> Result<String, String> {
    advanced(&["import", "claude", &profile, &file])
}

/// What session data an app holds and whether it would survive a move to another Mac.
#[tauri::command]
fn session_check(app: String) -> Result<String, String> {
    advanced(&["session-check", &app])
}


#[derive(Serialize)]
struct AppDataDto {
    name: String,
    bytes: u64,
    evidence: String,
    dirs: usize,
}

/// Applications on this machine that hold a session worth keeping.
#[tauri::command]
fn list_app_data() -> Result<Vec<AppDataDto>, String> {
    Ok(appdata::installed()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|a| AppDataDto {
            name: a.name,
            bytes: a.bytes,
            evidence: a.evidence.label().to_string(),
            dirs: a.dirs.len(),
        })
        .collect())
}

fn now_stamp() -> String {
    // no chrono for one string: seconds since the epoch is unique per archive and sorts correctly
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

/// Archive an app's data, or just the files that carry its login.
#[tauri::command]
fn backup_app(app: String, out: String, sessions_only: bool) -> Result<String, String> {
    let dirs = appdata::dirs_for(&app).map_err(|e| e.to_string())?;
    if dirs.is_empty() {
        return Err(format!("no local data found for '{app}'"));
    }
    let s = archive::create(&app, &dirs, std::path::Path::new(&out), sessions_only, false, &now_stamp())
        .map_err(|e| e.to_string())?;
    Ok(format!("{} file(s), {} — {}", s.files, human(s.bytes), s.path.display()))
}

/// What is inside an archive, before anything is written.
#[tauri::command]
fn archive_info(path: String) -> Result<serde_json::Value, String> {
    let m = archive::read_manifest(std::path::Path::new(&path)).map_err(|e| e.to_string())?;
    serde_json::to_value(m).map_err(|e| e.to_string())
}

/// Put an archive back. Merges into what is there; anything overwritten is staged to Trash first.
#[tauri::command]
fn restore_archive(path: String) -> Result<String, String> {
    let s = archive::restore(std::path::Path::new(&path), &now_stamp()).map_err(|e| e.to_string())?;
    let mut msg = format!("{} file(s), {} restored", s.files, human(s.bytes));
    if let Some(t) = s.staged {
        msg.push_str(&format!(" — replaced files kept in {}", t.display()));
    }
    Ok(msg)
}

fn human(b: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{b} B") } else { format!("{v:.1} {}", U[i]) }
}


/// Which sites an app's cookies belong to. Host names only — cookie values are never read.
#[tauri::command]
fn cookie_report(app: String) -> Result<serde_json::Value, String> {
    let c = appdata::cookie_report(&app).map_err(|e| e.to_string())?;
    serde_json::to_value(c).map_err(|e| e.to_string())
}

/// Launch an app into a throwaway directory to find out whether it honours the flag at all.
/// Some apps override their data directory in their own code and simply ignore it.
#[tauri::command]
fn probe_app(app: String) -> Result<serde_json::Value, String> {
    let p = launch::probe(&app, 20).map_err(|e| e.to_string())?;
    serde_json::to_value(p).map_err(|e| e.to_string())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            list_profiles,
            create_profile,
            launch_profile,
            stop_profile,
            profiles_root,
            detect_apps,
            app_exists,
            in_start_menu,
            add_to_start_menu,
            clone_profile,
            rename_profile,
            delete_profile,
            profile_size,
            reveal_profile,
            advanced_available,
            run_advanced,
            claude_sessions,
            claude_accounts,
            transfer_sessions,
            export_sessions,
            import_sessions,
            session_check,
            list_app_data,
            backup_app,
            archive_info,
            restore_archive,
            cookie_report,
            probe_app,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start the Multiapp window");
}

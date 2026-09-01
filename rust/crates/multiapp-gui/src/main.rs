// The Multiapp desktop UI.
//
// `windows_subsystem = "windows"` is what stops a console window appearing behind the UI on
// Windows. Without it a Tauri app still opens a terminal, which is exactly the complaint that
// prompted this program: "when I open it, it works like a terminal — that is unprofessional".
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use multiapp_core::{launch, paths, profile};
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
    v.sort_by(|a, b| (a.app.to_lowercase(), a.name.to_lowercase())
        .cmp(&(b.app.to_lowercase(), b.name.to_lowercase())));
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

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            list_profiles,
            create_profile,
            launch_profile,
            stop_profile,
            profiles_root,
            detect_apps,
            app_exists,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start the Multiapp window");
}

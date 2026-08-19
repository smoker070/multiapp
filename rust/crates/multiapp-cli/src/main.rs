//! multiapp — run multiple isolated profiles of desktop apps.
//! Milestone 0: new / launch / list / stop, one codebase for macOS, Windows and Linux.
use multiapp_core::{paths, profile, Error, VERSION};

fn usage() -> i32 {
    println!(
"multiapp {VERSION} — run multiple isolated profiles of desktop apps

usage:
  multiapp new     <app> <profile>              create a profile
  multiapp launch  <app> <profile> [-- args]    run the app in that profile
  multiapp list                                 profiles and whether they are running
  multiapp stop    <app> <profile>              ask it to quit (never force-kills)
  multiapp where                                show where profiles are stored

<app> is an installed app: a name (\"Notion\"), or a full path to the app/exe.
Profiles are isolated with --user-data-dir=<dir>, which Electron/Chromium apps honour.");
    2
}

fn run() -> Result<i32, Error> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(|s| s.as_str()).unwrap_or("help");

    match cmd {
        "new" => {
            let (app, name) = (args.get(1), args.get(2));
            let (app, name) = match (app, name) {
                (Some(a), Some(n)) => (a, n),
                _ => return Ok(usage()),
            };
            let dir = profile::create(app, name)?;
            println!("created {app}/{name}");
            println!("  {}", dir.display());
            println!("launch it:  multiapp launch \"{app}\" \"{name}\"");
            Ok(0)
        }
        "launch" => {
            let (app, name) = match (args.get(1), args.get(2)) {
                (Some(a), Some(n)) => (a, n),
                _ => return Ok(usage()),
            };
            let extra: Vec<String> = args.iter().skip(3).skip_while(|a| *a == "--").cloned().collect();
            profile::launch_profile(app, app, name, &extra)?;
            println!("launched {app}/{name}");
            Ok(0)
        }
        "list" => {
            let ps = profile::list()?;
            if ps.is_empty() {
                println!("no profiles yet — create one:  multiapp new <app> <name>");
                return Ok(0);
            }
            println!("{:<18} {:<22} STATE", "APP", "PROFILE");
            for p in ps {
                let state = if p.running {
                    "running"
                } else if !p.certain {
                    "unknown"   // processes existed that we could not inspect
                } else {
                    "stopped"
                };
                println!("{:<18} {:<22} {}", p.app, p.name, state);
            }
            Ok(0)
        }
        "stop" => {
            let (app, name) = match (args.get(1), args.get(2)) {
                (Some(a), Some(n)) => (a, n),
                _ => return Ok(usage()),
            };
            if profile::stop(app, name, 15)? {
                println!("{app}/{name} stopped");
                Ok(0)
            } else {
                // Windows especially: many Electron apps treat a close request as "minimise to tray"
                eprintln!("{app}/{name} did not exit. Quit it from the app itself;");
                eprintln!("multiapp will not force-kill it, because that loses unsaved data.");
                Ok(1)
            }
        }
        "where" => {
            println!("{}", paths::profiles_root()?.display());
            Ok(0)
        }
        "--version" | "version" => {
            println!("multiapp {VERSION} ({})", std::env::consts::OS);
            Ok(0)
        }
        _ => Ok(usage()),
    }
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("multiapp: {e}");
            std::process::exit(1);
        }
    }
}

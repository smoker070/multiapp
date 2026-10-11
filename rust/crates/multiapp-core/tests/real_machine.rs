//! Not an assertion suite — a comparison against the machine this runs on.
//! `cargo test --test real_machine -- --nocapture`
use multiapp_core::appdata;

#[test]
fn what_this_machine_holds() {
    let apps = appdata::installed().expect("scan");
    println!("\n{:<24} {:<22} {:>9}", "APP", "SESSION", "SIZE");
    for a in &apps {
        // installed() no longer measures: sizes are asked for one app at a time, as the GUI does.
        let mb = appdata::size_of(&a.name).unwrap_or(0) as f64 / 1_048_576.0;
        println!("{:<24} {:<22} {:>8.1}M", a.name, a.evidence.label(), mb);
    }
    println!("\n{} app(s) with a saved session", apps.len());
}

#[test]
fn cookie_hosts_for_the_apps_that_were_cross_checked() {
    for app in ["Claude", "Notion", "Notion Calendar", "hdrezka-client", "Firefox"] {
        match appdata::cookie_report(app) {
            Ok(c) if c.total > 0 => {
                println!("\n{app}: {} cookies ({} encrypted, {} plaintext) across {} host(s)",
                         c.total, c.encrypted, c.plaintext, c.hosts.len());
                for (h, n) in c.hosts.iter().take(5) {
                    println!("   {n:>4}  {h}");
                }
            }
            Ok(_) => println!("\n{app}: no cookies"),
            Err(e) => println!("\n{app}: {e}"),
        }
    }
}

/// Where the Back Up window's wait actually goes, per app. Run it when that window feels slow —
/// guessing which half is expensive is how the 15 GB byte-sum survived a release.
#[test]
fn how_long_the_scan_costs_per_app() {
    use std::time::Instant;
    let t0 = Instant::now();
    let apps = appdata::installed().expect("scan");
    let whole = t0.elapsed();

    let mut rows: Vec<(u128, u128, String)> = Vec::new();
    for a in &apps {
        let t = Instant::now();
        let _ = appdata::evidence_for(&a.name);
        let ev = t.elapsed().as_millis();
        let t = Instant::now();
        let _ = appdata::session_files(&a.dirs[0]);
        rows.push((ev, t.elapsed().as_millis(), a.name.clone()));
    }
    rows.sort_by_key(|r| std::cmp::Reverse(r.0));

    println!("\n{:<24} {:>10} {:>14}", "APP", "evidence", "session_files");
    for (ev, sf, name) in rows.iter().take(12) {
        println!("{name:<24} {ev:>8}ms {sf:>12}ms");
    }
    println!("\ninstalled() total: {} ms for {} apps", whole.as_millis(), apps.len());
}

/// Every folder in the scan root, timed — including the ones that are rejected. The accepted apps
/// are a small minority, so timing only those answers the wrong question.
#[test]
fn how_long_the_rejected_folders_cost() {
    use std::time::Instant;
    let h = directories::BaseDirs::new().unwrap().home_dir().to_path_buf();
    let root = h.join("Library/Application Support");
    let names: Vec<String> = std::fs::read_dir(&root).unwrap().flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
        .filter(|n| !n.starts_with('.'))
        .collect();

    let mut rows: Vec<(u128, String)> = Vec::new();
    let t0 = Instant::now();
    for n in &names {
        let t = Instant::now();
        let _ = appdata::evidence_for(n);
        rows.push((t.elapsed().as_millis(), n.clone()));
    }
    let total = t0.elapsed().as_millis();
    rows.sort_by_key(|r| std::cmp::Reverse(r.0));
    println!("\n{} folders scanned, {} ms total", names.len(), total);
    println!("\nslowest 15:");
    for (ms, n) in rows.iter().take(15) { println!("{ms:>6}ms  {n}"); }
    let tail: u128 = rows.iter().skip(15).map(|r| r.0).sum();
    println!("remaining {} folders: {} ms", rows.len().saturating_sub(15), tail);
}

/// Which data folders belong to an app that is actually installed here — the Sessions picker's
/// list — and which are leftovers. Read it when the picker shows something surprising.
#[test]
fn which_session_folders_still_have_their_app() {
    let home = directories::BaseDirs::new().unwrap().home_dir().to_path_buf();
    let mut installed: Vec<String> = Vec::new();
    for dir in [std::path::PathBuf::from("/Applications"), home.join("Applications")] {
        let Ok(rd) = std::fs::read_dir(dir) else { continue };
        for p in rd.flatten().map(|e| e.path()) {
            if p.extension().and_then(|s| s.to_str()) == Some("app") {
                installed.push(p.file_stem().unwrap().to_string_lossy().to_string());
            }
        }
    }
    println!();
    for a in appdata::installed().expect("scan") {
        match appdata::owner_of(&a.name, &installed) {
            Some(app) => println!("  KEEP  {:<20} -> {app}", a.name),
            None => println!("  skip  {:<20} (no installed app)", a.name),
        }
    }
}

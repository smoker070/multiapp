//! Not an assertion suite — a comparison against the machine this runs on.
//! `cargo test --test real_machine -- --nocapture`
use multiapp_core::appdata;

#[test]
fn what_this_machine_holds() {
    let apps = appdata::installed().expect("scan");
    println!("\n{:<24} {:<22} {:>9}", "APP", "SESSION", "SIZE");
    for a in &apps {
        println!("{:<24} {:<22} {:>8.1}M", a.name, a.evidence.label(), a.bytes as f64 / 1_048_576.0);
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

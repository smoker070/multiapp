//! Not an assertion suite — a comparison against the machine this runs on. `cargo test --test
//! real_machine -- --nocapture` prints what the ported discovery sees, so it can be held against
//! what the original shell tool reported for the same machine.
use multiapp_core::appdata;

#[test]
fn what_this_machine_holds() {
    let apps = appdata::installed().expect("scan");
    println!("\n{:<26} {:<22} {:>10}", "APP", "SESSION", "SIZE");
    for a in &apps {
        let mb = a.bytes as f64 / 1_048_576.0;
        println!("{:<26} {:<22} {:>9.1}M", a.name, a.evidence.label(), mb);
    }
    println!("\n{} app(s) with a saved session\n", apps.len());
}

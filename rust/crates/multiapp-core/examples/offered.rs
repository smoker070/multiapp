//! Prints what the New Profile list would offer on this machine, and why the rest is left out.
use multiapp_core::adapter;
fn main() {
    let mut apps: Vec<_> = ["/Applications", "/Applications/Utilities"]
        .iter()
        .filter_map(|d| std::fs::read_dir(d).ok())
        .flat_map(|rd| rd.flatten().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("app"))
        .collect();
    apps.sort();
    for p in apps {
        let name = p.file_stem().unwrap().to_string_lossy().to_string();
        match (adapter::kind_of(&p), adapter::offered(&p)) {
            (_, Some(k)) => println!("OFFER  {name}  ({k:?})"),
            (Some(k), None) => println!("skip   {name}  ({k:?}, but no login found in its data)"),
            (None, None) => println!("skip   {name}"),
        }
    }
}

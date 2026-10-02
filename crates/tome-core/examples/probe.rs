//! cargo run -p tome-core --example probe -- <working copy> [branch]
fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("a working copy path");
    let branch = args.next().unwrap_or_default();
    let mut repo = tome_core::Repository::open(path);
    repo.offline = true;
    for (name, result) in [("status", repo.status()), ("branches", repo.branches()), ("history", repo.history(&branch, 5))] {
        println!("== {name}: status {} {}", result.status, result.error);
        for event in result.events.iter().filter(|e| e["tagName"] != "log") {
            println!("  {event}");
        }
    }
}

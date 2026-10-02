//! Prints a working copy's graph (every branch) as text, to check the Smartlog lanes:
//! `cargo run -p tome-core --example graph -- <working copy> [length] [--json]`
fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("a working copy path");
    let length = args.next().and_then(|n| n.parse().ok()).unwrap_or(40);
    let mut repo = tome_core::Repository::open(path);
    repo.offline = true;
    let graph = repo.graph(length).expect("graph");
    for branch in &graph.incomplete {
        eprintln!("incomplete: {branch}");
    }
    if std::env::args().any(|a| a == "--json") {
        let status = tome_core::model::status(&repo.status()).expect("status");
        let branches = tome_core::model::branches(&repo.branches());
        println!("{}", serde_json::json!({ "graph": graph, "status": status, "branches": branches }));
        return;
    }
    let rows = graph.rows;
    let text = tome_core::graph::ascii(&rows, |r| format!("r{} {}", r.number, r.message.lines().next().unwrap_or("")));
    println!("{text}");
}

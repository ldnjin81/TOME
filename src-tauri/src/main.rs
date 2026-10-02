// Prevents an additional console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `tome --lore-worker <job json>`: run one long Lore operation and print its progress
    // (the window starts itself this way so the operation can be cancelled).
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--lore-worker") {
        std::process::exit(tome_core::ops::worker_main(args.get(2).map(String::as_str).unwrap_or("")));
    }
    tome_lib::run();
}

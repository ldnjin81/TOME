// Prevents an additional console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Keep Lore's stores loaded between calls (Lore 0.11 LORE_STORE_RETAIN): TOME makes many
    // calls in a row, and this skips reopening the store each time (20 status calls on a game
    // project: 315 ms -> 46 ms). Lore asks not to share a store with processes of a build
    // older than 0.11, which the CLI kept current alongside TOME isn't. A value already set wins.
    if std::env::var_os("LORE_STORE_RETAIN").is_none() {
        // SAFETY: first thing in main, before any thread exists.
        unsafe { std::env::set_var("LORE_STORE_RETAIN", "1") };
    }
    // `tome --lore-worker <job json>`: run one long Lore operation and print its progress
    // (the window starts itself this way so the operation can be cancelled).
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--lore-worker") {
        std::process::exit(tome_core::ops::worker_main(args.get(2).map(String::as_str).unwrap_or("")));
    }
    tome_lib::run();
}

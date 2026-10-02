//! The worker mode as its own program, for the tests: `tome-worker '<job json>'`.
fn main() {
    let job = std::env::args().nth(1).unwrap_or_default();
    std::process::exit(tome_core::ops::worker_main(&job));
}

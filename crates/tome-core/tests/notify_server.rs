//! Notifications on a real Lore server: a working copy subscribed to the repository hears the
//! locks another working copy takes and releases, and a push. Opt-in like `server.rs`.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tome_core::notify::Notification;
use tome_core::{CallResult, Repository, model};

fn server() -> Option<String> {
    let server = std::env::var("TOME_TEST_SERVER").ok().filter(|s| !s.is_empty())?;
    assert!(!server.contains(":41337"), "TOME_TEST_SERVER {server} looks like the team server");
    Some(server)
}

fn ok(result: CallResult, what: &str) -> CallResult {
    assert!(result.ok(), "{what} failed: status {} {}", result.status, result.error);
    result
}

/// Waits up to 10 s for a notification of `kind`.
fn wait_for(seen: &Arc<Mutex<Vec<Notification>>>, kind: &str) -> Notification {
    let start = Instant::now();
    loop {
        if let Some(n) = seen.lock().unwrap().iter().find(|n| n.kind == kind) {
            return n.clone();
        }
        assert!(start.elapsed() < Duration::from_secs(10), "no {kind} notification; got {:?}", seen.lock().unwrap());
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn another_working_copy_locks_and_pushes() {
    let Some(server) = server() else {
        eprintln!("skipped: TOME_TEST_SERVER is not set");
        return;
    };
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let root = std::env::temp_dir().join(format!("tome-notify-{stamp}"));
    let url = format!("{server}/notify-{stamp}");
    let mine = root.join("mine");
    let theirs = root.join("theirs");
    std::fs::create_dir_all(&mine).unwrap();

    let mut a = Repository::open(mine.to_string_lossy().to_string());
    a.identity = "alice".into();
    ok(a.create(&url), "create");
    std::fs::write(mine.join("Hero.uasset"), b"asset").unwrap();
    ok(a.scan_status(), "scan");
    ok(a.stage(&["Hero.uasset".to_string()]), "stage");
    ok(a.commit("B"), "commit");
    ok(a.push("main"), "push");
    let mut b = Repository::open(theirs.to_string_lossy().to_string());
    b.identity = "bob".into();
    ok(b.clone_from(&url, ""), "clone");

    let seen: Arc<Mutex<Vec<Notification>>> = Arc::default();
    let sink = seen.clone();
    let subscription = a.subscribe(move |n| sink.lock().unwrap().push(n)).expect("subscribe");

    ok(b.lock("main", &["Hero.uasset".to_string()]), "lock by the other copy");
    let locked = wait_for(&seen, "resourceLocked");
    assert_eq!(locked.paths(), ["Hero.uasset"], "{locked:?}");
    ok(b.unlock("main", &["Hero.uasset".to_string()]), "unlock");
    assert_eq!(wait_for(&seen, "resourceUnlocked").paths(), ["Hero.uasset"]);

    std::fs::write(theirs.join("Hero.uasset"), b"asset v2").unwrap();
    ok(b.scan_status(), "scan");
    ok(b.stage(&["Hero.uasset".to_string()]), "stage");
    ok(b.commit("from bob"), "commit");
    ok(b.push("main"), "push by the other copy");
    let pushed = wait_for(&seen, "branchPushed");
    eprintln!("pushed: {:?}", pushed.data);

    // After unsubscribing nothing more arrives (and nothing touches freed memory).
    drop(subscription);
    let count = seen.lock().unwrap().len();
    ok(b.lock("main", &["Hero.uasset".to_string()]), "lock after unsubscribe");
    std::thread::sleep(Duration::from_millis(800));
    assert_eq!(seen.lock().unwrap().len(), count);
    let _ = model::status(&a.status());
    std::fs::remove_dir_all(&root).ok();
}

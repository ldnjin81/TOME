//! Long operations in a worker process on a real Lore server: progress lines, and cancelling
//! (ending the process) part way through a sync and a clone, then running again. Opt-in like
//! `server.rs` (never the team server).

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use tome_core::ops::{Job, Line, Op};
use tome_core::{CallResult, Repository, model};

fn server() -> Option<String> {
    let server = std::env::var("TOME_TEST_SERVER").ok().filter(|s| !s.is_empty())?;
    assert!(!server.contains(":41337") && !server.contains("lore.example.com"), "TOME_TEST_SERVER {server} looks like the team server");
    Some(server)
}

fn ok(result: CallResult, what: &str) -> CallResult {
    assert!(result.ok(), "{what} failed: status {} {}", result.status, result.error);
    result
}

/// The example program `tome-worker`, built next to the tests.
fn worker() -> PathBuf {
    let deps = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
    deps.parent().unwrap().join("examples").join(if cfg!(windows) { "tome-worker.exe" } else { "tome-worker" })
}

fn start(job: &Job) -> Child {
    Command::new(worker()).arg(serde_json::to_string(job).unwrap()).stdout(Stdio::piped()).spawn().expect("start worker")
}

/// Reads the worker's lines; with `cancel_on_progress`, ends it at the first progress line.
/// Returns (progress lines seen, the done line if it came, whether it was cancelled).
fn follow(mut child: Child, cancel_on_progress: bool) -> (usize, Option<(i32, String)>, bool) {
    let lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let (mut progress, mut done, mut cancelled) = (0, None, false);
    for line in lines.map_while(Result::ok) {
        match serde_json::from_str::<Line>(&line).expect("a worker line") {
            Line::Progress(_) => {
                progress += 1;
                if cancel_on_progress {
                    child.kill().ok();
                    cancelled = true;
                    break;
                }
            }
            Line::Done { status, error } => done = Some((status, error)),
        }
    }
    child.wait().ok();
    (progress, done, cancelled)
}

/// Deterministic, incompressible-looking bytes.
fn noise(seed: u64, size: usize) -> Vec<u8> {
    let mut x = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    (0..size)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect()
}

fn add_files(repo: &Repository, dir: &Path, prefix: &str, count: usize, size: usize, message: &str) {
    let mut paths = Vec::new();
    for i in 0..count {
        let name = format!("{prefix}/f{i:03}.bin");
        std::fs::create_dir_all(dir.join(prefix)).unwrap();
        std::fs::write(dir.join(&name), noise((i as u64) << 8 | prefix.len() as u64, size)).unwrap();
        paths.push(name);
    }
    ok(repo.scan_status(), "scan");
    ok(repo.stage(&paths), "stage");
    ok(repo.commit(message), message);
}

#[test]
fn progress_and_cancel() {
    let Some(server) = server() else {
        eprintln!("skipped: TOME_TEST_SERVER is not set");
        return;
    };
    assert!(worker().exists(), "build the examples first: {}", worker().display());
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let root = std::env::temp_dir().join(format!("tome-ops-{stamp}"));
    let url = format!("{server}/ops-{stamp}");
    let a_dir = root.join("a");
    std::fs::create_dir_all(&a_dir).unwrap();
    let mut a = Repository::open(a_dir.to_string_lossy().to_string());
    a.identity = "alice".into();
    ok(a.create(&url), "create");
    add_files(&a, &a_dir, "Content", 150, 128 * 1024, "first");

    // Push through the worker: progress lines, then done.
    let (progress, done, _) = follow(start(&Job { op: Op::Push { path: a_dir.to_string_lossy().into(), branch: "main".into() }, identity: "alice".into() }), false);
    assert_eq!(done.as_ref().map(|d| d.0), Some(0), "{done:?}");
    eprintln!("push progress lines: {progress}");

    // Clone through the worker.
    let b_dir = root.join("b");
    let (progress, done, _) = follow(start(&Job { op: Op::Clone { path: b_dir.to_string_lossy().into(), url: url.clone(), view: String::new() }, identity: "bob".into() }), false);
    assert_eq!(done.as_ref().map(|d| d.0), Some(0), "{done:?}");
    assert!(progress > 0, "clone reported progress");
    assert_eq!(std::fs::read(b_dir.join("Content/f007.bin")).unwrap(), std::fs::read(a_dir.join("Content/f007.bin")).unwrap());

    // Cancel a sync part way, then sync again: the working copy ends up complete.
    add_files(&a, &a_dir, "More", 150, 128 * 1024, "second");
    ok(a.push("main"), "push second");
    let mut b = Repository::open(b_dir.to_string_lossy().to_string());
    b.identity = "bob".into();
    let (_, done, cancelled) = follow(start(&Job { op: Op::Sync { path: b_dir.to_string_lossy().into() }, identity: "bob".into() }), true);
    eprintln!("sync cancelled: {cancelled}, done before cancel: {done:?}");
    let after_cancel = model::status(&ok(b.status(), "status after a cancelled sync")).unwrap();
    eprintln!("after cancel: rev {} remote {} files {}", after_cancel.revision_number, after_cancel.remote_number, after_cancel.files.len());
    let (_, done, _) = follow(start(&Job { op: Op::Sync { path: b_dir.to_string_lossy().into() }, identity: "bob".into() }), false);
    assert_eq!(done.as_ref().map(|d| d.0), Some(0), "sync again: {done:?}");
    // The worker stored the new revision before it ended (it flushes and shuts Lore down).
    let after_sync = model::status(&ok(b.status(), "status after sync")).unwrap();
    assert_eq!((after_sync.revision_number, after_sync.remote_number), (2, 2), "{after_sync:?}");
    for i in [0, 77, 149] {
        let name = format!("More/f{i:03}.bin");
        assert_eq!(std::fs::read(b_dir.join(&name)).unwrap(), std::fs::read(a_dir.join(&name)).unwrap(), "{name}");
    }
    let status = model::status(&ok(b.scan_status(), "scan b")).unwrap();
    assert!(status.files.iter().all(|f| f.directory), "nothing left changed: {:?}", status.files);

    // Cancel a clone part way: the partial folder is thrown away and a new clone works.
    let c_dir = root.join("c");
    let (_, _, cancelled) = follow(start(&Job { op: Op::Clone { path: c_dir.to_string_lossy().into(), url: url.clone(), view: String::new() }, identity: "carol".into() }), true);
    eprintln!("clone cancelled: {cancelled}");
    std::fs::remove_dir_all(&c_dir).ok();
    let (_, done, _) = follow(start(&Job { op: Op::Clone { path: c_dir.to_string_lossy().into(), url: url.clone(), view: String::new() }, identity: "carol".into() }), false);
    assert_eq!(done.as_ref().map(|d| d.0), Some(0), "clone again: {done:?}");
    assert_eq!(std::fs::read(c_dir.join("More/f149.bin")).unwrap(), std::fs::read(a_dir.join("More/f149.bin")).unwrap());

    std::fs::remove_dir_all(&root).ok();
}

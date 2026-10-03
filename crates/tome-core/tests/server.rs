//! End to end against a real Lore server. Opt-in: set TOME_TEST_SERVER (e.g. lore://127.0.0.1:41437)
//! to a throwaway server, such as `loreserver` run with no config on its own ports:
//! `LORE__SERVER__GRPC__PORT=41437 LORE__SERVER__QUIC__PORT=41437 LORE__SERVER__HTTP__PORT=41439 loreserver`.

use tome_core::{model, CallResult, Repository};

fn server() -> Option<String> {
    let server = std::env::var("TOME_TEST_SERVER").ok().filter(|s| !s.is_empty())?;
    // 41337 is Lore's default port, where a real server listens: tests create repositories,
    // so they must never run there.
    assert!(!server.contains(":41337"), "TOME_TEST_SERVER {server} looks like the team server; run a throwaway loreserver on another port");
    Some(server)
}

fn ok(result: CallResult, what: &str) -> CallResult {
    assert!(result.ok(), "{what} failed: status {} {}", result.status, result.error);
    result
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("tome-{name}-{}-{stamp}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn commit_push_lock_and_view() {
    let Some(server) = server() else {
        eprintln!("skipped: TOME_TEST_SERVER is not set");
        return;
    };
    let dir = temp_dir("work");
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
    let url = format!("{server}/tome-test-{stamp}");
    let mut repo = Repository::open(dir.to_string_lossy().to_string());
    // On a server without authentication, this name is the author and the lock owner.
    repo.identity = "tome-tester".into();
    ok(repo.create(&url), "create");

    // New files show up after a scan, then stage and commit them as a draft.
    std::fs::create_dir_all(dir.join("Content")).unwrap();
    std::fs::write(dir.join("Content/Hero.uasset"), b"asset v1").unwrap();
    std::fs::write(dir.join("README.txt"), b"hello").unwrap();
    let status = model::status(&ok(repo.scan_status(), "scan")).unwrap();
    let mut paths: Vec<String> = status.files.iter().filter(|f| !f.directory).map(|f| f.path.clone()).collect();
    paths.sort();
    assert_eq!(paths, ["Content/Hero.uasset", "README.txt"], "{status:?}");
    ok(repo.stage(&paths), "stage");
    let committed = ok(repo.commit("첫 커밋"), "commit");
    let revision = committed.data("revisionCommitRevision").next().expect("committed revision");
    assert_eq!(revision["revisionNumber"], 1, "a draft gets a revision number: {revision}");
    let status = model::status(&ok(repo.status(), "status")).unwrap();
    assert!(status.local_ahead, "the draft is not pushed yet: {status:?}");

    // Push, then the history has the revision with its message.
    ok(repo.push(&status.branch_name), "push");
    let status = model::status(&ok(repo.status(), "status after push")).unwrap();
    assert!(!status.local_ahead, "{status:?}");
    let history = model::history(&ok(repo.history(&status.branch_name, 10), "history"));
    assert_eq!(history[0].message, "첫 커밋");
    assert_eq!(history[0].author, "tome-tester", "{:?}", history[0]);

    // Locks: acquire, see it in the team list, release.
    let asset = vec!["Content/Hero.uasset".to_string()];
    ok(repo.lock(&status.branch_name, &asset), "lock");
    let locks = model::locks(&ok(repo.locks(&status.branch_name), "locks"));
    assert_eq!(locks.len(), 1, "{locks:?}");
    assert_eq!(locks[0].path, "Content/Hero.uasset");
    // The server takes a lock's owner from the login token only, so a server without
    // authentication records "<unknown>" whatever identity the client sends.
    assert_eq!(locks[0].owner, "<unknown>");
    ok(repo.unlock(&status.branch_name, &asset), "unlock");
    assert!(model::locks(&ok(repo.locks(&status.branch_name), "locks after unlock")).is_empty());

    // A view that leaves out Content/ removes the clean file; clearing it brings the file back.
    std::fs::write(dir.join("Content/Notes.txt"), "draft").unwrap();
    let change = repo.apply_view(&["/Content/".to_string()]).unwrap();
    assert_eq!(change.removed, vec!["Content/Hero.uasset".to_string()]);
    assert_eq!(change.kept, vec!["Content/Notes.txt".to_string()], "untracked file is kept");
    assert!(!dir.join("Content/Hero.uasset").exists());
    let after = model::status(&ok(repo.scan_status(), "scan with view")).unwrap();
    assert!(after.files.iter().all(|f| f.path != "Content/Hero.uasset"), "{:?}", after.files);

    let change = repo.apply_view(&[]).unwrap();
    assert_eq!(change.restored, vec!["Content/Hero.uasset".to_string()]);
    assert!(dir.join("Content/Hero.uasset").exists());
    assert!(dir.join("Content/Notes.txt").exists());

    // The server lists the repository, and a clone of it gets the committed files.
    let root = server.as_str();
    let listed = model::repositories(&ok(tome_core::list_repositories(root, ""), "list"));
    let name = url.rsplit('/').next().unwrap();
    assert!(listed.iter().any(|r| r.name == name), "{listed:?}");
    let copy = dir.with_extension("clone");
    let cloned = Repository::open(copy.to_string_lossy().to_string());
    ok(cloned.clone_from(&url, ""), "clone");
    assert_eq!(std::fs::read(copy.join("Content/Hero.uasset")).unwrap(), b"asset v1");
    std::fs::remove_dir_all(&copy).ok();
}

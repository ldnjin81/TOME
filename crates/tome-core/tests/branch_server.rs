//! Branches and merges on a real Lore server: create, switch (refused with local edits), a clean
//! merge, a conflicting merge settled file by file (mine, theirs, edited by hand), aborting a
//! merge, and the merge revision a commit makes. Opt-in like `server.rs` (never the team server).

use std::path::PathBuf;

use tome_core::{CallResult, Repository, Resolution, model};

fn server() -> Option<String> {
    let server = std::env::var("TOME_TEST_SERVER").ok().filter(|s| !s.is_empty())?;
    assert!(!server.contains(":41337"), "TOME_TEST_SERVER {server} looks like the team server");
    Some(server)
}

fn ok(result: CallResult, what: &str) -> CallResult {
    assert!(result.ok(), "{what} failed: status {} {}", result.status, result.error);
    result
}

struct Work {
    dir: PathBuf,
    repo: Repository,
}

impl Work {
    fn new(server: &str, name: &str) -> Self {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("tome-branch-{name}-{stamp}"));
        std::fs::create_dir_all(&dir).unwrap();
        let mut repo = Repository::open(dir.to_string_lossy().to_string());
        repo.identity = "tome-tester".into();
        ok(repo.create(&format!("{server}/branch-{name}-{stamp}")), "create");
        Work { dir, repo }
    }

    fn write(&self, file: &str, body: &[u8]) {
        std::fs::write(self.dir.join(file), body).unwrap();
    }

    fn read(&self, file: &str) -> Vec<u8> {
        std::fs::read(self.dir.join(file)).unwrap()
    }

    fn commit(&self, files: &[(&str, &[u8])], message: &str) {
        for (file, body) in files {
            self.write(file, body);
        }
        ok(self.repo.scan_status(), "scan");
        ok(self.repo.stage(&files.iter().map(|(f, _)| f.to_string()).collect::<Vec<_>>()), "stage");
        ok(self.repo.commit(message), message);
    }

    fn status(&self) -> model::Status {
        model::status(&ok(self.repo.scan_status(), "status")).unwrap()
    }

    fn file(&self, path: &str) -> model::ChangedFile {
        self.status().files.into_iter().find(|f| f.path == path).unwrap_or_else(|| panic!("{path} not in status"))
    }

    /// main: a.txt, b.bin; branch f changes both, main changes both differently.
    fn diverged(server: &str, name: &str) -> Self {
        let w = Work::new(server, name);
        w.commit(&[("a.txt", b"one\ntwo\nthree\n"), ("b.bin", &[0, 1, 2, 0]), ("c.txt", b"same\n")], "B");
        ok(w.repo.create_branch("f"), "create f");
        ok(w.repo.switch_branch("f"), "switch f");
        w.commit(&[("a.txt", b"one\nTWO-f\nthree\n"), ("b.bin", &[0, 9, 9, 0]), ("d.txt", b"new on f\n")], "F1");
        ok(w.repo.switch_branch("main"), "switch main");
        w.commit(&[("a.txt", b"one\nTWO-main\nthree\n"), ("b.bin", &[0, 7, 7, 0])], "M1");
        w
    }
}

impl Drop for Work {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

#[test]
fn branches_switching_and_merges() {
    let Some(server) = server() else {
        eprintln!("skipped: TOME_TEST_SERVER is not set");
        return;
    };

    // Switching with an uncommitted edit is refused and leaves everything as it was.
    let w = Work::diverged(&server, "switch");
    w.write("a.txt", b"local edit\n");
    ok(w.repo.scan_status(), "scan");
    let refused = w.repo.switch_branch("f");
    assert!(!refused.ok());
    assert!(refused.error.contains("Local modifications"), "{}", refused.error);
    assert_eq!(w.status().branch_name, "main");
    assert_eq!(w.read("a.txt"), b"local edit\n");
    // A new branch starts at the current revision.
    w.write("a.txt", b"one\nTWO-main\nthree\n");
    ok(w.repo.create_branch("g"), "create g");
    ok(w.repo.switch_branch("g"), "switch g");
    assert_eq!(w.status().branch_name, "g");
    assert_eq!(w.read("a.txt"), b"one\nTWO-main\nthree\n");

    // A merge without conflicts commits on its own (two parents).
    let w = Work::new(&server, "clean");
    w.commit(&[("a.txt", b"a\n")], "B");
    ok(w.repo.create_branch("f"), "create f");
    ok(w.repo.switch_branch("f"), "switch f");
    w.commit(&[("f.txt", b"f\n")], "F1");
    ok(w.repo.switch_branch("main"), "switch main");
    w.commit(&[("m.txt", b"m\n")], "M1");
    ok(w.repo.merge_branch("f", "merge f"), "clean merge");
    let status = w.status();
    assert!(status.merging.is_empty(), "{status:?}");
    assert_eq!(w.read("f.txt"), b"f\n");
    let top = &model::history(&ok(w.repo.history("", 1), "history"))[0];
    assert_eq!((top.message.as_str(), top.parents.len()), ("merge f", 2));

    // Conflicts: the merge stops with both files unresolved; text gets conflict markers.
    let w = Work::diverged(&server, "conflict");
    let merge = ok(w.repo.merge_branch("f", "merge f"), "merge with conflicts");
    let status = w.status();
    assert!(!status.merging.is_empty(), "a merge is in progress: {status:?}");
    let _ = merge;
    for path in ["a.txt", "b.bin"] {
        let file = w.file(path);
        assert!(file.conflict && file.unresolved && file.resolution.is_empty(), "{file:?}");
    }
    let marked = String::from_utf8(w.read("a.txt")).unwrap();
    assert!(marked.contains("<<<<<<<") && marked.contains("TWO-main") && marked.contains("TWO-f"), "{marked}");
    // The file only f added came in without a conflict.
    assert_eq!(w.read("d.txt"), b"new on f\n");

    // Settle: a.txt by hand, b.bin with theirs.
    w.write("a.txt", b"one\nTWO-both\nthree\n");
    ok(w.repo.merge_resolve(&["a.txt".into()], Resolution::Edited), "resolve edited");
    ok(w.repo.merge_resolve(&["b.bin".into()], Resolution::Theirs), "resolve theirs");
    assert_eq!((w.file("a.txt").unresolved, w.file("a.txt").resolution.as_str()), (false, "edited"));
    assert_eq!((w.file("b.bin").unresolved, w.file("b.bin").resolution.as_str()), (false, "theirs"));
    assert_eq!(w.read("b.bin"), [0, 9, 9, 0]);
    // Unresolve and settle again with mine.
    ok(w.repo.merge_unresolve(&["b.bin".into()]), "unresolve");
    assert!(w.file("b.bin").unresolved);
    ok(w.repo.merge_resolve(&["b.bin".into()], Resolution::Mine), "resolve mine");
    assert_eq!(w.file("b.bin").resolution, "mine");
    assert_eq!(w.read("b.bin"), [0, 7, 7, 0]);
    // The commit finishes the merge.
    ok(w.repo.commit("merge f, settled"), "commit merge");
    assert!(w.status().merging.is_empty());
    let top = &model::history(&ok(w.repo.history("", 1), "history"))[0];
    assert_eq!((top.message.as_str(), top.parents.len()), ("merge f, settled", 2));
    assert_eq!(w.read("a.txt"), b"one\nTWO-both\nthree\n");

    // Abort: everything goes back to the branch as it was.
    let w = Work::diverged(&server, "abort");
    ok(w.repo.merge_branch("f", "merge f"), "merge with conflicts");
    assert!(!w.status().merging.is_empty());
    ok(w.repo.merge_abort(), "abort");
    let status = w.status();
    assert!(status.merging.is_empty(), "{status:?}");
    assert!(status.files.iter().all(|f| f.directory), "no changed files left: {:?}", status.files);
    assert_eq!(w.read("a.txt"), b"one\nTWO-main\nthree\n");
    assert!(!w.dir.join("d.txt").exists(), "files the merge brought are gone");
}


/// A dry-run merge lists what would come in and what would conflict, and changes nothing.
#[test]
fn merge_preview_reports_without_changing_anything() {
    let Some(server) = server() else { return };
    let w = Work::diverged(&server, "preview");
    let preview = model::merge_preview(&ok(w.repo.merge_preview("f"), "preview"));
    assert_eq!(preview.conflicts, 2, "{preview:?}");
    let rows: Vec<_> = preview.files.iter().map(|f| (f.path.as_str(), f.action.as_str(), f.conflict)).collect();
    assert_eq!(rows, [("a.txt", "change", true), ("b.bin", "change", true), ("d.txt", "add", false)]);
    let status = w.status();
    assert!(status.merging.is_empty(), "no merge started: {status:?}");
    assert!(status.files.iter().all(|f| f.directory), "nothing written: {:?}", status.files);
    assert!(!w.dir.join("d.txt").exists());
    assert_eq!(w.read("a.txt"), b"one\nTWO-main\nthree\n");
}

/// 보관한 브랜치를 같은 id로 되살리면 원래 이름·마지막 리비전으로 돌아오고 작업본은 원래 브랜치에
/// 남는다. 같은 이름의 활성 브랜치가 있으면 거부한다.
#[test]
fn archived_branch_is_restored_with_its_id_and_history() {
    let Some(server) = server() else { return };
    let w = Work::new(&server, "restore");
    w.commit(&[("a.txt", b"a\n")], "B");
    ok(w.repo.push("main"), "push main");
    ok(w.repo.create_branch("f"), "create f");
    ok(w.repo.switch_branch("f"), "switch f");
    w.commit(&[("f.txt", b"f\n")], "F1");
    ok(w.repo.push("f"), "push f");
    let f = model::branches(&ok(w.repo.branches(), "branches")).into_iter().find(|b| b.name == "f").unwrap();
    let latest = model::history(&ok(w.repo.history("f", 1), "history f"))[0].id.clone();
    ok(w.repo.switch_branch("main"), "switch main");
    ok(w.repo.archive_branch("f"), "archive f");
    assert!(model::branches(&ok(w.repo.branches(), "branches")).iter().all(|b| b.name != "f" || b.archived));

    w.repo.restore_branch(&f.id, "f", &latest).expect("restore f");
    assert_eq!(w.status().branch_name, "main", "the working copy stays on its branch");
    let restored: Vec<_> = model::branches(&ok(w.repo.branches(), "branches")).into_iter().filter(|b| b.name == "f").collect();
    assert!(!restored.is_empty() && restored.iter().all(|b| b.id == f.id && !b.archived), "{restored:?}");
    assert_eq!(model::history(&ok(w.repo.history("f", 1), "history f"))[0].id, latest);
    ok(w.repo.switch_branch("f"), "switch to the restored branch");
    assert_eq!(w.read("f.txt"), b"f\n");

    // 같은 이름의 새 브랜치가 생긴 뒤에는 되살리지 않는다.
    ok(w.repo.switch_branch("main"), "switch main");
    ok(w.repo.archive_branch("f"), "archive f again");
    ok(w.repo.create_branch("f"), "a new f");
    ok(w.repo.switch_branch("main"), "switch main");
    let refused = w.repo.restore_branch(&f.id, "f", &latest).unwrap_err();
    assert!(refused.contains("already exists"), "{refused}");
}

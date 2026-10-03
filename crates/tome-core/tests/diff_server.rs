//! Diffs on a real Lore server: files changed by a revision, text and binary patches, and the
//! working files against the current revision. Opt-in like `server.rs` (never the team server).

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

#[test]
fn revision_and_working_diffs() {
    let Some(server) = server() else {
        eprintln!("skipped: TOME_TEST_SERVER is not set");
        return;
    };
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("tome-diff-{stamp}"));
    std::fs::create_dir_all(dir.join("Content")).unwrap();
    let mut repo = Repository::open(dir.to_string_lossy().to_string());
    repo.identity = "tome-tester".into();
    ok(repo.create(&format!("{server}/diff-{stamp}")), "create");

    let commit = |files: &[(&str, &[u8])], message: &str| {
        for (path, body) in files {
            std::fs::write(dir.join(path), body).unwrap();
        }
        ok(repo.scan_status(), "scan");
        ok(repo.stage(&files.iter().map(|(p, _)| p.to_string()).collect::<Vec<_>>()), "stage");
        ok(repo.commit(message), message);
    };
    commit(&[("notes.txt", b"one\ntwo\nthree\n"), ("Content/Hero.uasset", &[0, 1, 2, 3, 0, 255])], "v1");
    commit(&[("notes.txt", b"one\nTWO\nthree\nfour\n"), ("Content/Hero.uasset", &[0, 9, 9, 9, 0, 255]), ("new.txt", b"hello\n")], "v2");

    let history = model::history(&ok(repo.history("", 10), "history"));
    let (v2, v1) = (&history[0], &history[1]);
    assert_eq!(v2.parents[0], v1.id);

    // Files the revision changed, against its first parent.
    let mut files: Vec<(String, String)> = model::diff_files(&ok(repo.revision_diff(&v1.id, &v2.id), "revision diff"))
        .into_iter()
        .filter(|f| !f.directory)
        .map(|f| (f.path, f.action))
        .collect();
    files.sort();
    assert_eq!(files.iter().map(|f| f.0.as_str()).collect::<Vec<_>>(), ["Content/Hero.uasset", "new.txt", "notes.txt"], "{files:?}");
    let action = |p: &str| files.iter().find(|f| f.0 == p).unwrap().1.clone();
    assert_eq!(action("new.txt"), "add");
    assert_eq!(action("notes.txt"), "modify");

    // Text patch and binary marker.
    let paths = ["notes.txt".to_string(), "Content/Hero.uasset".to_string()];
    let patches = model::patches(&ok(repo.file_diff(&paths, &v1.id, &v2.id, 3), "file diff"));
    let text = patches.iter().find(|p| p.path == "notes.txt").expect("notes.txt patch");
    assert!(!text.binary);
    assert!(text.patch.contains("-two") && text.patch.contains("+TWO") && text.patch.contains("+four"), "{}", text.patch);
    let binary = patches.iter().find(|p| p.path == "Content/Hero.uasset").expect("binary patch");
    assert!(binary.binary, "{}", binary.patch);

    // The first revision has no parent: its files are all additions.
    let mut first: Vec<(String, String)> = model::delta_files(&ok(repo.changes(&v1.id), "changes of the first revision"))
        .into_iter()
        .filter(|f| !f.directory)
        .map(|f| (f.path, f.action))
        .collect();
    first.sort();
    assert_eq!(first, [("Content/Hero.uasset".to_string(), "add".to_string()), ("notes.txt".to_string(), "add".to_string())]);
    // changes() of a later revision is the diff against its first parent.
    let second = model::delta_files(&ok(repo.changes(&v2.id), "changes"));
    eprintln!("delta of v2: {second:?}");
    assert_eq!(second.iter().filter(|f| !f.directory && f.action != "keep").count(), 3, "{second:?}");
    // A text patch from the empty revision is the whole file as added lines.
    let added = repo.file_diff(&["notes.txt".to_string()], "", &v1.id, 3);
    eprintln!("patch of a file in the first revision: status {} {} {:?}", added.status, added.error, model::patches(&added));

    // A file's history: the revisions that changed it, newest first, with their messages.
    commit(&[("other.txt", b"untouched notes\n")], "v3 (not notes.txt)");
    let notes = repo.file_history("notes.txt", 10).expect("file history");
    let seen: Vec<(String, u64)> = notes.iter().map(|f| (f.revision.message.clone(), f.size)).collect();
    assert_eq!(seen, [("v2".to_string(), 19), ("v1".to_string(), 14)], "{notes:?}");
    assert_eq!(notes[0].revision.author, "tome-tester");
    assert_eq!(notes[0].path, "notes.txt");
    assert_eq!(repo.file_history("new.txt", 10).unwrap().len(), 1);

    // Working files against the current revision (uncommitted edit).
    std::fs::write(dir.join("notes.txt"), "one\nTWO\nthree\nfour\nfive\n").unwrap();
    let current = model::status(&ok(repo.status(), "status")).unwrap().revision;
    let working = model::patches(&ok(repo.file_diff(&["notes.txt".to_string()], &current, "", 3), "working diff"));
    assert!(working.iter().any(|p| p.patch.contains("+five")), "{working:?}");

    std::fs::remove_dir_all(&dir).ok();
}

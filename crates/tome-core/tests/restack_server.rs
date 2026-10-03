//! Restack on a real Lore server: my unpushed revisions moved onto new server revisions (text
//! merged on its own, a new file), a binary conflict settled by keeping my version or the
//! base's, abort putting everything back, reordering the stack, the preview's conflict risks,
//! and refusing to start with uncommitted changes. Opt-in like `server.rs` (never the team server).

use std::path::PathBuf;

use tome_core::restack::{self, Keep, Pick, Plan, Step};
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

struct Side {
    dir: PathBuf,
    repo: Repository,
}

impl Side {
    fn open(dir: PathBuf, who: &str) -> Self {
        std::fs::create_dir_all(&dir).unwrap();
        let mut repo = Repository::open(dir.to_string_lossy().to_string());
        repo.identity = who.into();
        Side { dir, repo }
    }

    fn commit(&self, files: &[(&str, &[u8])], message: &str) -> String {
        for (file, body) in files {
            std::fs::write(self.dir.join(file), body).unwrap();
        }
        ok(self.repo.scan_status(), "scan");
        ok(self.repo.stage(&files.iter().map(|(f, _)| f.to_string()).collect::<Vec<_>>()), "stage");
        ok(self.repo.commit(message), message);
        self.head()
    }

    fn head(&self) -> String {
        model::status(&self.repo.status()).unwrap().revision
    }

    fn read(&self, file: &str) -> Vec<u8> {
        std::fs::read(self.dir.join(file)).unwrap()
    }

    /// Messages on the branch, newest first, and whether the history is a straight line.
    fn line(&self) -> (Vec<String>, bool) {
        let history = model::history(&self.repo.history("main", 20));
        let straight = history.iter().all(|r| r.parents.len() <= 1) && history.windows(2).all(|w| w[0].parents == [w[1].id.clone()]);
        (history.into_iter().map(|r| r.message).collect(), straight)
    }
}

/// Alice and Bob share main (B: a.txt, b.bin). Bob pushes `bob`; Alice commits `alice`
/// without pushing. Returns (root, alice, bob).
fn diverged(server: &str, name: &str, bob: &[(&[(&str, &[u8])], &str)], alice: &[(&[(&str, &[u8])], &str)]) -> (PathBuf, Side, Side) {
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let root = std::env::temp_dir().join(format!("tome-restack-{name}-{stamp}"));
    let url = format!("{server}/restack-{name}-{stamp}");
    let a = Side::open(root.join("a"), "alice");
    ok(a.repo.create(&url), "create");
    a.commit(&[("a.txt", b"1\n2\n3\n"), ("b.bin", &[0, 1, 0])], "B");
    ok(a.repo.push("main"), "push B");
    let b = Side::open(root.join("b"), "bob");
    ok(b.repo.clone_from(&url, ""), "clone");
    for (files, message) in bob {
        b.commit(files, message);
    }
    if !bob.is_empty() {
        ok(b.repo.push("main"), "push bob");
    }
    for (files, message) in alice {
        a.commit(files, message);
    }
    (root, a, b)
}

/// The plan that moves the whole stack, in its order, onto the server's head.
fn onto_server(stack: &restack::Stack, original_head: &str) -> Plan {
    Plan {
        onto: stack.remote_head.clone(),
        picks: stack.drafts.iter().rev().map(|r| Pick { id: r.id.clone(), message: r.message.clone() }).collect(),
        original_head: original_head.to_string(),
    }
}

fn order(stack: &restack::Stack) -> Vec<String> {
    stack.drafts.iter().rev().map(|r| r.id.clone()).collect()
}

#[test]
fn restack_onto_new_server_revisions() {
    let Some(server) = server() else { return };
    let (root, a, b) = diverged(
        &server,
        "onto",
        &[(&[("a.txt", b"1\n2\nTHREE\n"), ("c.txt", b"bob\n")], "R1 bob")],
        &[(&[("a.txt", b"ONE\n2\n3\n")], "D1"), (&[("x.txt", b"x\n")], "D2")],
    );
    // Pushing as it is is refused: the branch has diverged.
    assert!(!a.repo.push("main").ok());

    let stack = a.repo.stack().unwrap();
    assert_eq!(stack.drafts.iter().map(|r| r.message.as_str()).collect::<Vec<_>>(), ["D2", "D1"]);
    assert_eq!(stack.fork.as_ref().map(|r| r.message.as_str()), Some("B"));
    assert_eq!(stack.incoming.iter().map(|r| r.message.as_str()).collect::<Vec<_>>(), ["R1 bob"]);

    let plan = onto_server(&stack, &a.head());
    let preview = restack::preview(&a.repo, &plan, &stack.fork.as_ref().unwrap().id, &order(&stack), &[], "alice").unwrap();
    let mut base_changes = preview.base_changes.clone();
    base_changes.sort();
    assert_eq!(base_changes, ["a.txt", "c.txt"]);
    assert_eq!(preview.picks[0].risks, [restack::Risk { path: "a.txt".into(), binary: false }]);
    assert!(preview.picks[1].risks.is_empty());

    assert!(matches!(restack::run(&a.repo, &plan, 0).unwrap(), Step::Done { .. }));
    assert_eq!(a.read("a.txt"), b"ONE\n2\nTHREE\n", "both edits of a.txt kept");
    assert_eq!(a.read("c.txt"), b"bob\n");
    assert_eq!(a.read("x.txt"), b"x\n");
    assert_eq!(a.line(), (vec!["D2".into(), "D1".into(), "R1 bob".into(), "B".into()], true));

    ok(a.repo.push("main"), "push the restacked stack");
    ok(b.repo.sync(), "bob syncs");
    assert_eq!(b.line(), (vec!["D2".into(), "D1".into(), "R1 bob".into(), "B".into()], true));
    assert_eq!(b.read("a.txt"), b"ONE\n2\nTHREE\n");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn binary_conflict_keeps_my_version_or_the_base() {
    let Some(server) = server() else { return };
    for keep in [Keep::Mine, Keep::Base] {
        let (root, a, _b) = diverged(
            &server,
            &format!("bin-{keep:?}"),
            &[(&[("M_Rock.uasset", &[0, 7, 0])], "R1 bob bin")],
            &[(&[("M_Rock.uasset", &[0, 9, 0])], "D1 bin"), (&[("x.txt", b"x\n")], "D2")],
        );
        let stack = a.repo.stack().unwrap();
        let plan = onto_server(&stack, &a.head());
        let preview = restack::preview(&a.repo, &plan, &stack.fork.as_ref().unwrap().id, &order(&stack), &[], "alice").unwrap();
        assert_eq!(preview.picks[0].risks, [restack::Risk { path: "M_Rock.uasset".into(), binary: true }]);

        let step = restack::run(&a.repo, &plan, 0).unwrap();
        assert_eq!(step, Step::Conflict { index: 0, files: vec!["M_Rock.uasset".into()] });
        // Not settled yet: continuing is refused.
        assert!(restack::continue_after(&a.repo, &plan, 0).unwrap_err().contains("M_Rock.uasset"));

        restack::resolve(&a.repo, &["M_Rock.uasset".into()], keep).unwrap();
        let expected: &[u8] = if keep == Keep::Mine { &[0, 9, 0] } else { &[0, 7, 0] };
        assert_eq!(a.read("M_Rock.uasset"), expected, "{keep:?}");
        assert!(matches!(restack::continue_after(&a.repo, &plan, 0).unwrap(), Step::Done { .. }));
        assert_eq!(a.read("M_Rock.uasset"), expected, "{keep:?} after the commit");
        assert_eq!(a.read("x.txt"), b"x\n", "the pick after the conflict applied too");
        assert_eq!(a.line(), (vec!["D2".into(), "D1 bin".into(), "R1 bob bin".into(), "B".into()], true));
        ok(a.repo.push("main"), "push");
        std::fs::remove_dir_all(&root).ok();
    }
}

#[test]
fn abort_puts_the_branch_and_files_back() {
    let Some(server) = server() else { return };
    let (root, a, _b) = diverged(
        &server,
        "abort",
        &[(&[("b.bin", &[0, 7, 0]), ("c.txt", b"bob\n")], "R1 bob")],
        &[(&[("x.txt", b"x\n")], "D1"), (&[("b.bin", &[0, 9, 0])], "D2 bin")],
    );
    let before = model::history(&a.repo.history("main", 10)).into_iter().map(|r| r.id).collect::<Vec<_>>();
    let stack = a.repo.stack().unwrap();
    let plan = onto_server(&stack, &a.head());
    assert_eq!(restack::run(&a.repo, &plan, 0).unwrap(), Step::Conflict { index: 1, files: vec!["b.bin".into()] });

    restack::abort(&a.repo, &plan).unwrap();
    assert_eq!(model::history(&a.repo.history("main", 10)).into_iter().map(|r| r.id).collect::<Vec<_>>(), before);
    assert_eq!(a.read("b.bin"), [0, 9, 0]);
    assert_eq!(a.read("x.txt"), b"x\n");
    assert!(!a.dir.join("c.txt").exists(), "the server's file is gone again");
    let status = model::status(&ok(a.repo.scan_status(), "status")).unwrap();
    assert!(status.files.iter().all(|f| f.directory), "nothing left changed: {:?}", status.files);
    // The same restack can be started again.
    assert_eq!(restack::run(&a.repo, &plan, 0).unwrap(), Step::Conflict { index: 1, files: vec!["b.bin".into()] });
    restack::abort(&a.repo, &plan).unwrap();
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn reorder_the_stack() {
    let Some(server) = server() else { return };
    let (root, a, _b) = diverged(
        &server,
        "reorder",
        &[],
        &[(&[("a.txt", b"ONE\n2\n3\n")], "D1"), (&[("x.txt", b"x\n")], "D2"), (&[("a.txt", b"ONE\n2\n3!\n")], "D3")],
    );
    ok(a.repo.push("main"), "nothing diverged yet");
    // Three more, unpushed: the stack to reorder on its own base.
    a.commit(&[("p.txt", b"p\n")], "P");
    a.commit(&[("q.txt", b"q\n")], "Q");
    a.commit(&[("p.txt", b"p2\n")], "P2");
    let stack = a.repo.stack().unwrap();
    assert!(stack.incoming.is_empty());
    let fork = stack.fork.clone().expect("fork");
    assert_eq!(fork.message, "D3");
    let [p, q, p2] = [2, 1, 0].map(|i| Pick { id: stack.drafts[i].id.clone(), message: stack.drafts[i].message.clone() });

    // Q first: independent of P, so no risk; P2 still after P.
    let plan = Plan { onto: fork.id.clone(), picks: vec![q.clone(), p.clone(), p2.clone()], original_head: a.head() };
    let preview = restack::preview(&a.repo, &plan, &fork.id, &order(&stack), &[("q.txt".into(), "carol".into()), ("p.txt".into(), "alice".into())], "alice").unwrap();
    assert!(preview.base_changes.is_empty());
    assert!(preview.picks.iter().all(|p| p.risks.is_empty()), "{:?}", preview.picks);
    assert_eq!(preview.picks[0].locked, [("q.txt".to_string(), "carol".to_string())], "someone else's lock; mine is not listed");
    assert!(preview.picks[1].locked.is_empty());
    assert!(matches!(restack::run(&a.repo, &plan, 0).unwrap(), Step::Done { .. }));
    assert_eq!(a.line().0[..4], ["P2", "P", "Q", "D3"]);
    assert_eq!(a.read("p.txt"), b"p2\n");

    // P2 before P: both change p.txt, so both are at risk.
    let stack = a.repo.stack().unwrap();
    let ids: Vec<Pick> = stack.drafts.iter().rev().map(|r| Pick { id: r.id.clone(), message: r.message.clone() }).collect();
    let plan = Plan { onto: fork.id.clone(), picks: vec![ids[0].clone(), ids[2].clone(), ids[1].clone()], original_head: a.head() };
    let preview = restack::preview(&a.repo, &plan, &fork.id, &order(&stack), &[], "alice").unwrap();
    let risky: Vec<&str> = preview.picks.iter().filter(|p| !p.risks.is_empty()).map(|p| p.message.as_str()).collect();
    assert_eq!(risky, ["P2", "P"]);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn refuses_with_uncommitted_changes() {
    let Some(server) = server() else { return };
    let (root, a, _b) = diverged(&server, "dirty", &[(&[("c.txt", b"bob\n")], "R1")], &[(&[("x.txt", b"x\n")], "D1")]);
    std::fs::write(a.dir.join("x.txt"), b"edited\n").unwrap();
    let stack = a.repo.stack().unwrap();
    let plan = onto_server(&stack, &a.head());
    assert!(restack::run(&a.repo, &plan, 0).unwrap_err().contains("uncommitted"));
    assert_eq!(a.read("x.txt"), b"edited\n", "nothing touched");
    assert_eq!(a.line().0, ["D1", "B"]);
    std::fs::remove_dir_all(&root).ok();
}

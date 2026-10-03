//! Graph scenarios built for real on a Lore server: branches, commits and merges made through
//! tome-core, then every branch read back and laid out. Checks that what Lore returns (parent
//! order, branch ids, revision numbers) gives the graph the unit scenarios in `graph.rs` expect.
//! Opt-in like `server.rs`: TOME_TEST_SERVER=lore://127.0.0.1:41437 (never the team server).

use std::path::PathBuf;

use tome_core::{CallResult, Repository, graph, model};

fn server() -> Option<String> {
    let server = std::env::var("TOME_TEST_SERVER").ok().filter(|s| !s.is_empty())?;
    assert!(!server.contains(":41337"), "TOME_TEST_SERVER {server} looks like the team server");
    Some(server)
}

fn ok(result: CallResult, what: &str) -> CallResult {
    assert!(result.ok(), "{what} failed: status {} {}", result.status, result.error);
    result
}

/// A fresh repository on the server with one working copy.
struct Work {
    dir: PathBuf,
    repo: Repository,
}

impl Work {
    fn new(server: &str, name: &str) -> Self {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("tome-graph-{name}-{stamp}"));
        std::fs::create_dir_all(&dir).unwrap();
        let repo = Repository::open(dir.to_string_lossy().to_string());
        ok(repo.create(&format!("{server}/graph-{name}-{stamp}")), "create");
        Work { dir, repo }
    }

    /// Writes `file` and commits it as `message` on the current branch.
    fn commit(&self, file: &str, message: &str) {
        // Distinct timestamps, so the expected order does not depend on the clock's resolution.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(self.dir.join(file), message).unwrap();
        ok(self.repo.scan_status(), "scan");
        ok(self.repo.stage(&[file.to_string()]), "stage");
        ok(self.repo.commit(message), message);
    }

    fn branch(&self, name: &str) {
        ok(self.repo.create_branch(name), "create branch");
        ok(self.repo.switch_branch(name), "switch to new branch");
    }

    fn switch(&self, name: &str) {
        ok(self.repo.switch_branch(name), "switch");
    }

    fn merge(&self, from: &str, message: &str) {
        std::thread::sleep(std::time::Duration::from_millis(20));
        ok(self.repo.merge_branch(from, message), message);
    }

    fn push(&self, branch: &str) {
        ok(self.repo.push(branch), "push");
    }

    /// The whole graph as text, each node labelled with its message.
    fn drawing(&self) -> String {
        let rows = self.repo.graph(100).unwrap().rows;
        graph::ascii(&rows, |r| r.message.clone())
    }

    fn check(&self, expected: &str) {
        // TOME_GRAPH_DUMP=<dir>: also save the graph and status as JSON, to look at it in the UI.
        if let Ok(dump) = std::env::var("TOME_GRAPH_DUMP") {
            let name = self.dir.file_name().unwrap().to_string_lossy().split('-').nth(2).unwrap_or("graph").to_string();
            let status = model::status(&self.repo.status()).unwrap();
            let value = serde_json::json!({ "graph": self.repo.graph(100).unwrap(), "status": status, "branches": model::branches(&self.repo.branches()) });
            std::fs::write(std::path::Path::new(&dump).join(format!("{name}.json")), value.to_string()).unwrap();
        }
        let got = self.drawing();
        let expected = expected.trim_matches('\n').lines().map(str::trim_end).collect::<Vec<_>>().join("\n");
        assert_eq!(got, expected, "\n--- got ---\n{got}\n--- expected ---\n{expected}\n");
    }
}

impl Drop for Work {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

#[test]
fn scenarios_on_a_real_server() {
    let Some(server) = server() else {
        eprintln!("skipped: TOME_TEST_SERVER is not set");
        return;
    };

    // 1. Linear history on main.
    let w = Work::new(&server, "linear");
    w.commit("a.txt", "B");
    w.commit("a.txt", "C2");
    w.commit("a.txt", "C3");
    w.check(
        "
*  C3
*  C2
*  B",
    );

    // 2. A feature branch merged back while main moved on.
    let w = Work::new(&server, "feature");
    w.commit("a.txt", "B");
    w.push("main");
    w.branch("feature");
    w.commit("f.txt", "F1");
    w.commit("f.txt", "F2");
    w.switch("main");
    w.commit("m.txt", "M1");
    w.merge("feature", "M2");
    w.check(
        r"
*    M2
|\
* |  M1
| *  F2
| *  F1
|/
*    B",
    );

    // 3. A branch that is not merged (its head is newer than main's).
    let w = Work::new(&server, "open");
    w.commit("a.txt", "B");
    w.branch("feature");
    w.switch("main");
    w.commit("m.txt", "M1");
    w.switch("feature");
    w.commit("f.txt", "F1");
    w.check(
        r"
*    F1
| *  M1
|/
*    B",
    );

    // 4. main merged into the branch, then the branch merged into main.
    let w = Work::new(&server, "both-ways");
    w.commit("a.txt", "B");
    w.branch("feature");
    w.commit("f.txt", "F1");
    w.switch("main");
    w.commit("m.txt", "M1");
    w.switch("feature");
    w.merge("main", "F2");
    w.switch("main");
    w.merge("feature", "M2");
    w.check(
        r"
*    M2
|\
| *  F2
|/|
* |  M1
| *  F1
|/
*    B",
    );

    // 5. Two branches merged one after the other (C1 is older than the merge M1, so it sits
    //    below it and A1's line crosses C1's lane).
    let w = Work::new(&server, "two");
    w.commit("a.txt", "B");
    w.branch("a");
    w.commit("x.txt", "A1");
    w.switch("main");
    w.branch("c");
    w.commit("y.txt", "C1");
    w.switch("main");
    w.merge("a", "M1");
    w.merge("c", "M2");
    w.check(
        r"
*      M2
|\
* |    M1
|-|\
| * |  C1
| | *  A1
|/-/
*      B",
    );

    // 6. Drafts: revisions committed after the last push are ahead of the remote number.
    let w = Work::new(&server, "drafts");
    w.commit("a.txt", "B");
    w.push("main");
    w.commit("a.txt", "D1");
    w.commit("a.txt", "D2");
    let status = model::status(&ok(w.repo.status(), "status")).unwrap();
    assert!(status.local_ahead, "{status:?}");
    let history = model::history(&ok(w.repo.history("main", 10), "history"));
    let drafts: Vec<&str> = history.iter().filter(|r| r.number > status.remote_number).map(|r| r.message.as_str()).collect();
    assert_eq!(drafts, ["D2", "D1"], "remote r{} {history:?}", status.remote_number);
}

//! TOME core: talks to Lore in-process through its C API (`lore::interface`, the functions
//! declared in Lore's `lore.h`).
//!
//! Every Lore function is asynchronous: it returns at once and reports through a callback that
//! runs on a Lore worker thread. [`call`] turns one call into a [`CallResult`]: every event,
//! copied out as JSON while the callback runs (event data is only valid until it returns), and
//! the final status.

use std::sync::mpsc;

pub mod graph;
pub mod model;
pub mod tools;
pub mod view;

pub use lore::interface;
use lore::interface::{LoreEvent, LoreEventCallbackConfig, LoreGlobalArgs, LoreString};
use serde_json::Value;

/// The outcome of one Lore call.
#[derive(Debug, Clone)]
pub struct CallResult {
    /// Every event in order, as `{"tagName": ..., "data": ...}` (Lore's own serde form).
    pub events: Vec<Value>,
    /// The status of the `Complete` event: 0 on success.
    pub status: i32,
    /// The error message of the `Complete` event; empty on success.
    pub error: String,
}

impl CallResult {
    pub fn ok(&self) -> bool {
        self.status == 0
    }

    /// The data of every event with this tag (`"branchListEntry"`, ...).
    pub fn data(&self, tag: &str) -> impl Iterator<Item = &Value> {
        self.events.iter().filter(move |e| e["tagName"] == tag).map(|e| &e["data"])
    }
}

/// One event copied out of the callback, or the end of the stream.
enum Message {
    Event(Value),
    Complete(i32, String),
    End,
}

/// Receives events on a Lore worker thread. `user_context` is a leaked `Box<mpsc::Sender>`
/// that is freed when the `End` event arrives (always the last event of a call).
unsafe extern "C" fn on_event(event: &LoreEvent, user_context: u64) {
    // SAFETY: user_context is the pointer created in `call` and is freed only after End.
    let sender = unsafe { &*(user_context as *const mpsc::Sender<Message>) };
    match event {
        LoreEvent::Complete(data) => {
            let _ = sender.send(Message::Complete(data.status, data.error.message.as_str().to_string()));
        }
        LoreEvent::End(_) => {
            let _ = sender.send(Message::End);
            // SAFETY: End is the final event; nothing uses the sender after this.
            drop(unsafe { Box::from_raw(user_context as *mut mpsc::Sender<Message>) });
            return;
        }
        _ => {}
    }
    if !matches!(event, LoreEvent::Complete(_)) {
        let value = serde_json::to_value(event).unwrap_or(Value::Null);
        let _ = sender.send(Message::Event(value));
    }
}

/// Runs one Lore function and waits for it to finish.
///
/// `globals` and `args` (and the strings they point to) must stay alive until this returns,
/// which they do because the call blocks until the `End` event.
pub fn call<A>(function: extern "C" fn(&LoreGlobalArgs, &A, LoreEventCallbackConfig), globals: &LoreGlobalArgs, args: &A) -> CallResult {
    let (sender, receiver) = mpsc::channel();
    let context = Box::into_raw(Box::new(sender)) as u64;
    function(globals, args, LoreEventCallbackConfig { user_context: context, func: Some(on_event) });
    let mut result = CallResult { events: Vec::new(), status: -1, error: String::new() };
    for message in receiver {
        match message {
            Message::Event(value) => result.events.push(value),
            Message::Complete(status, error) => {
                result.status = status;
                result.error = error;
            }
            Message::End => break,
        }
    }
    result
}

/// [`Repository::graph`]'s result.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Graph {
    pub rows: Vec<graph::Row>,
    /// Branches whose history could not be read in full, as `"name: error"`.
    pub incomplete: Vec<String>,
}

/// The repositories on the server at `url` (`lore://host:port`), asked as `identity`.
pub fn list_repositories(url: &str, identity: &str) -> CallResult {
    let args = lore::repository::LoreRepositoryListArgs { url: LoreString::from_bytes(url.as_bytes()) };
    let globals = LoreGlobalArgs { identity: LoreString::from_bytes(identity.as_bytes()), ..Default::default() };
    call(interface::lore_repository_list_async, &globals, &args)
}

/// Whether the server at `url` needs a login, and who is logged in on this machine.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AuthState {
    /// The server has an auth endpoint (a login is needed); false for a LAN server without one.
    pub server_requires_login: bool,
    /// Identities logged in on this machine (Lore's stored logins).
    pub logged_in: Vec<String>,
    /// What Lore said when asked, for the settings screen.
    pub detail: String,
}

/// Asks Lore about logins. Lore answers "requires a configured auth endpoint" when the server
/// (or this working copy's remote) has no authentication: then the identity name is all it uses.
pub fn auth_state(working_copy: &str) -> AuthState {
    let globals = LoreGlobalArgs {
        repository_path: LoreString::from_bytes(working_copy.as_bytes()),
        working_directory: LoreString::from_bytes(working_copy.as_bytes()),
        ..Default::default()
    };
    let list = call(interface::lore_auth_list_async, &globals, &lore::auth::LoreAuthListArgs { with_token: 0 });
    let logged_in = list
        .events
        .iter()
        .filter(|e| e["tagName"].as_str().is_some_and(|t| t.starts_with("authIdentity") || t.starts_with("authUser")))
        .filter_map(|e| e["data"]["userId"].as_str().or(e["data"]["name"].as_str()).map(str::to_string))
        .filter(|s| !s.is_empty())
        .collect();
    let info = call(
        interface::lore_auth_local_user_info_async,
        &globals,
        &lore::auth::LoreAuthLocalUserInfoArgs { auth_endpoint: LoreString::default(), user_ids: Default::default(), with_identity_token: 0, with_access_token: 0 },
    );
    let no_endpoint = !info.ok() && info.error.contains("auth endpoint");
    AuthState { server_requires_login: !info.ok() && !no_endpoint, logged_in, detail: if info.ok() { String::new() } else { info.error } }
}

/// Global arguments for calls on the repository at `path`.
pub struct Repository {
    path: String,
    /// Run without contacting the server (local data only).
    pub offline: bool,
    /// Who acts: on a server without authentication Lore records this name as the author of
    /// commits and the owner of locks. Empty: Lore's default (a logged-in identity, if any).
    pub identity: String,
}

impl Repository {
    pub fn open(path: impl Into<String>) -> Self {
        Repository { path: path.into(), offline: false, identity: String::new() }
    }

    fn globals(&self) -> LoreGlobalArgs {
        LoreGlobalArgs {
            repository_path: LoreString::from_bytes(self.path.as_bytes()),
            // Relative paths in a call (stage, lock, ...) are relative to the working copy root.
            working_directory: LoreString::from_bytes(self.path.as_bytes()),
            offline: self.offline as u8,
            identity: LoreString::from_bytes(self.identity.as_bytes()),
            ..Default::default()
        }
    }

    /// Creates a repository on the server at `url` with this working copy (tests and new projects).
    pub fn create(&self, url: &str) -> CallResult {
        let args = lore::repository::LoreRepositoryCreateArgs {
            repository_url: LoreString::from_bytes(url.as_bytes()),
            description: LoreString::default(),
            id: LoreString::default(),
            vfs: Default::default(),
            use_shared_store: Default::default(),
            shared_store_path: LoreString::default(),
        };
        std::fs::create_dir_all(&self.path).ok();
        call(interface::lore_repository_create_async, &self.globals(), &args)
    }

    /// Clones the repository at `url` (`lore://host:port/name`) into this path. `view` is the
    /// initial `.lore/view` text; empty materializes everything.
    pub fn clone_from(&self, url: &str, view: &str) -> CallResult {
        let args = lore::repository::LoreRepositoryCloneArgs {
            repository_url: LoreString::from_bytes(url.as_bytes()),
            view: LoreString::from_bytes(view.as_bytes()),
            ..Default::default()
        };
        std::fs::create_dir_all(&self.path).ok();
        // The working directory must exist before the call; the clone fills it.
        let globals = LoreGlobalArgs {
            repository_path: LoreString::from_bytes(self.path.as_bytes()),
            identity: LoreString::from_bytes(self.identity.as_bytes()),
            ..Default::default()
        };
        call(interface::lore_repository_clone_async, &globals, &args)
    }

    /// Status after reconciling the working files with the current revision, so new and
    /// edited files show up (this refreshes Lore's dirty tracking).
    pub fn scan_status(&self) -> CallResult {
        let args = lore::repository::LoreRepositoryStatusArgs {
            staged: 1,
            scan: 1,
            check_dirty: 0,
            reset: 0,
            sync_point: 0,
            revision_only: 0,
            count: 0,
            paths: interface::LoreArray::default(),
        };
        call(interface::lore_repository_status_async, &self.globals(), &args)
    }

    /// Stages files (paths relative to the working copy root) for the next commit.
    pub fn stage(&self, paths: &[String]) -> CallResult {
        let args = lore::file::LoreFileStageArgs { paths: strings(paths), case_change: 0, scan: 1 };
        call(interface::lore_file_stage_async, &self.globals(), &args)
    }

    pub fn unstage(&self, paths: &[String]) -> CallResult {
        let args = lore::file::LoreFileUnstageArgs { paths: strings(paths) };
        call(interface::lore_file_unstage_async, &self.globals(), &args)
    }

    /// Commits the staged files as a local revision (a draft until pushed).
    pub fn commit(&self, message: &str) -> CallResult {
        let args = lore::revision::LoreRevisionCommitArgs {
            message: LoreString::from_bytes(message.as_bytes()),
            link: LoreString::default(),
            link_paths: interface::LoreArray::default(),
            link_messages: interface::LoreArray::default(),
            layer: LoreString::default(),
            layer_paths: interface::LoreArray::default(),
            layer_messages: interface::LoreArray::default(),
        };
        call(interface::lore_revision_commit_async, &self.globals(), &args)
    }

    /// Pushes the branch's local revisions to the server.
    pub fn push(&self, branch: &str) -> CallResult {
        let args = lore::branch::LoreBranchPushArgs { branch: LoreString::from_bytes(branch.as_bytes()), fast_forward_merge: 0 };
        call(interface::lore_branch_push_async, &self.globals(), &args)
    }

    /// Brings the working copy to the branch's latest revision.
    pub fn sync(&self) -> CallResult {
        self.sync_with(false)
    }

    /// `reset`: rewrite the working files to match the revision exactly (applies a changed
    /// `.lore/view`; local edits to tracked files are lost, so check status first).
    pub fn sync_with(&self, reset: bool) -> CallResult {
        let args = lore::revision::LoreRevisionSyncArgs {
            revision: LoreString::default(),
            forward_changes: 1,
            reset: reset as u8,
            root_files: interface::LoreArray::default(),
            dependency_tags: interface::LoreArray::default(),
            dependency_recursive: 0,
            dependency_depth_limit: 0,
        };
        call(interface::lore_revision_sync_async, &self.globals(), &args)
    }

    /// Creates `branch` at the current revision (it does not switch to it).
    pub fn create_branch(&self, branch: &str) -> CallResult {
        let args = lore::branch::LoreBranchCreateArgs {
            branch: LoreString::from_bytes(branch.as_bytes()),
            category: LoreString::default(),
            id: LoreString::default(),
        };
        call(interface::lore_branch_create_async, &self.globals(), &args)
    }

    /// Makes `branch` the working copy's branch, at its latest revision.
    pub fn switch_branch(&self, branch: &str) -> CallResult {
        let args = lore::branch::LoreBranchSwitchArgs {
            branch: LoreString::from_bytes(branch.as_bytes()),
            revision: LoreString::default(),
            reset: 0,
            bare: 0,
        };
        call(interface::lore_branch_switch_async, &self.globals(), &args)
    }

    /// Merges `branch` into the current branch and commits the merge with `message` when
    /// nothing conflicts.
    pub fn merge_branch(&self, branch: &str, message: &str) -> CallResult {
        let args = lore::branch::LoreBranchMergeStartArgs {
            branch: LoreString::from_bytes(branch.as_bytes()),
            message: LoreString::from_bytes(message.as_bytes()),
            no_commit: 0,
            link: LoreString::default(),
            ignore_links: 0,
            inherit_metadata: interface::LoreArray::default(),
        };
        call(interface::lore_branch_merge_start_async, &self.globals(), &args)
    }

    /// The graph of every branch that is not archived: each branch's last `length` revisions,
    /// joined and laid out in lanes (a merge's second parent is in another branch's history).
    ///
    /// Offline, Lore reads only what this machine has, and stops with "Not found" where older
    /// or merged revisions were never fetched. Such a branch is read again from the server
    /// (reading only); what still cannot be read is listed in [`Graph::incomplete`], and the
    /// revisions already returned are kept.
    pub fn graph(&self, length: u32) -> Result<Graph, String> {
        let branches = self.branches();
        if !branches.ok() {
            return Err(branches.error);
        }
        let online = Repository { path: self.path.clone(), offline: false, identity: self.identity.clone() };
        let mut revisions = Vec::new();
        let mut incomplete = Vec::new();
        for branch in model::branches(&branches).into_iter().filter(|b| !b.archived) {
            let mut history = self.history(&branch.name, length);
            if !history.ok() && self.offline {
                let again = online.history(&branch.name, length);
                if again.ok() || model::history(&again).len() > model::history(&history).len() {
                    history = again;
                }
            }
            if !history.ok() {
                incomplete.push(format!("{}: {}", branch.name, history.error));
            }
            revisions.extend(model::history(&history));
        }

        // Merged branches are often deleted after the merge, so no branch history reaches the
        // second parents. Read each one's chain from the revision itself, a few rounds deep (a
        // side chain can hold merges of its own).
        let mut tried = std::collections::HashSet::new();
        for _ in 0..4 {
            let present: std::collections::HashSet<&str> = revisions.iter().map(|r| r.id.as_str()).collect();
            let wanted: Vec<String> = revisions
                .iter()
                .flat_map(|r| r.parents.iter().skip(1))
                .filter(|p| !present.contains(p.as_str()) && !tried.contains(p.as_str()))
                .cloned()
                .collect();
            if wanted.is_empty() {
                break;
            }
            let mut found = Vec::new();
            for parent in wanted {
                let mut side = self.history_from(&parent, length);
                if !side.ok() && self.offline {
                    side = online.history_from(&parent, length);
                }
                found.extend(model::history(&side));
                tried.insert(parent);
            }
            revisions.extend(found);
        }
        Ok(Graph { rows: graph::layout(&revisions), incomplete })
    }

    /// Restores `paths` from the current revision (rewrites the working files; local edits are lost).
    pub fn reset_files(&self, paths: &[String]) -> CallResult {
        let args = lore::file::LoreFileResetArgs { paths: strings(paths), revision: LoreString::default(), purge: 0 };
        call(interface::lore_file_reset_async, &self.globals(), &args)
    }

    /// Every lock on `branch` (owner and path filters empty: the whole team).
    pub fn locks(&self, branch: &str) -> CallResult {
        let args = lore::lock::LoreLockFileQueryArgs {
            branch: LoreString::from_bytes(branch.as_bytes()),
            owner: LoreString::default(),
            path: LoreString::default(),
        };
        call(interface::lore_lock_file_query_async, &self.globals(), &args)
    }

    pub fn lock(&self, branch: &str, paths: &[String]) -> CallResult {
        let args = lore::lock::LoreLockFileAcquireArgs { paths: strings(paths), branch: LoreString::from_bytes(branch.as_bytes()) };
        call(interface::lore_lock_file_acquire_async, &self.globals(), &args)
    }

    /// Releases this user's locks on `paths`.
    pub fn unlock(&self, branch: &str, paths: &[String]) -> CallResult {
        let args = lore::lock::LoreLockFileReleaseArgs {
            paths: strings(paths),
            branch: LoreString::from_bytes(branch.as_bytes()),
            owner: LoreString::default(),
            owner_id: LoreString::default(),
        };
        call(interface::lore_lock_file_release_async, &self.globals(), &args)
    }

    /// The `.lore/view` lines. A plain glob EXCLUDES paths and a `!glob` includes them again
    /// (e.g. `/*` then `!/Source/` keeps only Source). Empty: everything is materialized.
    pub fn view(&self) -> Vec<String> {
        std::fs::read_to_string(std::path::Path::new(&self.path).join(".lore").join("view"))
            .map(|text| text.lines().map(str::to_string).collect())
            .unwrap_or_default()
    }

    /// Writes `.lore/view` only; [`Repository::apply_view`] also updates the working files.
    pub fn set_view(&self, lines: &[String]) -> std::io::Result<()> {
        let mut text = lines.join("\n");
        if !text.is_empty() {
            text.push('\n');
        }
        std::fs::write(std::path::Path::new(&self.path).join(".lore").join("view"), text)
    }

    /// The working copy's current revision, branch and changed files (no filesystem scan).
    pub fn status(&self) -> CallResult {
        let args = lore::repository::LoreRepositoryStatusArgs {
            staged: 1,
            scan: 0,
            check_dirty: 0,
            reset: 0,
            sync_point: 0,
            revision_only: 0,
            count: 0,
            paths: interface::LoreArray::default(),
        };
        call(interface::lore_repository_status_async, &self.globals(), &args)
    }

    /// Every branch.
    pub fn branches(&self) -> CallResult {
        let args = lore::branch::LoreBranchListArgs { archived: 0 };
        call(interface::lore_branch_list_async, &self.globals(), &args)
    }

    /// The files that differ from `source` to `target` (revision ids; an empty target is the
    /// current revision).
    pub fn revision_diff(&self, source: &str, target: &str) -> CallResult {
        let args = lore::revision::LoreRevisionDiffArgs {
            revision_source: LoreString::from_bytes(source.as_bytes()),
            revision_target: LoreString::from_bytes(target.as_bytes()),
            paths: interface::LoreArray::default(),
        };
        call(interface::lore_revision_diff_async, &self.globals(), &args)
    }

    /// The files `revision` changed against its parent (Lore's revision info with the delta;
    /// it works for a first revision too, where a diff has no source to start from).
    pub fn changes(&self, revision: &str) -> CallResult {
        let args = lore::revision::LoreRevisionInfoArgs { revision: LoreString::from_bytes(revision.as_bytes()), delta: 1, metadata: 0 };
        call(interface::lore_revision_info_async, &self.globals(), &args)
    }

    /// Unified diffs of `paths` from `source` to `target` (an empty target: the working files).
    /// A binary file gives a "Binary files differ" marker instead of text.
    pub fn file_diff(&self, paths: &[String], source: &str, target: &str, context_lines: u32) -> CallResult {
        let args = lore::file::LoreFileDiffArgs {
            paths: strings(paths),
            source_revision: LoreString::from_bytes(source.as_bytes()),
            target_revision: LoreString::from_bytes(target.as_bytes()),
            diff3: 0,
            context_lines,
            ignore_whitespace_eol: 0,
            ignore_whitespace_inline: 0,
        };
        call(interface::lore_file_diff_async, &self.globals(), &args)
    }

    /// Up to `length` revisions from `revision` back along its first parents, stopping where
    /// the chain reaches another branch (the side of a merge whose branch may be deleted).
    pub fn history_from(&self, revision: &str, length: u32) -> CallResult {
        let args = lore::revision::LoreRevisionHistoryArgs {
            revision: LoreString::from_bytes(revision.as_bytes()),
            length,
            only_branch: 1,
            ..Default::default()
        };
        call(interface::lore_revision_history_async, &self.globals(), &args)
    }

    /// Up to `length` revisions of `branch` (empty: the current branch), newest first.
    pub fn history(&self, branch: &str, length: u32) -> CallResult {
        let args = lore::revision::LoreRevisionHistoryArgs {
            branch: LoreString::from_bytes(branch.as_bytes()),
            length,
            ..Default::default()
        };
        call(interface::lore_revision_history_async, &self.globals(), &args)
    }
}

/// A Lore string array borrowing `items` (which must outlive the call).
fn strings(items: &[String]) -> interface::LoreArray<LoreString> {
    interface::LoreArray::from_vec(items.iter().map(|s| LoreString::from_bytes(s.as_bytes())).collect())
}

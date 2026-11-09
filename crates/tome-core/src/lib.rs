//! TOME core: talks to Lore in-process through its C API (`lore::interface`, the functions
//! declared in Lore's `lore.h`).
//!
//! Every Lore function is asynchronous: it returns at once and reports through a callback that
//! runs on a Lore worker thread. [`call`] turns one call into a [`CallResult`]: every event,
//! copied out as JSON while the callback runs (event data is only valid until it returns), and
//! the final status.

use std::sync::mpsc;

pub mod assets;
pub mod auth;
pub mod graph;
pub mod branch_explorer;
pub mod model;
pub mod nested;
pub mod notify;
pub mod ops;
pub mod tools;
pub mod restack;
pub mod uasset;
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
    call_watching(function, globals, args, &mut |_| {})
}

/// Like [`call`], and `watch` sees each event as it arrives (for progress).
pub fn call_watching<A>(
    function: extern "C" fn(&LoreGlobalArgs, &A, LoreEventCallbackConfig),
    globals: &LoreGlobalArgs,
    args: &A,
    watch: &mut dyn FnMut(&Value),
) -> CallResult {
    let (sender, receiver) = mpsc::channel();
    let context = Box::into_raw(Box::new(sender)) as u64;
    function(globals, args, LoreEventCallbackConfig { user_context: context, func: Some(on_event) });
    let mut result = CallResult { events: Vec::new(), status: -1, error: String::new() };
    for message in receiver {
        match message {
            Message::Event(value) => {
                watch(&value);
                result.events.push(value);
            }
            Message::Complete(status, error) => {
                result.status = status;
                result.error = error;
            }
            Message::End => break,
        }
    }
    result
}

/// How a conflicted file is settled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Resolution {
    /// Keep the current branch's version.
    Mine,
    /// Take the merged branch's version.
    Theirs,
    /// The file was edited by hand; take it as it is now.
    Edited,
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
    let identity = auth::identity_for_remote(url, identity, true);
    let args = lore::repository::LoreRepositoryListArgs { url: LoreString::from_bytes(url.as_bytes()) };
    let globals = LoreGlobalArgs { identity: LoreString::from_bytes(identity.as_bytes()), ..Default::default() };
    call(interface::lore_repository_list_async, &globals, &args)
}

pub use auth::AuthState;

/// Closes Lore's reused server connections (after a logout, or before a working copy moves).
pub fn drop_connections() {
    lore_revision::interface::drop_connections();
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

    pub(crate) fn globals(&self) -> LoreGlobalArgs {
        LoreGlobalArgs {
            repository_path: LoreString::from_bytes(self.path.as_bytes()),
            // Relative paths in a call (stage, lock, ...) are relative to the working copy root.
            working_directory: LoreString::from_bytes(self.path.as_bytes()),
            offline: self.offline as u8,
            identity: LoreString::from_bytes(auth::identity_for_path(&self.path, &self.identity, !self.offline).as_bytes()),
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
        let mut globals = self.globals();
        globals.identity = LoreString::from_bytes(auth::identity_for_remote(url, &self.identity, true).as_bytes());
        call(interface::lore_repository_create_async, &globals, &args)
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
            identity: LoreString::from_bytes(auth::identity_for_remote(url, &self.identity, true).as_bytes()),
            ..Default::default()
        };
        call(interface::lore_repository_clone_async, &globals, &args)
    }

    /// Global arguments for removing or moving this working copy. Lore refuses while the call's
    /// working directory is inside the instance, so it runs from the folder that holds it.
    fn instance_globals(&self, dry_run: bool) -> LoreGlobalArgs {
        let parent = std::path::Path::new(&self.path).parent().map(|p| p.display().to_string()).unwrap_or_default();
        LoreGlobalArgs {
            repository_path: LoreString::from_bytes(self.path.as_bytes()),
            working_directory: LoreString::from_bytes(parent.as_bytes()),
            offline: 1,
            dry_run: dry_run as u8,
            ..Default::default()
        }
    }

    /// Deletes this working copy: deregisters the instance and removes its files (Lore 0.13).
    /// `dry_run` only reports the phases it would run.
    pub fn instance_delete(&self, dry_run: bool) -> CallResult {
        let args = lore::repository::LoreRepositoryInstanceDeleteArgs {};
        call(interface::lore_repository_instance_delete_async, &self.instance_globals(dry_run), &args)
    }

    /// Moves this working copy to `target`, which must not exist yet (Lore 0.13).
    /// `dry_run` checks the move without changing anything.
    pub fn instance_move(&self, target: &str, dry_run: bool) -> CallResult {
        let args = lore::repository::LoreRepositoryInstanceMoveArgs { target_path: LoreString::from_bytes(target.as_bytes()) };
        call(interface::lore_repository_instance_move_async, &self.instance_globals(dry_run), &args)
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
            view: LoreString::default(),
        };
        call(interface::lore_revision_sync_async, &self.globals(), &args)
    }

    /// Creates `branch` at the current revision (it does not switch to it).
    ///
    /// Lore 0.13의 `branch create`는 작업본의 현재 브랜치를 새 브랜치로 바꾼다(파일·리비전은 그대로).
    /// 그래서 원래 브랜치로 되돌린다. 같은 리비전 간 전환이라 로컬 수정이 있어도 된다.
    pub fn create_branch(&self, branch: &str) -> CallResult {
        let original = self.current_branch_name();
        let args = lore::branch::LoreBranchCreateArgs {
            branch: LoreString::from_bytes(branch.as_bytes()),
            category: LoreString::default(),
            id: LoreString::default(),
        };
        let mut created = call(interface::lore_branch_create_async, &self.globals(), &args);
        if created.ok() {
            if let Err(error) = self.return_to_branch(&original) {
                created.status = -1;
                created.error = format!("Created {branch}, but could not return to {original}: {error}");
            }
        }
        created
    }

    /// 작업본의 현재 브랜치 이름. 알 수 없으면 빈 문자열.
    fn current_branch_name(&self) -> String {
        model::status(&self.status()).map(|s| s.branch_name).unwrap_or_default()
    }

    /// `branch create`가 바꿔 놓은 현재 브랜치를 `original`로 되돌린다(이미 그대로면 아무것도 안 한다).
    fn return_to_branch(&self, original: &str) -> Result<(), String> {
        if !needs_return(original, &self.current_branch_name()) {
            return Ok(());
        }
        let back = self.switch_branch(original);
        if back.ok() { Ok(()) } else { Err(back.error) }
    }

    /// Archives `branch` locally and on the server (Lore keeps its metadata, so it can be restored).
    pub fn archive_branch(&self, branch: &str) -> CallResult {
        let args = lore::branch::LoreBranchArchiveArgs {
            branch: LoreString::from_bytes(branch.as_bytes()),
            layer: LoreString::default(),
            include_layers: 0,
            link: LoreString::default(),
            include_links: 0,
        };
        call(interface::lore_branch_archive_async, &self.globals(), &args)
    }

    /// 보관한 브랜치를 원래 id·이름으로 되살린다(Lore에는 unarchive가 없다).
    ///
    /// 1. 같은 id로 `branch create`: Lore는 남아 있는 브랜치 메타데이터를 보고 이름→ID만 되살린다
    ///    ("Restoring deleted branch"). 이때 Lore가 작업본의 현재 브랜치를 그 브랜치로 바꾸므로
    ///    (파일은 그대로) 원래 브랜치로 되돌린다.
    /// 2. `branch reset <latest> --branch <name>`: 로컬 latest를 보관 전 마지막 리비전으로 맞춘다.
    /// 3. `branch push <name>`: 서버의 브랜치 항목을 되살린다(다른 사람·기기에도 보인다).
    ///
    /// `latest_revision`이 비어 있으면 `branch latest list --branch <id>`로 보관 전 마지막 리비전을
    /// 구한다(보관 브랜치는 이름으로 찾을 수 없어 id로 묻는다).
    ///
    /// 같은 이름의 활성 브랜치가 있거나, 그 id가 이미 활성이거나, 오프라인이면 거부한다.
    /// 되살린 브랜치의 마지막 리비전을 돌려준다.
    pub fn restore_branch(&self, id: &str, name: &str, latest_revision: &str) -> Result<String, String> {
        if self.offline {
            return Err("Restoring a branch needs the server; go online first".into());
        }
        if !is_branch_id(id) {
            return Err(format!("Not a branch id: {id}"));
        }
        if name.trim().is_empty() {
            return Err("Restoring a branch needs its name".into());
        }
        let listed = self.branches();
        if !listed.ok() {
            return Err(listed.error);
        }
        if let Some(refusal) = restore_refusal(&model::branches(&listed), id, name) {
            return Err(refusal);
        }
        let latest_revision = match latest_revision.trim() {
            "" => self.branch_latest(id).map_err(|e| format!("Could not find the last revision of {name}: {e}"))?,
            given => given.to_string(),
        };
        let latest_revision = latest_revision.as_str();
        let original = self.current_branch_name();

        let args = lore::branch::LoreBranchCreateArgs {
            branch: LoreString::from_bytes(name.as_bytes()),
            category: LoreString::default(),
            id: LoreString::from_bytes(id.as_bytes()),
        };
        let created = call(interface::lore_branch_create_async, &self.globals(), &args);
        if !created.ok() {
            return Err(created.error);
        }
        // 작업본은 원래 브랜치의 리비전 그대로이니 같은 리비전으로 전환하는 것이라 로컬 수정이 있어도 된다.
        self.return_to_branch(&original).map_err(|e| format!("Restored {name}, but could not return to {original}: {e}"))?;
        let args = lore::branch::LoreBranchResetArgs {
            revision: LoreString::from_bytes(latest_revision.as_bytes()),
            branch: LoreString::from_bytes(name.as_bytes()),
        };
        let reset = call(interface::lore_branch_reset_async, &self.globals(), &args);
        if !reset.ok() {
            return Err(format!("Restored {name}, but could not set its last revision: {}", reset.error));
        }
        let pushed = self.push(name);
        if !pushed.ok() {
            return Err(format!("Restored {name} locally, but could not push it: {}", pushed.error));
        }
        Ok(latest_revision.to_string())
    }

    /// `branch`(이름 또는 id)의 LATEST가 마지막으로 가리킨 리비전(`branch latest list`의 첫 항목).
    /// 보관한 브랜치도 id로 물으면 보관 전 마지막 리비전을 돌려준다.
    pub fn branch_latest(&self, branch: &str) -> Result<String, String> {
        let args = lore::branch::LoreBranchLatestListArgs { branch: LoreString::from_bytes(branch.as_bytes()), limit: 1 };
        let result = call(interface::lore_branch_latest_list_async, &self.globals(), &args);
        if !result.ok() {
            return Err(result.error);
        }
        model::latest_revision(&result).ok_or_else(|| format!("Branch {branch} has no latest revision"))
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
            link_paths: interface::LoreArray::default(),
            link_messages: interface::LoreArray::default(),
        };
        call(interface::lore_branch_merge_start_async, &self.globals(), &args)
    }

    /// The same merge as a dry run (Lore 0.12: no side effects): what it would bring in and
    /// which files would conflict, with nothing written to the working copy or the server.
    pub fn merge_preview(&self, branch: &str) -> CallResult {
        let args = lore::branch::LoreBranchMergeStartArgs {
            branch: LoreString::from_bytes(branch.as_bytes()),
            message: LoreString::default(),
            no_commit: 1,
            link: LoreString::default(),
            ignore_links: 0,
            inherit_metadata: interface::LoreArray::default(),
            link_paths: interface::LoreArray::default(),
            link_messages: interface::LoreArray::default(),
        };
        let mut globals = self.globals();
        globals.dry_run = 1;
        call(interface::lore_branch_merge_start_async, &globals, &args)
    }

    /// How to settle conflicted files of a merge in progress.
    pub fn merge_resolve(&self, paths: &[String], how: Resolution) -> CallResult {
        let paths = strings(paths);
        match how {
            Resolution::Mine => call(interface::lore_branch_merge_resolve_mine_async, &self.globals(), &lore::branch::LoreBranchMergeResolveMineArgs { paths }),
            Resolution::Theirs => call(interface::lore_branch_merge_resolve_theirs_async, &self.globals(), &lore::branch::LoreBranchMergeResolveTheirsArgs { paths }),
            Resolution::Edited => call(interface::lore_branch_merge_resolve_async, &self.globals(), &lore::branch::LoreBranchMergeResolveArgs { paths }),
        }
    }

    /// Marks resolved files conflicted again.
    pub fn merge_unresolve(&self, paths: &[String]) -> CallResult {
        call(interface::lore_branch_merge_unresolve_async, &self.globals(), &lore::branch::LoreBranchMergeUnresolveArgs { paths: strings(paths) })
    }

    /// Gives up the merge in progress and returns to the branch as it was.
    pub fn merge_abort(&self) -> CallResult {
        let args = lore::branch::LoreBranchMergeAbortArgs { link: LoreString::default(), ignore_links: 0 };
        call(interface::lore_branch_merge_abort_async, &self.globals(), &args)
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

    /// Moves the branch's local head to `revision` (the working files are not touched; sync to
    /// it afterwards). Revisions after it that were never pushed are no longer on the branch.
    pub fn branch_reset(&self, revision: &str) -> CallResult {
        let args = lore::branch::LoreBranchResetArgs { revision: LoreString::from_bytes(revision.as_bytes()), branch: LoreString::default() };
        call(interface::lore_branch_reset_async, &self.globals(), &args)
    }

    /// Syncs to `revision` with the view filter in the file `view`: the working files are made to
    /// match that view and Lore keeps it as this instance's view (Lore 0.10.1, `lore sync --view`).
    pub fn sync_view(&self, revision: &str, view: &str) -> CallResult {
        let args = lore::revision::LoreRevisionSyncArgs {
            revision: LoreString::from_bytes(revision.as_bytes()),
            forward_changes: 1,
            reset: 0,
            root_files: interface::LoreArray::default(),
            dependency_tags: interface::LoreArray::default(),
            dependency_recursive: 0,
            dependency_depth_limit: 0,
            view: LoreString::from_bytes(view.as_bytes()),
        };
        call(interface::lore_revision_sync_async, &self.globals(), &args)
    }

    /// Brings the working copy to `revision` (with `reset`, local edits are overwritten).
    pub fn sync_to(&self, revision: &str, reset: bool) -> CallResult {
        let args = lore::revision::LoreRevisionSyncArgs {
            revision: LoreString::from_bytes(revision.as_bytes()),
            forward_changes: 0,
            reset: reset as u8,
            root_files: interface::LoreArray::default(),
            dependency_tags: interface::LoreArray::default(),
            dependency_recursive: 0,
            dependency_depth_limit: 0,
            view: LoreString::default(),
        };
        call(interface::lore_revision_sync_async, &self.globals(), &args)
    }

    /// Applies `revision`'s changes on top of the current revision and commits them with
    /// `message` when nothing conflicts; on a conflict the pick stays open (resolve or abort).
    pub fn cherry_pick(&self, revision: &str, message: &str) -> CallResult {
        let args = lore::revision::LoreRevisionCherryPickArgs {
            revision: LoreString::from_bytes(revision.as_bytes()),
            message: LoreString::from_bytes(message.as_bytes()),
            no_commit: 0,
            inherit_metadata: interface::LoreArray::default(),
        };
        call(restack::cherry_pick_async, &self.globals(), &args)
    }

    /// Settles conflicted files of the cherry-pick in progress.
    pub fn cherry_pick_resolve(&self, paths: &[String], how: Resolution) -> CallResult {
        let paths = strings(paths);
        match how {
            Resolution::Mine => call(restack::cherry_pick_resolve_mine_async, &self.globals(), &lore::revision::LoreRevisionCherryPickResolveMineArgs { paths }),
            Resolution::Theirs => call(restack::cherry_pick_resolve_theirs_async, &self.globals(), &lore::revision::LoreRevisionCherryPickResolveTheirsArgs { paths }),
            Resolution::Edited => call(restack::cherry_pick_resolve_async, &self.globals(), &lore::revision::LoreRevisionCherryPickResolveArgs { paths }),
        }
    }

    /// Gives up the cherry-pick in progress.
    pub fn cherry_pick_abort(&self) -> CallResult {
        call(restack::cherry_pick_abort_async, &self.globals(), &lore::revision::LoreRevisionCherryPickAbortArgs {})
    }

    /// Writes `paths` as they are in `revision` into the working copy (a path that does not
    /// exist there is deleted); the branch does not move.
    pub fn reset_files_to(&self, paths: &[String], revision: &str) -> CallResult {
        let args = lore::file::LoreFileResetArgs { paths: strings(paths), revision: LoreString::from_bytes(revision.as_bytes()), purge: 0 };
        call(interface::lore_file_reset_async, &self.globals(), &args)
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

    /// The revisions that changed `file` (newest first, up to `length`), each with its message,
    /// author and time, the file's path in that revision (it may have moved) and its size.
    pub fn file_history(&self, file: &str, length: u32) -> Result<Vec<model::FileRevision>, String> {
        let args = lore::file::LoreFileHistoryArgs {
            path: LoreString::from_bytes(file.as_bytes()),
            revision: LoreString::default(),
            branch: LoreString::default(),
            length,
            depth: 0,
        };
        let result = call(interface::lore_file_history_async, &self.globals(), &args);
        if !result.ok() {
            return Err(result.error);
        }
        Ok(result
            .data("fileHistory")
            .map(|entry| {
                let id = entry["revision"].as_str().unwrap_or("").to_string();
                // The revision's message, author and time come with its own history entry.
                let revision = model::history(&self.history_from(&id, 1)).into_iter().next().unwrap_or(model::Revision {
                    id: id.clone(),
                    number: entry["revisionNumber"].as_u64().unwrap_or(0),
                    parents: Vec::new(),
                    message: String::new(),
                    author: String::new(),
                    timestamp: 0,
                    branch_id: String::new(),
                    metadata: Vec::new(),
                });
                model::FileRevision { path: entry["path"].as_str().unwrap_or("").to_string(), size: entry["size"].as_u64().unwrap_or(0), revision }
            })
            .collect())
    }

    /// Which revision last changed each line of `file` as it is on disk: ranges of
    /// consecutive lines sharing an attribution, in line order, with the file's text.
    /// Lines edited since the current revision come back as `uncommitted`.
    pub fn file_audit(&self, file: &str) -> Result<model::FileAudit, String> {
        let args = lore::file::LoreFileAuditArgs {
            path: LoreString::from_bytes(file.as_bytes()),
            revision: LoreString::default(),
            max_revisions: 0,
            no_ignore_space_at_eol: 0,
            ignore_space_change: 0,
        };
        let result = call(interface::lore_file_audit_async, &self.globals(), &args);
        if !result.ok() {
            return Err(result.error);
        }
        let mut ranges: Vec<model::AuditRange> = result
            .data("fileAudit")
            .map(|range| model::AuditRange {
                revision: range["revision"].as_str().unwrap_or("").to_string(),
                first: range["lineFirst"].as_u64().unwrap_or(0) as u32,
                last: range["lineLast"].as_u64().unwrap_or(0) as u32,
                outcome: range["outcome"].as_str().unwrap_or("").to_string(),
            })
            .collect();
        ranges.sort_by_key(|r| r.first);
        let bytes = std::fs::read(std::path::Path::new(&self.path).join(file)).map_err(|e| e.to_string())?;
        let lines = String::from_utf8_lossy(&bytes).lines().map(str::to_string).collect();
        Ok(model::FileAudit { lines, ranges })
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

    /// `branch`의 리비전 중 `since`(Unix ms) 이후에 만든 것만, 최신부터. Lore가 그 날짜보다 오래된
    /// 리비전에 닿으면 거기서 멈추므로 오래된 기록은 읽지 않는다. 0이면 [`Repository::history`]와 같다.
    pub fn history_since(&self, branch: &str, since: u64) -> CallResult {
        let args = lore::revision::LoreRevisionHistoryArgs {
            branch: LoreString::from_bytes(branch.as_bytes()),
            date: since,
            length: u32::MAX,
            ..Default::default()
        };
        call(interface::lore_revision_history_async, &self.globals(), &args)
    }
}

/// Lore 브랜치 id(16바이트 hex, 32자)인지.
pub fn is_branch_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

/// 보관 브랜치 `id`를 `name`으로 되살릴 수 없는 이유. 목록에는 같은 브랜치가 로컬·원격으로 두 번
/// 나올 수 있고, 보관된 항목은 따지지 않는다.
pub fn restore_refusal(branches: &[model::Branch], id: &str, name: &str) -> Option<String> {
    let active = || branches.iter().filter(|b| !b.archived);
    if active().any(|b| b.id.eq_ignore_ascii_case(id)) {
        return Some(format!("Branch {name} is not archived"));
    }
    if active().any(|b| b.name == name) {
        return Some(format!("A branch named {name} already exists; rename or archive it before restoring"));
    }
    None
}

/// `branch create` 뒤 원래 브랜치로 되돌려야 하는지. 원래 브랜치를 몰랐으면(빈 이름) 건드리지 않는다.
fn needs_return(original: &str, now: &str) -> bool {
    !original.is_empty() && now != original
}

/// A Lore string array borrowing `items` (which must outlive the call).
fn strings(items: &[String]) -> interface::LoreArray<LoreString> {
    interface::LoreArray::from_vec(items.iter().map(|s| LoreString::from_bytes(s.as_bytes())).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branch(id: &str, name: &str, archived: bool) -> model::Branch {
        model::Branch { id: id.into(), name: name.into(), latest: String::new(), current: false, archived, creator: String::new(), created: 0 }
    }

    const OLD: &str = "01a1c12b0ae1724ca701e9751c978dc7";

    #[test]
    fn create_returns_to_the_original_branch_only_when_it_moved() {
        // Lore가 현재 브랜치를 새 브랜치로 바꿨으면 되돌린다.
        assert!(needs_return("main", "feat"));
        // 그대로면, 또는 원래 브랜치를 몰랐으면 아무것도 안 한다.
        assert!(!needs_return("main", "main"));
        assert!(!needs_return("", "feat"));
    }

    #[test]
    fn branch_ids_are_32_hex_digits() {
        assert!(is_branch_id(OLD));
        assert!(is_branch_id(&OLD.to_uppercase()));
        assert!(!is_branch_id(""));
        assert!(!is_branch_id(&OLD[..31]));
        assert!(!is_branch_id(&format!("{}g", &OLD[..31])));
    }

    #[test]
    fn restore_refuses_a_taken_name_or_a_live_branch() {
        // 보관 항목만 있으면(로컬·원격 중복 포함) 되살릴 수 있다.
        let archived = [branch(OLD, "feat", true), branch(OLD, "feat", true), branch("e726318bbc3fd75ac8733a7e030cc35b", "main", false)];
        assert_eq!(restore_refusal(&archived, OLD, "feat"), None);
        // 같은 이름의 다른 활성 브랜치가 있으면 거부한다.
        let taken = [branch(OLD, "feat", true), branch("01a1c12bf5de753fa910e6eb00f42bcc", "feat", false)];
        assert!(restore_refusal(&taken, OLD, "feat").unwrap().contains("already exists"));
        // 이미 활성인 id(대소문자 무관)는 되살릴 것이 없다.
        let live = [branch(OLD, "feat", false)];
        assert!(restore_refusal(&live, &OLD.to_uppercase(), "feat").unwrap().contains("not archived"));
    }
}

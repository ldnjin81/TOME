//! TOME core: talks to Lore in-process through its C API (`lore::interface`, the functions
//! declared in Lore's `lore.h`).
//!
//! Every Lore function is asynchronous: it returns at once and reports through a callback that
//! runs on a Lore worker thread. [`call`] turns one call into a [`CallResult`]: every event,
//! copied out as JSON while the callback runs (event data is only valid until it returns), and
//! the final status.

use std::sync::mpsc;

pub mod model;
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

/// The repositories on the server at `url` (`lore://host:port`).
pub fn list_repositories(url: &str) -> CallResult {
    let args = lore::repository::LoreRepositoryListArgs { url: LoreString::from_bytes(url.as_bytes()) };
    call(interface::lore_repository_list_async, &LoreGlobalArgs::default(), &args)
}

/// Global arguments for calls on the repository at `path`.
pub struct Repository {
    path: String,
    /// Run without contacting the server (local data only).
    pub offline: bool,
}

impl Repository {
    pub fn open(path: impl Into<String>) -> Self {
        Repository { path: path.into(), offline: false }
    }

    fn globals(&self) -> LoreGlobalArgs {
        LoreGlobalArgs {
            repository_path: LoreString::from_bytes(self.path.as_bytes()),
            // Relative paths in a call (stage, lock, ...) are relative to the working copy root.
            working_directory: LoreString::from_bytes(self.path.as_bytes()),
            offline: self.offline as u8,
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
        let globals = LoreGlobalArgs { repository_path: LoreString::from_bytes(self.path.as_bytes()), ..Default::default() };
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

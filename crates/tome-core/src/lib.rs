//! TOME core: talks to Lore in-process through its C API (`lore::interface`, the functions
//! declared in Lore's `lore.h`).
//!
//! Every Lore function is asynchronous: it returns at once and reports through a callback that
//! runs on a Lore worker thread. [`call`] turns one call into a [`CallResult`]: every event,
//! copied out as JSON while the callback runs (event data is only valid until it returns), and
//! the final status.

use std::sync::mpsc;

pub mod model;

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
            offline: self.offline as u8,
            ..Default::default()
        }
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

//! Long operations (clone, sync, push) with progress, run in a separate process so they can be
//! cancelled.
//!
//! Lore v0.10.0 has no way to cancel one call; its own CLI simply ends when interrupted and the
//! next run picks up from what was stored. TOME does the same: the app starts itself in worker
//! mode ([`worker_main`]) for the operation, reads its progress lines, and ends the process to
//! cancel.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{CallResult, Repository, call_watching, interface};
use lore::interface::LoreString;

/// What to run, as the worker receives it (JSON).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Op {
    /// Clone `url` into the empty folder `path` with the `.lore/view` text `view`.
    Clone { path: String, url: String, view: String },
    /// Bring the working copy at `path` to its branch's latest revision.
    Sync { path: String },
    /// Push `branch` of the working copy at `path`.
    Push { path: String, branch: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub op: Op,
    pub identity: String,
}

/// Progress of a long operation, the same shape for each kind.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Progress {
    /// "clone", "sync" or "push".
    pub phase: String,
    /// Files (clone, sync) or fragments (push) done, of `total`.
    pub done: u64,
    pub total: u64,
    pub bytes: u64,
    pub bytes_total: u64,
}

fn number(value: &Value) -> u64 {
    value.as_u64().unwrap_or(0)
}

/// The progress an event carries, if it is a progress event.
pub fn progress_of(event: &Value) -> Option<Progress> {
    let data = &event["data"];
    match event["tagName"].as_str()? {
        "repositoryCloneProgress" => {
            let count = &data["count"];
            Some(Progress { phase: "clone".into(), done: number(&count["fileComplete"]), total: number(&count["fileCount"]), bytes: number(&count["bytesTransferred"]), bytes_total: number(&count["bytesTotal"]) })
        }
        "revisionSyncProgress" => Some(Progress {
            phase: "sync".into(),
            done: number(&data["fileUpdate"]) + number(&data["fileDelete"]),
            total: number(&data["fileUpdateTotal"]) + number(&data["fileDeleteTotal"]),
            bytes: number(&data["bytesUpdate"]),
            bytes_total: number(&data["bytesUpdateTotal"]),
        }),
        "branchPushFragmentProgress" => Some(Progress { phase: "push".into(), done: number(&data["complete"]), total: number(&data["count"]), bytes: number(&data["bytesTransferred"]), bytes_total: number(&data["bytesTotal"]) }),
        _ => None,
    }
}

/// Runs `job` here, telling `on_progress` at most every 100 ms (and on the last event).
pub fn run(job: &Job, on_progress: &mut dyn FnMut(&Progress)) -> CallResult {
    let mut last = Instant::now() - Duration::from_secs(1);
    let mut pending: Option<Progress> = None;
    let mut watch = |event: &Value| {
        if let Some(progress) = progress_of(event) {
            if last.elapsed() >= Duration::from_millis(100) {
                on_progress(&progress);
                last = Instant::now();
                pending = None;
            } else {
                pending = Some(progress);
            }
        }
    };
    let result = match &job.op {
        Op::Clone { path, url, view } => {
            let mut repository = Repository::open(path.clone());
            repository.identity = job.identity.clone();
            let args = lore::repository::LoreRepositoryCloneArgs {
                repository_url: LoreString::from_bytes(url.as_bytes()),
                view: LoreString::from_bytes(view.as_bytes()),
                ..Default::default()
            };
            std::fs::create_dir_all(path).ok();
            let globals = lore::interface::LoreGlobalArgs {
                repository_path: LoreString::from_bytes(path.as_bytes()),
                identity: LoreString::from_bytes(job.identity.as_bytes()),
                ..Default::default()
            };
            call_watching(interface::lore_repository_clone_async, &globals, &args, &mut watch)
        }
        Op::Sync { path } => {
            let mut repository = Repository::open(path.clone());
            repository.identity = job.identity.clone();
            let args = lore::revision::LoreRevisionSyncArgs {
                revision: LoreString::default(),
                forward_changes: 1,
                reset: 0,
                root_files: interface::LoreArray::default(),
                dependency_tags: interface::LoreArray::default(),
                dependency_recursive: 0,
                dependency_depth_limit: 0,
            };
            call_watching(interface::lore_revision_sync_async, &repository.globals(), &args, &mut watch)
        }
        Op::Push { path, branch } => {
            let mut repository = Repository::open(path.clone());
            repository.identity = job.identity.clone();
            let args = lore::branch::LoreBranchPushArgs { branch: LoreString::from_bytes(branch.as_bytes()), fast_forward_merge: 0 };
            call_watching(interface::lore_branch_push_async, &repository.globals(), &args, &mut watch)
        }
    };
    if let Some(progress) = pending {
        on_progress(&progress);
    }
    result
}

/// One line of the worker's output.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Line {
    Progress(Progress),
    Done { status: i32, error: String },
}

/// Worker mode: runs the job given as JSON and prints one [`Line`] per line (JSON) on stdout.
/// Returns the process exit code.
pub fn worker_main(job_json: &str) -> i32 {
    use std::io::Write;
    let print = |line: &Line| {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{}", serde_json::to_string(line).unwrap_or_default());
        let _ = out.flush();
    };
    let job: Job = match serde_json::from_str(job_json) {
        Ok(job) => job,
        Err(error) => {
            print(&Line::Done { status: -1, error: format!("bad job: {error}") });
            return 2;
        }
    };
    let result = run(&job, &mut |progress| print(&Line::Progress(progress.clone())));
    // Lore writes some state (the new current revision) lazily; flush it and shut Lore down
    // before the process ends, or the next run finds the files written but the revision not.
    let path = match &job.op {
        Op::Clone { path, .. } | Op::Sync { path } | Op::Push { path, .. } => path.clone(),
    };
    finish(&path);
    print(&Line::Done { status: result.status, error: result.error.clone() });
    if result.ok() { 0 } else { 1 }
}

/// Flushes the working copy's stored state and shuts Lore down (call once, at the end).
pub fn finish(path: &str) {
    if !path.is_empty() {
        let mut repository = Repository::open(path.to_string());
        repository.offline = true;
        crate::call(interface::lore_repository_flush_async, &repository.globals(), &lore::repository::LoreRepositoryFlushArgs {});
    }
    interface::lore_shutdown();
}

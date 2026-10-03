//! Restack: move my unpushed revisions (the stack) onto another base, or reorder them, by
//! resetting the branch to the base and cherry-picking each revision again in order.
//!
//! Lore v0.10.0 has cherry-pick in its library (`lore::revision::cherry_pick`) but not in the C
//! interface, so [`cherry_pick_async`] and friends start the library calls the way the C
//! interface functions do, and [`crate::call`] takes them like any other.

use lore::interface::{LoreEventCallbackConfig, LoreGlobalArgs};
use lore::revision::{
    LoreRevisionCherryPickAbortArgs, LoreRevisionCherryPickArgs, LoreRevisionCherryPickResolveArgs, LoreRevisionCherryPickResolveMineArgs,
    LoreRevisionCherryPickResolveTheirsArgs,
};
use lore_revision::event::{LoreCompleteEventData, LoreEndEventData, LoreEvent};

/// Starts a library call on Lore's runtime with the events going to `config`, like the C
/// interface's `run_asynchronously` (which is private to Lore).
fn spawn<A, F, Fut>(globals: &LoreGlobalArgs, args: &A, config: LoreEventCallbackConfig, handler: F)
where
    A: Clone + Send + 'static,
    F: FnOnce(LoreGlobalArgs, A, lore::interface::LoreEventCallback) -> Fut,
    Fut: Future<Output = i32> + Send + 'static,
{
    lore::size_threads_for_relaying();
    let callback = lore_revision::event::convert_event_callback(config);
    let mut globals = globals.clone();
    if let Err(error) = globals.validate() {
        if let Some(callback) = callback {
            let mut detail = lore_revision::event::LoreErrorDetail::default();
            detail.error_code = -1;
            detail.message = lore::interface::LoreString::from_bytes(format!("invalid arguments: {error}").as_bytes());
            callback(&LoreEvent::Complete(LoreCompleteEventData { status: -1, error: detail }));
            callback(&LoreEvent::End(LoreEndEventData::default()));
        }
        return;
    }
    let call = handler(globals, args.clone(), callback);
    drop(lore::runtime().spawn(call));
}

pub extern "C" fn cherry_pick_async(globals: &LoreGlobalArgs, args: &LoreRevisionCherryPickArgs, config: LoreEventCallbackConfig) {
    spawn(globals, args, config, lore::revision::cherry_pick);
}

pub extern "C" fn cherry_pick_abort_async(globals: &LoreGlobalArgs, args: &LoreRevisionCherryPickAbortArgs, config: LoreEventCallbackConfig) {
    spawn(globals, args, config, lore::revision::cherry_pick_abort);
}

pub extern "C" fn cherry_pick_resolve_async(globals: &LoreGlobalArgs, args: &LoreRevisionCherryPickResolveArgs, config: LoreEventCallbackConfig) {
    spawn(globals, args, config, lore::revision::cherry_pick_resolve);
}

pub extern "C" fn cherry_pick_resolve_mine_async(globals: &LoreGlobalArgs, args: &LoreRevisionCherryPickResolveMineArgs, config: LoreEventCallbackConfig) {
    spawn(globals, args, config, lore::revision::cherry_pick_resolve_mine);
}

pub extern "C" fn cherry_pick_resolve_theirs_async(globals: &LoreGlobalArgs, args: &LoreRevisionCherryPickResolveTheirsArgs, config: LoreEventCallbackConfig) {
    spawn(globals, args, config, lore::revision::cherry_pick_resolve_theirs);
}

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::model::{self, Revision};
use crate::{CallResult, Repository, Resolution};

/// How many revisions are read back when looking for where my stack leaves the server's history.
const DEPTH: u32 = 200;

/// My stack against the server: what restack moves, and what it can move onto.
#[derive(Debug, Clone, Serialize)]
pub struct Stack {
    /// My revisions on this branch that the server does not have, newest first.
    pub drafts: Vec<Revision>,
    /// Where the stack leaves the server's history (the last revision both have); None when
    /// there are no drafts or nothing in common was found.
    pub fork: Option<Revision>,
    /// Server revisions after the fork that I do not have yet, newest first.
    pub incoming: Vec<Revision>,
    /// The server's latest revision on this branch (empty when unknown).
    pub remote_head: String,
}

impl Repository {
    /// My stack on the current branch. Reads the server's history (online) when the server's
    /// head is not mine; offline, `incoming` is empty and drafts are told by revision number.
    pub fn stack(&self) -> Result<Stack, String> {
        let status = model::status(&self.status()).ok_or("status returned nothing")?;
        let local = model::history(&self.history(&status.branch_name, DEPTH));
        let remote_head = status.remote_revision.clone();
        if remote_head.is_empty() || remote_head == status.revision {
            return Ok(Stack { drafts: Vec::new(), fork: None, incoming: Vec::new(), remote_head });
        }
        let online = Repository { path: self.path.clone(), offline: false, identity: self.identity.clone() };
        let remote_result = online.history_from(&remote_head, DEPTH);
        if !remote_result.ok() {
            return Err(remote_result.error);
        }
        let remote = model::history(&remote_result);
        let on_server: HashSet<&str> = remote.iter().map(|r| r.id.as_str()).collect();
        let fork_at = local.iter().position(|r| on_server.contains(r.id.as_str()));
        let drafts = local[..fork_at.unwrap_or(local.len())].to_vec();
        let fork = fork_at.map(|i| local[i].clone());
        let mine: HashSet<&str> = local.iter().map(|r| r.id.as_str()).collect();
        let incoming = remote.iter().take_while(|r| !mine.contains(r.id.as_str())).cloned().collect();
        Ok(Stack { drafts, fork, incoming, remote_head })
    }
}

/// One revision to apply again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pick {
    pub id: String,
    pub message: String,
}

/// A restack: reset the branch to `onto`, then apply `picks` in order (oldest first).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub onto: String,
    pub picks: Vec<Pick>,
    /// The branch head before the restack, to go back to on abort.
    pub original_head: String,
}

/// A file a pick changes that the new order puts different changes under (the new base's, or
/// another pick's that moved across it): it may conflict.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Risk {
    pub path: String,
    /// Binary files cannot be merged: one side has to be chosen.
    pub binary: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PickPreview {
    pub id: String,
    pub message: String,
    pub files: Vec<String>,
    pub risks: Vec<Risk>,
    /// Files it changes that someone else has locked: (path, owner).
    pub locked: Vec<(String, String)>,
    /// A merge revision: applied again it becomes an ordinary revision (the merge link is lost).
    pub merge: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Preview {
    pub picks: Vec<PickPreview>,
    /// Files changed between the old base and the new one.
    pub base_changes: Vec<String>,
}

/// What a run of the plan ended with.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Step {
    /// Every pick applied: the new head.
    Done {
        head: String,
        /// Picks whose changes the new base already had: they would have made empty
        /// revisions, so they were left out.
        skipped: Vec<String>,
    },
    /// The pick at `index` stopped on these conflicting files; settle them, then continue.
    Conflict { index: usize, files: Vec<String> },
}

fn changed_files(repository: &Repository, revision: &str) -> Result<Vec<String>, String> {
    let result = repository.changes(revision);
    if !result.ok() {
        return Err(result.error);
    }
    Ok(model::delta_files(&result).into_iter().filter(|f| !f.directory).map(|f| f.path).collect())
}

fn ok(result: CallResult, what: &str) -> Result<CallResult, String> {
    if result.ok() { Ok(result) } else { Err(format!("{what}: {}", result.error)) }
}

/// Which files each pick may conflict on. `old_order` is the stack oldest first as it is now,
/// `old_base` the revision it sits on; `locks` are (path, owner) on this branch, `me` my identity.
pub fn preview(repository: &Repository, plan: &Plan, old_base: &str, old_order: &[String], locks: &[(String, String)], me: &str) -> Result<Preview, String> {
    let base_changes = if plan.onto == old_base {
        Vec::new()
    } else {
        let online = Repository { path: repository.path.clone(), offline: false, identity: repository.identity.clone() };
        let result = ok(online.revision_diff(old_base, &plan.onto), "comparing the bases")?;
        model::diff_files(&result).into_iter().filter(|f| !f.directory).map(|f| f.path).collect()
    };
    let mut files: HashMap<&str, Vec<String>> = HashMap::new();
    for pick in &plan.picks {
        files.insert(&pick.id, changed_files(repository, &pick.id)?);
    }
    let old_position = |id: &str| old_order.iter().position(|o| o == id);
    let mut merges = HashSet::new();
    for pick in &plan.picks {
        let info = ok(repository.changes(&pick.id), "reading the revision")?;
        // The second parent of a merge; all zeros otherwise.
        let merged = info.data("revisionInfo").next().and_then(|d| d["parent"][1].as_str()).is_some_and(|p| !p.is_empty() && p != model::NO_HASH);
        if merged {
            merges.insert(pick.id.as_str());
        }
    }
    let picks = plan
        .picks
        .iter()
        .enumerate()
        .map(|(i, pick)| {
            // What it sits on that differs from before: the new base's changes, picks moved before
            // it, and picks it used to sit on that now come after it.
            let mut before: HashSet<&str> = base_changes.iter().map(String::as_str).collect();
            for (j, other) in plan.picks.iter().enumerate() {
                let moved = if j < i { old_position(&other.id) > old_position(&pick.id) } else { j > i && old_position(&other.id) < old_position(&pick.id) };
                if moved {
                    before.extend(files[other.id.as_str()].iter().map(String::as_str));
                }
            }
            let mine = &files[pick.id.as_str()];
            PickPreview {
                id: pick.id.clone(),
                message: pick.message.clone(),
                risks: mine.iter().filter(|f| before.contains(f.as_str())).map(|f| Risk { path: f.clone(), binary: model::is_binary_path(f) }).collect(),
                locked: locks.iter().filter(|(path, owner)| owner != me && mine.contains(path)).cloned().collect(),
                merge: merges.contains(pick.id.as_str()),
                files: mine.clone(),
            }
        })
        .collect();
    Ok(Preview { picks, base_changes })
}

/// Applies the plan's picks from `from` on; with `from == 0` it first resets the branch to
/// `onto` (the working copy must have no changes). Stops at the first pick that conflicts.
/// A pick that changes nothing on the new base (the base already has it) is left out. If
/// anything fails after the branch was moved, the branch and files are put back as they were.
pub fn run(repository: &Repository, plan: &Plan, from: usize) -> Result<Step, String> {
    if from == 0 {
        let status = model::status(&ok(repository.scan_status(), "status")?).ok_or("status returned nothing")?;
        if !status.merging.is_empty() {
            return Err("a merge is in progress: finish or abort it first".into());
        }
        if status.files.iter().any(|f| !f.directory) {
            return Err("the working copy has uncommitted changes: commit or revert them first".into());
        }
    }
    apply(repository, plan, from).map_err(|error| match abort(repository, plan) {
        Ok(()) => format!("{error}; the branch and files were put back as they were before the restack"),
        Err(back) => format!("{error}; putting the branch back also failed ({back}): reset it to {} by hand", plan.original_head),
    })
}

fn head(repository: &Repository) -> Result<String, String> {
    Ok(model::status(&repository.status()).ok_or("status returned nothing")?.revision)
}

fn apply(repository: &Repository, plan: &Plan, from: usize) -> Result<Step, String> {
    if from == 0 {
        ok(repository.branch_reset(&plan.onto), "resetting the branch")?;
        ok(repository.sync_to(&plan.onto, false), "syncing to the new base")?;
    }
    let mut skipped = Vec::new();
    for (index, pick) in plan.picks.iter().enumerate().skip(from) {
        let before = head(repository)?;
        let result = ok(repository.cherry_pick(&pick.id, &pick.message), &format!("applying \"{}\"", pick.message))?;
        let conflicted = result.data("cherryPickStartEnd").any(|d| d["hasConflicts"].as_u64().unwrap_or(0) != 0);
        if conflicted {
            let files = result.data("cherryPickConflictFile").filter_map(|d| d["path"].as_str().map(str::to_string)).collect();
            return Ok(Step::Conflict { index, files });
        }
        if drop_if_empty(repository, &before)? {
            skipped.push(pick.id.clone());
        }
    }
    Ok(Step::Done { head: head(repository)?, skipped })
}

/// Lore commits a pick even when it changes nothing; such a revision is taken off the branch
/// again (the files are the same either way). True when it was.
fn drop_if_empty(repository: &Repository, before: &str) -> Result<bool, String> {
    let now = head(repository)?;
    if now == before {
        return Ok(false);
    }
    // The revision's own delta cannot tell: a cherry-pick lists every file it merged as
    // "keep", changed or not. Compare the two revisions' files instead.
    let diff = ok(repository.revision_diff(before, &now), "checking for an empty revision")?;
    if model::diff_files(&diff).iter().any(|f| !f.directory) {
        return Ok(false);
    }
    ok(repository.branch_reset(before), "leaving out an empty revision")?;
    Ok(true)
}

/// The side to keep for a conflicted file of a pick. In a cherry-pick Lore's "mine" is the
/// revision being built on (the new base) and "theirs" the picked revision, so keeping my
/// change is Lore's theirs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Keep {
    /// My revision's version of the file.
    Mine,
    /// The new base's version.
    Base,
    /// Edited by hand in the working copy.
    Edited,
}

pub fn resolve(repository: &Repository, paths: &[String], keep: Keep) -> Result<(), String> {
    let how = match keep {
        Keep::Mine => Resolution::Theirs,
        Keep::Base => Resolution::Mine,
        Keep::Edited => Resolution::Edited,
    };
    ok(repository.cherry_pick_resolve(paths, how), "settling the conflict").map(|_| ())
}

/// Commits the settled pick at `index` and applies the rest of the plan.
pub fn continue_after(repository: &Repository, plan: &Plan, index: usize) -> Result<Step, String> {
    let status = model::status(&ok(repository.scan_status(), "status")?).ok_or("status returned nothing")?;
    let open: Vec<&str> = status.files.iter().filter(|f| f.unresolved).map(|f| f.path.as_str()).collect();
    if !open.is_empty() {
        return Err(format!("conflicts not settled yet: {}", open.join(", ")));
    }
    let pick = plan.picks.get(index).ok_or("no such pick")?;
    // Committed by hand already (or settled to nothing): there is nothing left to commit.
    if status.files.iter().any(|f| !f.directory) {
        let before = head(repository)?;
        ok(repository.commit(&pick.message), &format!("committing \"{}\"", pick.message))?;
        drop_if_empty(repository, &before)?;
    }
    run(repository, plan, index + 1)
}

/// Gives up: drops a pick in progress and puts the branch and working copy back as they were.
pub fn abort(repository: &Repository, plan: &Plan) -> Result<(), String> {
    // Fails harmlessly when no pick is in progress.
    let _ = repository.cherry_pick_abort();
    ok(repository.branch_reset(&plan.original_head), "resetting the branch back")?;
    ok(repository.sync_to(&plan.original_head, true), "syncing back")?;
    Ok(())
}

//! The TOME window. Commands run Lore calls on a blocking thread and return the UI models.

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};
use tome_core::model::{self, Branch, Lock, RemoteRepository, Revision, Status};
use tome_core::view::ViewChange;
use tome_core::tools::{self, Selection, Tool};
use tome_core::{CallResult, Repository, Resolution, restack};

/// A repository opened in the window: its working copy, branches and history.
#[derive(Serialize)]
struct Overview {
    status: Status,
    branches: Vec<Branch>,
    history: Vec<Revision>,
    /// The Lore command line for what was just done (shown in the status bar).
    commands: Vec<String>,
}

/// The result of a command with the Lore command lines that do the same.
#[derive(Serialize)]
struct Done<T> {
    value: T,
    commands: Vec<String>,
}

/// The identity from the settings, sent with every Lore call (set when settings are read or saved).
static IDENTITY: std::sync::RwLock<String> = std::sync::RwLock::new(String::new());

fn identity() -> String {
    IDENTITY.read().map(|i| i.clone()).unwrap_or_default()
}

fn repository(path: &str, offline: bool) -> Repository {
    let mut repository = Repository::open(path);
    repository.offline = offline;
    repository.identity = identity();
    repository
}

fn checked(result: CallResult) -> Result<CallResult, String> {
    if result.ok() { Ok(result) } else { Err(result.error) }
}

/// Quotes a command-line argument when it needs it.
fn arg(text: &str) -> String {
    if !text.is_empty() && text.chars().all(|c| c.is_alphanumeric() || "/._-".contains(c)) {
        text.to_string()
    } else {
        format!("\"{}\"", text.replace('"', "\\\""))
    }
}

fn args(paths: &[String]) -> String {
    paths.iter().map(|p| arg(p)).collect::<Vec<_>>().join(" ")
}

async fn blocking<T: Send + 'static>(work: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work).await.map_err(|error| error.to_string())?
}

fn open(path: String, offline: bool, length: u32) -> Result<Overview, String> {
    let repository = repository(&path, offline);
    let status_result = repository.status();
    let status = model::status(&status_result).ok_or_else(|| {
        if status_result.error.is_empty() { format!("not a Lore working copy: {path}") } else { status_result.error.clone() }
    })?;
    let branches = model::branches(&repository.branches());
    let history = model::history(&repository.history(&status.branch_name, length));
    let flag = if offline { " --offline" } else { "" };
    Ok(Overview {
        commands: vec![
            format!("lore status{flag}"),
            format!("lore branch list{flag}"),
            format!("lore history {length} --branch {}{flag}", status.branch_name),
        ],
        status,
        branches,
        history,
    })
}

fn scanned_status(repository: &Repository) -> Result<Status, String> {
    model::status(&checked(repository.scan_status())?).ok_or_else(|| "status returned nothing".to_string())
}

#[tauri::command]
async fn open_repository(path: String, offline: bool) -> Result<Overview, String> {
    blocking(move || open(path, offline, 200)).await
}

#[tauri::command]
async fn branch_history(path: String, branch: String, offline: bool) -> Result<Vec<Revision>, String> {
    blocking(move || Ok(model::history(&checked(repository(&path, offline).history(&branch, 200))?))).await
}

/// The working copy after a scan, so new and edited files show up.
#[tauri::command]
async fn working_status(path: String, offline: bool) -> Result<Done<Status>, String> {
    blocking(move || {
        let flag = if offline { " --offline" } else { "" };
        Ok(Done { value: scanned_status(&repository(&path, offline))?, commands: vec![format!("lore status --scan{flag}")] })
    })
    .await
}

/// Stages (`stage` true) or unstages `paths`, then returns the new status.
#[tauri::command]
async fn stage_files(path: String, paths: Vec<String>, stage: bool) -> Result<Done<Status>, String> {
    blocking(move || {
        let repository = repository(&path, true);
        checked(if stage { repository.stage(&paths) } else { repository.unstage(&paths) })?;
        let verb = if stage { "stage" } else { "unstage" };
        Ok(Done { value: scanned_status(&repository)?, commands: vec![format!("lore {verb} {}", args(&paths))] })
    })
    .await
}

/// Commits the staged files, and pushes the branch when `push` is set.
#[tauri::command]
async fn commit(path: String, message: String, push: bool) -> Result<Done<Status>, String> {
    blocking(move || {
        let repository = repository(&path, !push);
        checked(repository.commit(&message))?;
        let mut commands = vec![format!("lore commit {}", arg(&message))];
        if push {
            let branch = scanned_status(&repository)?.branch_name;
            checked(repository.push(&branch))?;
            commands.push("lore push".into());
        }
        Ok(Done { value: scanned_status(&repository)?, commands })
    })
    .await
}

/// Every lock on `branch` (asks the server).
#[tauri::command]
async fn lock_board(path: String, branch: String) -> Result<Done<Vec<Lock>>, String> {
    blocking(move || {
        let locks = model::locks(&checked(repository(&path, false).locks(&branch))?);
        Ok(Done { value: locks, commands: vec![format!("lore lock query --branch {}", arg(&branch))] })
    })
    .await
}

/// Locks (`lock` true) or releases `paths`, then returns the board.
#[tauri::command]
async fn lock_files(path: String, branch: String, paths: Vec<String>, lock: bool) -> Result<Done<Vec<Lock>>, String> {
    blocking(move || {
        let repository = repository(&path, false);
        checked(if lock { repository.lock(&branch, &paths) } else { repository.unlock(&branch, &paths) })?;
        let locks = model::locks(&checked(repository.locks(&branch))?);
        let verb = if lock { "acquire" } else { "release" };
        Ok(Done { value: locks, commands: vec![format!("lore lock {verb} {}", args(&paths))] })
    })
    .await
}

#[tauri::command]
async fn read_view(path: String) -> Result<Vec<String>, String> {
    blocking(move || Ok(Repository::open(path).view())).await
}

/// Writes `.lore/view` and makes the working files match it.
#[tauri::command]
async fn apply_view(path: String, lines: Vec<String>) -> Result<Done<ViewChange>, String> {
    blocking(move || {
        let change = repository(&path, true).apply_view(&lines)?;
        let mut commands = vec!["edit .lore/view".to_string()];
        if !change.restored.is_empty() {
            commands.push(format!("lore reset {}", args(&change.restored)));
        }
        Ok(Done { value: change, commands })
    })
    .await
}

/// Long operations running in worker processes, by job id.
#[derive(Default)]
struct Jobs {
    next: std::sync::atomic::AtomicU64,
    running: std::sync::Mutex<std::collections::HashMap<u64, RunningJob>>,
}

struct RunningJob {
    child: std::process::Child,
    /// A clone's target folder: removed when the clone is cancelled (it was empty before).
    clone_path: Option<String>,
    cancelled: bool,
}

/// What the window hears about a job (`lore-job` events).
#[derive(Serialize, Clone)]
struct JobEvent {
    id: u64,
    progress: Option<tome_core::ops::Progress>,
    /// Set once at the end: (status, error).
    done: Option<(i32, String)>,
    cancelled: bool,
}

/// Starts a clone, sync or push in a worker process and returns its id; progress and the end
/// arrive as `lore-job` events.
#[tauri::command]
fn start_job(app: tauri::AppHandle, op: tome_core::ops::Op) -> Result<u64, String> {
    use std::io::BufRead;
    let clone_path = match &op {
        tome_core::ops::Op::Clone { path, .. } => {
            if std::fs::read_dir(path).is_ok_and(|mut entries| entries.next().is_some()) {
                return Err(format!("폴더가 비어 있지 않습니다: {path}"));
            }
            Some(path.clone())
        }
        _ => None,
    };
    let job = tome_core::ops::Job { op, identity: identity() };
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut child = std::process::Command::new(exe)
        .arg("--lore-worker")
        .arg(serde_json::to_string(&job).map_err(|e| e.to_string())?)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("작업 프로세스를 시작할 수 없습니다: {e}"))?;
    let stdout = child.stdout.take().ok_or("no worker output")?;
    let jobs = app.state::<Jobs>();
    let id = jobs.next.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    jobs.running.lock().unwrap().insert(id, RunningJob { child, clone_path, cancelled: false });
    std::thread::spawn(move || {
        let mut done = None;
        for line in std::io::BufReader::new(stdout).lines().map_while(Result::ok) {
            match serde_json::from_str::<tome_core::ops::Line>(&line) {
                Ok(tome_core::ops::Line::Progress(progress)) => {
                    let _ = app.emit("lore-job", JobEvent { id, progress: Some(progress), done: None, cancelled: false });
                }
                Ok(tome_core::ops::Line::Done { status, error }) => done = Some((status, error)),
                Err(_) => {}
            }
        }
        let finished = app.state::<Jobs>().running.lock().unwrap().remove(&id);
        let cancelled = finished.as_ref().is_some_and(|j| j.cancelled);
        if let Some(mut job) = finished {
            let _ = job.child.wait();
            if cancelled && let Some(path) = job.clone_path {
                // The clone had an empty or new folder to itself; a cancelled one is thrown away.
                let _ = std::fs::remove_dir_all(path);
            }
        }
        let done = done.unwrap_or((-2, if cancelled { "취소함".into() } else { "작업 프로세스가 결과 없이 끝났습니다".into() }));
        let _ = app.emit("lore-job", JobEvent { id, progress: None, done: Some(done), cancelled });
    });
    Ok(id)
}

/// Ends a running job's worker process (Lore picks up from what it stored on the next run).
#[tauri::command]
fn cancel_job(app: tauri::AppHandle, id: u64) -> Result<(), String> {
    let jobs = app.state::<Jobs>();
    let mut running = jobs.running.lock().unwrap();
    let job = running.get_mut(&id).ok_or("이미 끝난 작업입니다")?;
    job.cancelled = true;
    job.child.kill().map_err(|e| e.to_string())
}

/// The notification subscription of the open working copy (one at a time).
#[derive(Default)]
struct Watch(std::sync::Mutex<Option<tome_core::notify::Subscription>>);

/// Subscribes to the server's notifications for the working copy at `path`; each one is sent
/// to the window as a `lore-notification` event. Replaces the previous subscription.
#[tauri::command]
async fn watch_repository(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let previous = app.state::<Watch>().0.lock().unwrap().take();
    blocking(move || {
        drop(previous);
        let sender = app.clone();
        let subscription = repository(&path, false).subscribe(move |notification| {
            let _ = sender.emit("lore-notification", notification);
        })?;
        *app.state::<Watch>().0.lock().unwrap() = Some(subscription);
        Ok(())
    })
    .await
}

/// The working copy and its branches after a branch command.
#[derive(Serialize)]
struct BranchState {
    status: Status,
    branches: Vec<Branch>,
}

fn branch_state(repository: &Repository) -> Result<BranchState, String> {
    Ok(BranchState { status: scanned_status(repository)?, branches: model::branches(&checked(repository.branches())?) })
}

/// Lore's refusal to switch over local edits, in words that say what to do.
fn explain(error: String) -> String {
    if error.contains("Local modifications") {
        "커밋하지 않은 변경이 있어 전환할 수 없습니다. 커밋하거나 변경을 되돌린 뒤 다시 하세요.".into()
    } else {
        error
    }
}

/// Creates `name` at the current revision, and switches to it when `switch` is set.
#[tauri::command]
async fn create_branch(path: String, name: String, switch: bool) -> Result<Done<BranchState>, String> {
    blocking(move || {
        let repository = repository(&path, false);
        checked(repository.create_branch(&name))?;
        let mut commands = vec![format!("lore branch create {}", arg(&name))];
        if switch {
            checked(repository.switch_branch(&name)).map_err(explain)?;
            commands.push(format!("lore branch switch {}", arg(&name)));
        }
        Ok(Done { value: branch_state(&repository)?, commands })
    })
    .await
}

#[tauri::command]
async fn switch_branch(path: String, name: String) -> Result<Done<BranchState>, String> {
    blocking(move || {
        let repository = repository(&path, false);
        checked(repository.switch_branch(&name)).map_err(explain)?;
        Ok(Done { value: branch_state(&repository)?, commands: vec![format!("lore branch switch {}", arg(&name))] })
    })
    .await
}

/// Merges `from` into the current branch: commits on its own when nothing conflicts, otherwise
/// leaves a merge in progress (status.merging) with the conflicted files to settle.
#[tauri::command]
async fn merge_branch(path: String, from: String, message: String) -> Result<Done<Status>, String> {
    blocking(move || {
        let repository = repository(&path, false);
        checked(repository.merge_branch(&from, &message)).map_err(explain)?;
        Ok(Done { value: scanned_status(&repository)?, commands: vec![format!("lore branch merge {} --message {}", arg(&from), arg(&message))] })
    })
    .await
}

/// Settles conflicted files (`how`: mine, theirs, edited) or, with `how` empty, marks them conflicted again.
#[tauri::command]
async fn resolve_conflicts(path: String, paths: Vec<String>, how: Option<Resolution>) -> Result<Done<Status>, String> {
    blocking(move || {
        let repository = repository(&path, true);
        let command = match how {
            Some(how) => {
                checked(repository.merge_resolve(&paths, how))?;
                let verb = match how {
                    Resolution::Mine => "resolve-mine",
                    Resolution::Theirs => "resolve-theirs",
                    Resolution::Edited => "resolve",
                };
                format!("lore branch merge {verb} {}", args(&paths))
            }
            None => {
                checked(repository.merge_unresolve(&paths))?;
                format!("lore branch merge unresolve {}", args(&paths))
            }
        };
        Ok(Done { value: scanned_status(&repository)?, commands: vec![command] })
    })
    .await
}

#[tauri::command]
async fn abort_merge(path: String) -> Result<Done<Status>, String> {
    blocking(move || {
        let repository = repository(&path, true);
        checked(repository.merge_abort())?;
        Ok(Done { value: scanned_status(&repository)?, commands: vec!["lore branch merge abort".into()] })
    })
    .await
}

/// A restack stopped at a conflict, kept in `restacks.json` (working copy -> this) so it can be
/// continued or aborted after TOME restarts.
#[derive(Serialize, Deserialize, Clone)]
struct PendingRestack {
    plan: restack::Plan,
    /// The pick that stopped on a conflict.
    index: usize,
    files: Vec<String>,
}

fn restacks_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app.path().app_config_dir().map_err(|e| e.to_string())?.join("restacks.json"))
}

fn read_restacks(app: &tauri::AppHandle) -> std::collections::BTreeMap<String, PendingRestack> {
    restacks_path(app).ok().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn write_pending(app: &tauri::AppHandle, path: &str, pending: Option<PendingRestack>) -> Result<(), String> {
    let mut all = read_restacks(app);
    match pending {
        Some(p) => all.insert(path.to_string(), p),
        None => all.remove(path),
    };
    let file = restacks_path(app)?;
    std::fs::create_dir_all(file.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(file, serde_json::to_string_pretty(&all).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Where a restack run ended, with the status after it.
#[derive(Serialize)]
struct RestackOutcome {
    step: restack::Step,
    status: Status,
}

fn restack_outcome(app: &tauri::AppHandle, path: &str, repository: &Repository, plan: &restack::Plan, step: restack::Step) -> Result<RestackOutcome, String> {
    let pending = match &step {
        restack::Step::Conflict { index, files } => Some(PendingRestack { plan: plan.clone(), index: *index, files: files.clone() }),
        restack::Step::Done { .. } => None,
    };
    write_pending(app, path, pending)?;
    Ok(RestackOutcome { step, status: scanned_status(repository)? })
}

fn pick_commands(plan: &restack::Plan, from: usize) -> Vec<String> {
    plan.picks.iter().skip(from).map(|p| format!("lore revision cherry-pick {} --message {}", &p.id[..p.id.len().min(12)], arg(&p.message))).collect()
}

/// My stack against the server (reads the server's history).
#[tauri::command]
async fn stack_info(path: String) -> Result<restack::Stack, String> {
    blocking(move || repository(&path, false).stack()).await
}

/// Which files each pick of `plan` may conflict on, and others' locks on them.
#[tauri::command]
async fn restack_preview(path: String, plan: restack::Plan, old_base: String, old_order: Vec<String>) -> Result<restack::Preview, String> {
    blocking(move || {
        let repository = repository(&path, false);
        let branch = model::status(&repository.status()).map(|s| s.branch_name).unwrap_or_default();
        let locks: Vec<(String, String)> = model::locks(&repository.locks(&branch)).into_iter().map(|l| (l.path, l.owner)).collect();
        restack::preview(&repository, &plan, &old_base, &old_order, &locks, &identity())
    })
    .await
}

#[tauri::command]
async fn restack_start(app: tauri::AppHandle, path: String, plan: restack::Plan) -> Result<Done<RestackOutcome>, String> {
    blocking(move || {
        let repository = repository(&path, false);
        let step = restack::run(&repository, &plan, 0)?;
        let mut commands = vec![format!("lore branch reset {}", &plan.onto[..plan.onto.len().min(12)]), format!("lore revision sync {}", &plan.onto[..plan.onto.len().min(12)])];
        let upto = match &step {
            restack::Step::Conflict { index, .. } => index + 1,
            restack::Step::Done { .. } => plan.picks.len(),
        };
        commands.extend(pick_commands(&plan, 0).into_iter().take(upto));
        Ok(Done { value: restack_outcome(&app, &path, &repository, &plan, step)?, commands })
    })
    .await
}

/// Folds consecutive drafts into one revision, then applies the drafts above it again; a
/// conflict there stops like a restack (continue or abort the same way).
#[tauri::command]
async fn fold_drafts(app: tauri::AppHandle, path: String, fold: restack::Fold) -> Result<Done<RestackOutcome>, String> {
    blocking(move || {
        let repository = repository(&path, false);
        let (step, plan) = restack::fold(&repository, &fold)?;
        let short = |id: &str| id[..id.len().min(12)].to_string();
        let mut commands = vec![
            format!("lore branch reset {}", short(&fold.base)),
            format!("lore revision sync {}", short(&fold.base)),
            format!("lore file reset --revision {} <files changed in the group>", short(fold.group.last().map(String::as_str).unwrap_or(""))),
            format!("lore commit {}", arg(&fold.message)),
        ];
        commands.extend(pick_commands(&plan, 0));
        Ok(Done { value: restack_outcome(&app, &path, &repository, &plan, step)?, commands })
    })
    .await
}

/// The restack stopped at a conflict in this working copy, if any.
#[tauri::command]
fn restack_pending(app: tauri::AppHandle, path: String) -> Option<PendingRestack> {
    read_restacks(&app).remove(path.trim())
}

fn pending_for(app: &tauri::AppHandle, path: &str) -> Result<PendingRestack, String> {
    read_restacks(app).remove(path).ok_or_else(|| "진행 중인 restack이 없습니다".to_string())
}

/// Settles conflicted files of the stopped pick: keep my version, the base's, or my edit.
#[tauri::command]
async fn restack_resolve(path: String, paths: Vec<String>, keep: restack::Keep) -> Result<Done<Status>, String> {
    blocking(move || {
        let repository = repository(&path, true);
        restack::resolve(&repository, &paths, keep)?;
        // In a cherry-pick Lore's theirs is the picked revision (mine).
        let verb = match keep {
            restack::Keep::Mine => "resolve-theirs",
            restack::Keep::Base => "resolve-mine",
            restack::Keep::Edited => "resolve",
        };
        Ok(Done { value: scanned_status(&repository)?, commands: vec![format!("lore revision cherry-pick {verb} {}", args(&paths))] })
    })
    .await
}

/// Commits the settled pick and applies the rest.
#[tauri::command]
async fn restack_continue(app: tauri::AppHandle, path: String) -> Result<Done<RestackOutcome>, String> {
    blocking(move || {
        let pending = pending_for(&app, &path)?;
        let repository = repository(&path, false);
        let step = restack::continue_after(&repository, &pending.plan, pending.index).inspect_err(|error| {
            // A failure that put the branch back ends the restack: nothing is left to continue.
            if error.contains("put back as they were") {
                let _ = write_pending(&app, &path, None);
            }
        })?;
        let mut commands = vec![format!("lore commit {}", arg(&pending.plan.picks[pending.index].message))];
        commands.extend(pick_commands(&pending.plan, pending.index + 1));
        Ok(Done { value: restack_outcome(&app, &path, &repository, &pending.plan, step)?, commands })
    })
    .await
}

/// Gives up the stopped restack: the branch and files go back to how they were.
#[tauri::command]
async fn restack_abort(app: tauri::AppHandle, path: String) -> Result<Done<Status>, String> {
    blocking(move || {
        let pending = pending_for(&app, &path)?;
        let repository = repository(&path, false);
        restack::abort(&repository, &pending.plan)?;
        write_pending(&app, &path, None)?;
        let head = &pending.plan.original_head;
        Ok(Done {
            value: scanned_status(&repository)?,
            commands: vec!["lore revision cherry-pick abort".into(), format!("lore branch reset {}", &head[..head.len().min(12)]), format!("lore revision sync {} --reset", &head[..head.len().min(12)])],
        })
    })
    .await
}

/// Every branch's revisions laid out in lanes (the Smartlog's full view).
#[tauri::command]
async fn graph(path: String, offline: bool) -> Result<Done<tome_core::Graph>, String> {
    blocking(move || {
        let graph = repository(&path, offline).graph(300)?;
        let flag = if offline { " --offline" } else { "" };
        Ok(Done { value: graph, commands: vec![format!("lore branch list{flag}"), format!("lore history 300 --branch <each>{flag}")] })
    })
    .await
}

/// A revision's changed files (folders left out) with their diffs: against its first parent,
/// or none for a first revision (Lore has no empty side to diff from there).
#[derive(Serialize)]
struct RevisionChanges {
    files: Vec<model::DiffFile>,
    patches: Vec<model::FilePatch>,
    first_revision: bool,
}

#[tauri::command]
async fn revision_changes(path: String, revision: String, parent: String, offline: bool) -> Result<RevisionChanges, String> {
    blocking(move || {
        let repository = repository(&path, offline);
        let files: Vec<model::DiffFile> = model::delta_files(&checked(repository.changes(&revision))?).into_iter().filter(|f| !f.directory && f.action != "keep").collect();
        if parent.is_empty() {
            return Ok(RevisionChanges { files, patches: Vec::new(), first_revision: true });
        }
        // Text diffs for up to 300 files; binary ones come back as markers.
        let paths: Vec<String> = files.iter().take(300).map(|f| f.path.clone()).collect();
        let patches = if paths.is_empty() { Vec::new() } else { model::patches(&checked(repository.file_diff(&paths, &parent, &revision, 3))?) };
        Ok(RevisionChanges { files, patches, first_revision: false })
    })
    .await
}

/// The revisions that changed `file`, newest first (asks the server unless offline).
#[tauri::command]
async fn file_history(path: String, file: String, offline: bool) -> Result<Done<Vec<model::FileRevision>>, String> {
    blocking(move || {
        let history = repository(&path, offline).file_history(&file, 200)?;
        Ok(Done { value: history, commands: vec![format!("lore file history {}", arg(&file))] })
    })
    .await
}

/// One file's change in `revision` against `parent` (empty for a first revision: no diff).
#[tauri::command]
async fn file_patch(path: String, file: String, revision: String, parent: String, offline: bool) -> Result<Option<model::FilePatch>, String> {
    blocking(move || {
        if parent.is_empty() {
            return Ok(None);
        }
        let result = checked(repository(&path, offline).file_diff(&[file.clone()], &parent, &revision, 3))?;
        Ok(model::patches(&result).into_iter().next())
    })
    .await
}

/// The working copy's edits to `files` against the current revision.
#[tauri::command]
async fn working_patches(path: String, files: Vec<String>) -> Result<Vec<model::FilePatch>, String> {
    blocking(move || {
        let repository = repository(&path, true);
        let status = model::status(&checked(repository.status())?).ok_or("status returned nothing")?;
        Ok(model::patches(&checked(repository.file_diff(&files, &status.revision, "", 3))?))
    })
    .await
}

/// The repositories on a Lore server (`lore://host:port`).
#[tauri::command]
async fn list_repositories(server: String) -> Result<Done<Vec<RemoteRepository>>, String> {
    blocking(move || {
        let repositories = model::repositories(&checked(tome_core::list_repositories(&server, &identity()))?);
        Ok(Done { value: repositories, commands: vec![format!("lore repository list {}", arg(&server))] })
    })
    .await
}

/// A package's class and thumbnail for the asset grid; the image is a `data:` URL.
#[derive(Serialize, Clone)]
struct AssetPreview {
    path: String,
    /// Empty when the package has no thumbnail table (maps, data assets, unreadable files).
    class: String,
    image: Option<String>,
    width: i32,
    height: i32,
}

/// Previews already read: file path -> (size, modified, preview). A changed file is read again.
#[derive(Default)]
struct Previews(std::sync::Mutex<std::collections::HashMap<std::path::PathBuf, (u64, std::time::SystemTime, AssetPreview)>>);

fn data_url(mime: &str, data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = format!("data:{mime};base64,");
    out.reserve(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | (*b as u32) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= chunk.len() { ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

#[tauri::command]
async fn list_assets(path: String, folder: String) -> Result<tome_core::assets::Listing, String> {
    blocking(move || tome_core::assets::list(std::path::Path::new(path.trim()), &folder)).await
}

#[tauri::command]
async fn asset_previews(app: tauri::AppHandle, path: String, files: Vec<String>) -> Result<Vec<AssetPreview>, String> {
    blocking(move || {
        let root = std::path::PathBuf::from(path.trim());
        let previews = app.state::<Previews>();
        Ok(files
            .into_iter()
            .filter_map(|file| {
                let full = tome_core::assets::resolve(&root, &file)?;
                let meta = std::fs::metadata(&full).ok()?;
                let stamp = (meta.len(), meta.modified().ok()?);
                if let Some((size, modified, preview)) = previews.0.lock().unwrap().get(&full)
                    && (*size, *modified) == stamp
                {
                    return Some(preview.clone());
                }
                let found = tome_core::uasset::preview_file(&full);
                let thumbnail = found.as_ref().and_then(|p| p.thumbnail.as_ref());
                let preview = AssetPreview {
                    path: file,
                    class: found.as_ref().map(|p| p.class.clone()).unwrap_or_default(),
                    image: thumbnail.map(|t| data_url(t.mime, &t.data)),
                    width: thumbnail.map_or(0, |t| t.width),
                    height: thumbnail.map_or(0, |t| t.height),
                };
                previews.0.lock().unwrap().insert(full, (stamp.0, stamp.1, preview.clone()));
                Some(preview)
            })
            .collect())
    })
    .await
}

/// What TOME remembers between runs (`settings.json` in the app config folder).
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct Settings {
    /// Set once the first-run setup is finished.
    setup_done: bool,
    /// `lore://host:port` of the team's Lore server.
    server: String,
    /// Working copies opened, most recent first.
    recent: Vec<String>,
    offline: bool,
    /// Who I am to Lore: the author name on a server without authentication.
    identity: String,
    /// My own custom tools.
    tools: Vec<Tool>,
    /// Working copy root -> fingerprint of the `.tome/tools.json` content I trusted there.
    trusted_tools: std::collections::BTreeMap<String, String>,
    /// `programmer` (history and branches first) or `artist` (assets with thumbnails first).
    mode: String,
    /// Branch groups (the part before `/`, e.g. `auto`) folded away in the branch list.
    collapsed_branch_groups: Vec<String>,
}

fn settings_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app.path().app_config_dir().map_err(|e| e.to_string())?.join("settings.json"))
}

fn read_settings(app: &tauri::AppHandle) -> Result<Settings, String> {
    let path = settings_path(app)?;
    let settings: Settings = std::fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default();
    if let Ok(mut current) = IDENTITY.write() {
        *current = settings.identity.clone();
    }
    Ok(settings)
}

/// A name to suggest for the identity: the OS user name.
#[tauri::command]
fn default_identity() -> String {
    std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_default()
}

/// Whether the server needs a login and who is logged in on this machine.
#[tauri::command]
async fn auth_state(path: String) -> Result<tome_core::AuthState, String> {
    blocking(move || Ok(tome_core::auth_state(&path))).await
}

/// The tools for a working copy: mine, and the project's from `.tome/tools.json` with whether
/// I trusted its current content.
#[derive(Serialize)]
struct ToolSet {
    personal: Vec<Tool>,
    project: Vec<Tool>,
    /// `.tome/tools.json` exists but cannot be read.
    project_error: String,
    project_trusted: bool,
}

#[tauri::command]
fn list_tools(app: tauri::AppHandle, path: String) -> Result<ToolSet, String> {
    let settings = read_settings(&app)?;
    if path.trim().is_empty() {
        // No working copy open: only my own tools (never a relative .tome/tools.json).
        return Ok(ToolSet { personal: settings.tools, project: Vec::new(), project_error: String::new(), project_trusted: false });
    }
    let root = std::path::Path::new(&path);
    let (project, project_error) = match tools::read_project(root) {
        Ok(project) => (project, String::new()),
        Err(error) => (Vec::new(), error),
    };
    let print = tools::fingerprint(root);
    let project_trusted = !print.is_empty() && settings.trusted_tools.get(&path) == Some(&print);
    Ok(ToolSet { personal: settings.tools, project, project_error, project_trusted })
}

/// Trusts the project tools file as it is now (after the user has read it).
#[tauri::command]
fn trust_project_tools(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let mut settings = read_settings(&app)?;
    let print = tools::fingerprint(std::path::Path::new(&path));
    if print.is_empty() {
        return Err(".tome/tools.json이 없습니다".into());
    }
    settings.trusted_tools.insert(path, print);
    save_settings(app, settings)
}

/// Writes `.tome/tools.json` (to commit and share it) and trusts what I wrote.
#[tauri::command]
fn save_project_tools(app: tauri::AppHandle, path: String, tools: Vec<Tool>) -> Result<(), String> {
    tools::write_project(std::path::Path::new(&path), &tools)?;
    trust_project_tools(app, path)
}

/// Finds the tool by id where it lives (never trusting a tool sent by the page): a project
/// tool runs only while the file's content is the one I trusted (see `tools::find`).
fn find_tool(app: &tauri::AppHandle, path: &str, project: bool, id: &str) -> Result<Tool, String> {
    let settings = read_settings(app)?;
    let trusted = settings.trusted_tools.get(path).map(String::as_str);
    tools::find(&settings.tools, std::path::Path::new(path), trusted, project, id)
}

/// The command line a tool would run, for the confirmation dialog.
#[tauri::command]
fn preview_tool(app: tauri::AppHandle, path: String, project: bool, id: String, selection: Selection) -> Result<String, String> {
    let tool = find_tool(&app, &path, project, &id)?;
    Ok(tools::expand(&tool, std::path::Path::new(&path), &selection)?.display())
}

#[tauri::command]
async fn run_tool(app: tauri::AppHandle, path: String, project: bool, id: String, selection: Selection) -> Result<tools::Output, String> {
    let tool = find_tool(&app, &path, project, &id)?;
    blocking(move || {
        let invocation = tools::expand(&tool, std::path::Path::new(&path), &selection)?;
        tools::run(&invocation, tool.run)
    })
    .await
}

#[tauri::command]
fn load_settings(app: tauri::AppHandle) -> Result<Settings, String> {
    read_settings(&app)
}


#[tauri::command]
fn save_settings(app: tauri::AppHandle, settings: Settings) -> Result<(), String> {
    let path = settings_path(&app)?;
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())?;
    if let Ok(mut current) = IDENTITY.write() {
        *current = settings.identity;
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(Watch::default())
        .manage(Jobs::default())
        .manage(Previews::default())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            open_repository,
            branch_history,
            working_status,
            stage_files,
            commit,
            lock_board,
            lock_files,
            read_view,
            apply_view,
            list_repositories,
            graph,
            start_job,
            cancel_job,
            watch_repository,
            create_branch,
            switch_branch,
            merge_branch,
            resolve_conflicts,
            abort_merge,
            revision_changes,
            working_patches,
            file_history,
            file_patch,
            load_settings,
            save_settings,
            default_identity,
            auth_state,
            list_tools,
            trust_project_tools,
            save_project_tools,
            preview_tool,
            run_tool,
            list_assets,
            asset_previews,
            stack_info,
            restack_preview,
            restack_start,
            restack_pending,
            restack_resolve,
            restack_continue,
            restack_abort,
            fold_drafts
        ])
        .build(tauri::generate_context!())
        .expect("error while building TOME")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                // Stop running jobs, end the subscription, then let Lore store what it holds.
                for (_, mut job) in app.state::<Jobs>().running.lock().unwrap().drain() {
                    let _ = job.child.kill();
                }
                drop(app.state::<Watch>().0.lock().unwrap().take());
                tome_core::ops::finish("");
            }
        });
}

#[cfg(test)]
mod tests {
    #[test]
    fn data_urls_are_base64() {
        assert_eq!(super::data_url("image/png", b""), "data:image/png;base64,");
        assert_eq!(super::data_url("a", b"f"), "data:a;base64,Zg==");
        assert_eq!(super::data_url("a", b"fo"), "data:a;base64,Zm8=");
        assert_eq!(super::data_url("a", b"foo"), "data:a;base64,Zm9v");
        assert_eq!(super::data_url("a", &[0xFF, 0xD8, 0xFF, 0xE0]), "data:a;base64,/9j/4A==");
    }
}

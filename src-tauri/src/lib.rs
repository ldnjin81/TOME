//! The TOME window. Commands run Lore calls on a blocking thread and return the UI models.

use serde::Serialize;
use tome_core::model::{self, Branch, Lock, Revision, Status};
use tome_core::view::ViewChange;
use tome_core::{CallResult, Repository};

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

fn repository(path: &str, offline: bool) -> Repository {
    let mut repository = Repository::open(path);
    repository.offline = offline;
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

#[tauri::command]
async fn push(path: String, branch: String) -> Result<Done<Status>, String> {
    blocking(move || {
        let repository = repository(&path, false);
        checked(repository.push(&branch))?;
        Ok(Done { value: scanned_status(&repository)?, commands: vec!["lore push".into()] })
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
        let change = Repository::open(path).apply_view(&lines)?;
        let mut commands = vec!["edit .lore/view".to_string()];
        if !change.restored.is_empty() {
            commands.push(format!("lore reset {}", args(&change.restored)));
        }
        Ok(Done { value: change, commands })
    })
    .await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            open_repository,
            branch_history,
            working_status,
            stage_files,
            commit,
            push,
            lock_board,
            lock_files,
            read_view,
            apply_view
        ])
        .run(tauri::generate_context!())
        .expect("error while running TOME");
}

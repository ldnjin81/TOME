//! The TOME window. Commands run Lore calls on a blocking thread and return the UI models.

use serde::Serialize;
use tome_core::model::{self, Branch, Revision, Status};
use tome_core::Repository;

/// A repository opened in the window: its working copy, branches and history.
#[derive(Serialize)]
struct Overview {
    status: Status,
    branches: Vec<Branch>,
    history: Vec<Revision>,
    /// The Lore command line for what was just done (shown in the status bar).
    commands: Vec<String>,
}

fn open(path: String, offline: bool, length: u32) -> Result<Overview, String> {
    let mut repository = Repository::open(path.clone());
    repository.offline = offline;
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

#[tauri::command]
async fn open_repository(path: String, offline: bool) -> Result<Overview, String> {
    tauri::async_runtime::spawn_blocking(move || open(path, offline, 200))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn branch_history(path: String, branch: String, offline: bool) -> Result<Vec<Revision>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut repository = Repository::open(path);
        repository.offline = offline;
        let result = repository.history(&branch, 200);
        if result.ok() { Ok(model::history(&result)) } else { Err(result.error) }
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![open_repository, branch_history])
        .run(tauri::generate_context!())
        .expect("error while running TOME");
}

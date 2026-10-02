//! The TOME window. Commands run Lore calls on a blocking thread and return the UI models.

use serde::{Deserialize, Serialize};
use tauri::Manager;
use tome_core::model::{self, Branch, Lock, RemoteRepository, Revision, Status};
use tome_core::view::ViewChange;
use tome_core::tools::{self, Selection, Tool};
use tome_core::{CallResult, Repository, Resolution};

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
        let change = repository(&path, true).apply_view(&lines)?;
        let mut commands = vec!["edit .lore/view".to_string()];
        if !change.restored.is_empty() {
            commands.push(format!("lore reset {}", args(&change.restored)));
        }
        Ok(Done { value: change, commands })
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

/// Clones `url` into `path` with `view` (the `.lore/view` text) as the initial view.
#[tauri::command]
async fn clone_repository(path: String, url: String, view: String) -> Result<Done<()>, String> {
    blocking(move || {
        if std::fs::read_dir(&path).is_ok_and(|mut entries| entries.next().is_some()) {
            return Err(format!("폴더가 비어 있지 않습니다: {path}"));
        }
        checked(repository(&path, false).clone_from(&url, &view))?;
        Ok(Done { value: (), commands: vec![format!("lore clone {} {}", arg(&url), arg(&path))] })
    })
    .await
}

/// Brings the working copy to the branch's latest revision.
#[tauri::command]
async fn sync(path: String) -> Result<Done<Status>, String> {
    blocking(move || {
        let repository = repository(&path, false);
        checked(repository.sync())?;
        Ok(Done { value: scanned_status(&repository)?, commands: vec!["lore sync".into()] })
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
        .plugin(tauri_plugin_dialog::init())
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
            apply_view,
            list_repositories,
            graph,
            create_branch,
            switch_branch,
            merge_branch,
            resolve_conflicts,
            abort_merge,
            revision_changes,
            working_patches,
            clone_repository,
            sync,
            load_settings,
            save_settings,
            default_identity,
            auth_state,
            list_tools,
            trust_project_tools,
            save_project_tools,
            preview_tool,
            run_tool
        ])
        .run(tauri::generate_context!())
        .expect("error while running TOME");
}

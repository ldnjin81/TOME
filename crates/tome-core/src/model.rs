//! What the UI shows, read from Lore's events.

use serde::Serialize;
use serde_json::Value;

use crate::CallResult;

/// The working copy: where it is and what changed.
#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub branch_id: String,
    pub branch_name: String,
    pub revision: String,
    pub revision_number: u64,
    /// The remote branch's latest revision number (0 when unknown, e.g. offline).
    pub remote_number: u64,
    pub local_ahead: bool,
    pub remote_ahead: bool,
    /// The revision being merged in while a merge is in progress (conflicts to settle, then a
    /// commit finishes it); empty otherwise.
    pub merging: String,
    pub files: Vec<ChangedFile>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChangedFile {
    pub path: String,
    /// True for a directory entry (status reports directories too).
    pub directory: bool,
    pub action: String,
    pub staged: bool,
    /// The file was part of a merge conflict (still true after it is settled, until the commit).
    pub conflict: bool,
    /// The conflict is not settled yet.
    pub unresolved: bool,
    /// How a conflict was settled: "mine", "theirs", "auto" (merged on its own) or "edited";
    /// empty when there was no conflict or it is not settled.
    pub resolution: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Branch {
    pub id: String,
    pub name: String,
    /// Latest revision hash; all zeros when the branch has no revision of its own yet.
    pub latest: String,
    pub current: bool,
    pub archived: bool,
    pub creator: String,
    pub created: u64,
}

/// One revision in a branch's history.
#[derive(Debug, Clone, Serialize)]
pub struct Revision {
    pub id: String,
    pub number: u64,
    /// First parent, then the merged parent (a merge has two).
    pub parents: Vec<String>,
    pub message: String,
    pub author: String,
    /// Milliseconds since the Unix epoch.
    pub timestamp: u64,
    pub branch_id: String,
    /// Every metadata entry (review-status, build-status, ...), shown as badges.
    pub metadata: Vec<(String, Value)>,
}

/// A lock held on a path.
#[derive(Debug, Clone, Serialize)]
pub struct Lock {
    pub path: String,
    pub owner: String,
    /// Milliseconds since the Unix epoch.
    pub locked_at: u64,
}

pub fn locks(result: &CallResult) -> Vec<Lock> {
    result
        .data("lockFileQuery")
        .map(|lock| Lock {
            path: text(&lock["path"]),
            owner: text(&lock["owner"]),
            locked_at: lock["lockedAt"].as_u64().unwrap_or(0),
        })
        .collect()
}

/// The id Lore uses for "no revision".
pub const NO_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

fn text(value: &Value) -> String {
    value.as_str().map(str::to_string).unwrap_or_default()
}

fn flag(value: &Value) -> bool {
    value.as_u64().map(|n| n != 0).or_else(|| value.as_bool()).unwrap_or(false)
}

pub fn status(result: &CallResult) -> Option<Status> {
    let revision = result.data("repositoryStatusRevision").next()?;
    let files = result
        .data("repositoryStatusFile")
        .map(|file| ChangedFile {
            path: text(&file["path"]),
            directory: file["type"].as_str().is_some_and(|kind| kind.eq_ignore_ascii_case("directory")),
            // A changed file still at its path comes as "keep" (Lore has no modify action).
            action: diff_action(&file["action"]),
            staged: flag(&file["flagStaged"]),
            conflict: flag(&file["flagConflict"]),
            unresolved: flag(&file["flagConflictUnresolved"]),
            resolution: match () {
                _ if !flag(&file["flagConflict"]) || flag(&file["flagConflictUnresolved"]) => "",
                _ if flag(&file["flagConflictMine"]) => "mine",
                _ if flag(&file["flagConflictTheirs"]) => "theirs",
                _ if flag(&file["flagConflictAutomerged"]) => "auto",
                _ => "edited",
            }
            .to_string(),
        })
        .collect();
    let hash = |value: &Value| Some(text(value)).filter(|h| !h.is_empty() && h != NO_HASH).unwrap_or_default();
    Some(Status {
        branch_id: text(&revision["branch"]),
        branch_name: text(&revision["branchName"]),
        revision: text(&revision["revision"]),
        revision_number: revision["revisionNumber"].as_u64().unwrap_or(0),
        remote_number: revision["revisionRemoteNumber"].as_u64().unwrap_or(0),
        local_ahead: flag(&revision["isLocalAhead"]),
        remote_ahead: flag(&revision["isRemoteAhead"]),
        // Lore keeps revisionMerged on the commit a merge made, too; a merge is only in progress
        // while there is also a staged revision.
        merging: if hash(&revision["revisionStaged"]).is_empty() { String::new() } else { hash(&revision["revisionMerged"]) },
        files,
    })
}

/// One revision in a file's history (`Repository::file_history`).
#[derive(Debug, Clone, Serialize)]
pub struct FileRevision {
    pub revision: Revision,
    /// The file's path in that revision.
    pub path: String,
    pub size: u64,
}

/// A file that differs between two revisions.
#[derive(Debug, Clone, Serialize)]
pub struct DiffFile {
    pub path: String,
    /// Lore's action name (add, modify, delete, move, ...).
    pub action: String,
    /// Neither side is a file (a directory entry).
    pub directory: bool,
}

/// Lore has no "modify" action: a file listed in a diff with `keep` kept its path and changed
/// its content.
fn diff_action(value: &Value) -> String {
    match value.as_str() {
        Some("keep") => "modify".to_string(),
        Some(action) => action.to_string(),
        None => value.to_string(),
    }
}

pub fn diff_files(result: &CallResult) -> Vec<DiffFile> {
    result
        .data("revisionDiffFile")
        .map(|f| DiffFile {
            path: text(&f["path"]),
            action: diff_action(&f["action"]),
            directory: !flag(&f["oldIsFile"]) && !flag(&f["newIsFile"]),
        })
        .collect()
}

/// A revision's changed files from its info delta (see `Repository::changes`).
pub fn delta_files(result: &CallResult) -> Vec<DiffFile> {
    let mut files = result
        .data("revisionInfoDelta")
        .map(|f| {
            let mut action = diff_action(&f["action"]);
            if action == "modify" && !flag(&f["flagModify"]) {
                action = "keep".into();
            }
            DiffFile { path: text(&f["path"]), action, directory: false }
        })
        .collect::<Vec<_>>();
    // The delta lists folders too, with no flag saying so: a path that another entry is under.
    let folders: std::collections::HashSet<String> = files.iter().filter_map(|f| f.path.rsplit_once('/').map(|(dir, _)| dir.to_string())).flat_map(|dir| {
        let parts: Vec<&str> = dir.split('/').collect();
        (1..=parts.len()).map(move |n| parts[..n].join("/")).collect::<Vec<_>>()
    }).collect();
    for f in &mut files {
        f.directory = folders.contains(&f.path);
    }
    files
}

/// One file's diff text.
#[derive(Debug, Clone, Serialize)]
pub struct FilePatch {
    pub path: String,
    pub action: String,
    pub patch: String,
    /// Lore found binary content and gave a marker, not lines.
    pub binary: bool,
}

/// Files that are binary whatever their bytes look like (Unreal packages and common media).
const BINARY_EXTENSIONS: &[&str] = &["uasset", "umap", "ubulk", "uexp", "png", "jpg", "jpeg", "tga", "psd", "exr", "fbx", "wav", "ogg", "mp4", "dll", "exe", "pdb", "xlsx"];

pub fn is_binary_path(path: &str) -> bool {
    path.rsplit_once('.').is_some_and(|(_, ext)| BINARY_EXTENSIONS.iter().any(|b| b.eq_ignore_ascii_case(ext)))
}

pub fn patches(result: &CallResult) -> Vec<FilePatch> {
    result
        .data("fileDiff")
        .map(|f| {
            let path = text(&f["path"]);
            let patch = text(&f["patch"]);
            // Lore marks content it finds binary; it decodes the rest as text, so bytes that are
            // not text show up as NULs or U+FFFD. Either way it is not a diff worth showing.
            let binary = is_binary_path(&path) || patch.lines().any(|l| l.starts_with("Binary files")) || patch.contains(['\0', '\u{FFFD}']);
            FilePatch { action: diff_action(&f["action"]), patch: if binary { String::new() } else { patch }, binary, path }
        })
        .collect()
}

/// A repository on the server.
#[derive(Debug, Clone, Serialize)]
pub struct RemoteRepository {
    pub id: String,
    pub name: String,
}

pub fn repositories(result: &CallResult) -> Vec<RemoteRepository> {
    result
        .data("repositoryListEntry")
        .map(|r| RemoteRepository {
            id: r["id"].as_str().map(str::to_string).unwrap_or_else(|| r["id"].to_string()),
            name: text(&r["name"]),
        })
        .collect()
}

pub fn branches(result: &CallResult) -> Vec<Branch> {
    result
        .data("branchListEntry")
        .map(|b| Branch {
            id: text(&b["id"]),
            name: text(&b["name"]),
            latest: text(&b["latest"]),
            current: flag(&b["isCurrent"]),
            archived: flag(&b["archived"]),
            creator: text(&b["creator"]),
            created: b["created"].as_u64().unwrap_or(0),
        })
        .collect()
}

/// Revisions in order; each `revisionHistoryEntry` is followed by its `metadata` events.
pub fn history(result: &CallResult) -> Vec<Revision> {
    let mut revisions: Vec<Revision> = Vec::new();
    for event in &result.events {
        let data = &event["data"];
        match event["tagName"].as_str() {
            Some("revisionHistoryEntry") => revisions.push(Revision {
                id: text(&data["revision"]),
                number: data["revisionNumber"].as_u64().unwrap_or(0),
                parents: data["parent"]
                    .as_array()
                    .map(|parents| parents.iter().map(text).filter(|p| p != NO_HASH).collect())
                    .unwrap_or_default(),
                message: String::new(),
                author: String::new(),
                timestamp: 0,
                branch_id: String::new(),
                metadata: Vec::new(),
            }),
            Some("metadata") => {
                let Some(revision) = revisions.last_mut() else { continue };
                let key = text(&data["key"]);
                let value = data["value"]["data"].clone();
                match key.as_str() {
                    "message" => revision.message = text(&value),
                    "created-by" => revision.author = text(&value),
                    "timestamp" => revision.timestamp = value.as_u64().unwrap_or(0),
                    "branch" => revision.branch_id = text(&value),
                    _ => revision.metadata.push((key, value)),
                }
            }
            _ => {}
        }
    }
    revisions
}

#[cfg(test)]
mod tests {
    #[test]
    fn binary_files_by_extension_or_content() {
        assert!(is_binary_path("Content/Hero.uasset") && is_binary_path("Maps/A.UMAP"));
        assert!(!is_binary_path("Source/Game.cpp") && !is_binary_path("README"));
        let result = CallResult {
            events: vec![
                serde_json::json!({"tagName": "fileDiff", "data": {"path": "a.txt", "patch": "@@ -1 +1 @@\n-a\n+b\n", "action": "keep"}}),
                serde_json::json!({"tagName": "fileDiff", "data": {"path": "b.bin", "patch": "@@ -1 +1 @@\n-\u{FFFD}\n", "action": "keep"}}),
                serde_json::json!({"tagName": "fileDiff", "data": {"path": "c.dat", "patch": "Binary files differ\n", "action": "add"}}),
            ],
            status: 0,
            error: String::new(),
        };
        let p = patches(&result);
        assert_eq!(p.iter().map(|p| p.binary).collect::<Vec<_>>(), [false, true, true]);
        assert_eq!(p[0].action, "modify");
        assert!(p[1].patch.is_empty(), "no garbled text is passed on");
    }

    use super::*;
    use serde_json::json;

    fn result(events: Vec<Value>) -> CallResult {
        CallResult { events, status: 0, error: String::new() }
    }

    #[test]
    fn history_attaches_metadata_to_its_revision() {
        let revisions = history(&result(vec![
            json!({"tagName": "revisionHistory", "data": {"branch": "b1"}}),
            json!({"tagName": "revisionHistoryEntry", "data": {"revision": "r2", "revisionNumber": 2, "parent": ["r1", NO_HASH]}}),
            json!({"tagName": "metadata", "data": {"key": "message", "value": {"tagName": "string", "data": "두 번째"}}}),
            json!({"tagName": "metadata", "data": {"key": "timestamp", "value": {"tagName": "numeric", "data": 1700}}}),
            json!({"tagName": "metadata", "data": {"key": "review-status", "value": {"tagName": "string", "data": "approved"}}}),
            json!({"tagName": "revisionHistoryEntry", "data": {"revision": "r1", "revisionNumber": 1, "parent": [NO_HASH, NO_HASH]}}),
        ]));
        assert_eq!(revisions.len(), 2);
        assert_eq!(revisions[0].message, "두 번째");
        assert_eq!(revisions[0].timestamp, 1700);
        assert_eq!(revisions[0].parents, ["r1"]);
        assert_eq!(revisions[0].metadata, [("review-status".to_string(), json!("approved"))]);
        assert!(revisions[1].parents.is_empty(), "the zero hash is no parent");
    }

    #[test]
    fn status_reads_revision_and_files() {
        let parsed = status(&result(vec![
            json!({"tagName": "repositoryStatusRevision", "data": {"branch": "b1", "branchName": "main", "revision": "r3", "revisionNumber": 3, "revisionRemoteNumber": 2, "isLocalAhead": 1, "isRemoteAhead": 0}}),
            json!({"tagName": "repositoryStatusFile", "data": {"path": "Content/A.uasset", "action": "add", "flagStaged": 1, "flagConflict": 0}}),
        ]))
        .unwrap();
        assert_eq!((parsed.branch_name.as_str(), parsed.revision_number, parsed.remote_number), ("main", 3, 2));
        assert!(parsed.local_ahead && !parsed.remote_ahead);
        assert_eq!(parsed.files[0].path, "Content/A.uasset");
        assert!(parsed.files[0].staged);
    }
}

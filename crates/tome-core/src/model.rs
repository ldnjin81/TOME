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
    pub files: Vec<ChangedFile>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChangedFile {
    pub path: String,
    pub action: String,
    pub staged: bool,
    pub conflict: bool,
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

const NO_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

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
            action: file["action"].as_str().map(str::to_string).unwrap_or_else(|| file["action"].to_string()),
            staged: flag(&file["flagStaged"]),
            conflict: flag(&file["flagConflict"]),
        })
        .collect();
    Some(Status {
        branch_id: text(&revision["branch"]),
        branch_name: text(&revision["branchName"]),
        revision: text(&revision["revision"]),
        revision_number: revision["revisionNumber"].as_u64().unwrap_or(0),
        remote_number: revision["revisionRemoteNumber"].as_u64().unwrap_or(0),
        local_ahead: flag(&revision["isLocalAhead"]),
        remote_ahead: flag(&revision["isRemoteAhead"]),
        files,
    })
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

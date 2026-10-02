//! Applying a View change to the working files.
//!
//! Lore v0.10.0 reads `.lore/view` on clone and when a sync brings new revisions, but does not
//! re-materialize when the file changes (a sync with nothing new leaves the working files alone).
//! TOME applies the change itself:
//!
//! - a file the new view leaves out is deleted, unless it has local changes (it is kept and
//!   reported) or `.loreignore` ignores it. Lore does not count a file outside the view as
//!   deleted, so status stays clean.
//! - a file the new view brings back shows in status as an unstaged delete; it is restored with
//!   `lore_file_reset`. Only those paths are reset, because reset also overwrites local changes.
//!
//! The view rules are Lore's own (`lore_revision::filter`), so TOME and Lore agree on every path.

use std::collections::HashSet;
use std::path::Path;

use lore_revision::filter::{self, FilterInstance};
use lore_revision::util::path::RelativePath;
use serde::Serialize;

use crate::{Repository, model};

/// What applying a view did.
#[derive(Debug, Default, Clone, Serialize)]
pub struct ViewChange {
    /// Deleted from disk: outside the new view and unchanged.
    pub removed: Vec<String>,
    /// Outside the new view but left on disk because they have local changes.
    pub kept: Vec<String>,
    /// Brought back from the current revision.
    pub restored: Vec<String>,
}

impl Repository {
    /// Writes `.lore/view` and makes the working files match it.
    pub fn apply_view(&self, lines: &[String]) -> Result<ViewChange, String> {
        let before = self.changed_files("status before the view change")?;
        let changed: HashSet<&str> = before.iter().map(|f| f.path.as_str()).collect();

        self.set_view(lines).map_err(|e| format!("Writing .lore/view failed: {e}"))?;

        let root = Path::new(&self.path);
        let view = filter::load_filter(root.join(".lore").join("view")).map_err(|e| format!("{e:?}"))?;
        let ignore = filter::load_filter(root.join(".loreignore")).map_err(|e| format!("{e:?}"))?;

        let mut change = ViewChange::default();
        let mut files = Vec::new();
        walk(root, "", &ignore, &mut files);
        for path in files {
            let Ok(relative) = RelativePath::new_from_initial_path(&path) else { continue };
            if !view.excludes(&relative, false) {
                continue;
            }
            if changed.contains(path.as_str()) {
                change.kept.push(path);
            } else if std::fs::remove_file(root.join(&path)).is_ok() {
                remove_empty_parents(root, &path);
                change.removed.push(path);
            }
        }

        // Tracked files the new view includes but the disk lacks; deletes made before the
        // change are the user's own and stay.
        let deleted_before: HashSet<&str> =
            before.iter().filter(|f| f.action == "delete").map(|f| f.path.as_str()).collect();
        let restore: Vec<String> = self
            .changed_files("status after the view change")?
            .into_iter()
            .filter(|f| f.action == "delete" && !f.staged && !f.directory)
            .filter(|f| !deleted_before.contains(f.path.as_str()))
            .map(|f| f.path)
            .collect();
        if !restore.is_empty() {
            let reset = self.reset_files(&restore);
            if !reset.ok() {
                return Err(format!("Restoring files failed: {}", reset.error));
            }
            change.restored = restore;
        }
        Ok(change)
    }

    fn changed_files(&self, what: &str) -> Result<Vec<model::ChangedFile>, String> {
        let result = self.scan_status();
        if !result.ok() {
            return Err(format!("{what} failed: {}", result.error));
        }
        Ok(model::status(&result).map(|s| s.files).unwrap_or_default())
    }
}

/// Every file under `dir`, as repository-relative paths with `/`, skipping `.lore` and what
/// `.loreignore` ignores.
fn walk(root: &Path, dir: &str, ignore: &FilterInstance, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(root.join(dir)) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if dir.is_empty() && name == ".lore" {
            continue;
        }
        let path = if dir.is_empty() { name } else { format!("{dir}/{name}") };
        let Ok(kind) = entry.file_type() else { continue };
        let Ok(relative) = RelativePath::new_from_initial_path(&path) else { continue };
        if ignore.excludes(&relative, kind.is_dir()) {
            continue;
        }
        if kind.is_dir() {
            walk(root, &path, ignore, out);
        } else if kind.is_file() {
            out.push(path);
        }
    }
}

/// Removes the directories a deletion left empty, up to (not including) the root.
fn remove_empty_parents(root: &Path, path: &str) {
    let mut dir = Path::new(path).parent();
    while let Some(d) = dir.filter(|d| !d.as_os_str().is_empty()) {
        if std::fs::remove_dir(root.join(d)).is_err() {
            break;
        }
        dir = d.parent();
    }
}

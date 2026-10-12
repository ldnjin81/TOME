//! Applying a View change to the working files.
//!
//! Since Lore 0.10.1 a sync can change an instance's view (`lore sync --view <file>`): Lore
//! deletes the files the new view leaves out, fetches the ones it brings back and stores the
//! view. TOME syncs to the current revision with the new view and reports what changed on disk.
//! Files with local changes stay; they are reported as kept. (Lore 0.10.0 could not do this, so
//! TOME used to delete and restore the files itself.)
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
    /// Makes `lines` the view of this working copy and the working files match it.
    pub fn apply_view(&self, lines: &[String]) -> Result<ViewChange, String> {
        let before = self.changed_files("status before the view change")?;
        let changed: HashSet<&str> = before.iter().map(|f| f.path.as_str()).collect();
        let root = Path::new(&self.path);
        let ignore = filter::load_filter(root.join(".loreignore")).map_err(|e| format!("{e:?}"))?;
        let on_disk = |root: &Path| {
            let mut files = Vec::new();
            walk(root, "", &ignore, &mut files);
            files.into_iter().collect::<HashSet<String>>()
        };
        let disk_before = on_disk(root);

        // The new view goes to a file of its own; Lore stores it as the view once the sync is done.
        let next = std::env::temp_dir().join(format!("tome-view-{}-{}.txt", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos())));
        let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
        std::fs::write(&next, text).map_err(|e| format!("Writing the view failed: {e}"))?;
        let status = model::status(&self.status()).ok_or("status returned nothing")?;
        let result = self.sync_view(&status.revision, &next.to_string_lossy());
        let view = filter::load_filter(&next).map_err(|e| format!("{e:?}"));
        std::fs::remove_file(&next).ok();
        if !result.ok() {
            return Err(format!("Changing the view failed: {}", result.error));
        }
        let view = view?;
        // Lore stores the view lower-cased (its paths compare without case); keep the lines as
        // they were written, which Lore reads the same way.
        self.set_view(lines).map_err(|e| format!("Writing .lore/view failed: {e}"))?;

        let disk_after = on_disk(root);
        let mut change = ViewChange::default();
        for path in &disk_before {
            if !disk_after.contains(path) {
                change.removed.push(path.clone());
            } else if changed.contains(path.as_str())
                && RelativePath::new_from_initial_path(path).is_ok_and(|relative| view.excludes(&relative, false))
            {
                change.kept.push(path.clone());
            }
        }
        change.restored = disk_after.difference(&disk_before).cloned().collect();
        for list in [&mut change.removed, &mut change.kept, &mut change.restored] {
            list.sort();
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

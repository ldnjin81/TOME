//! Applying a View change to the working files.
//!
//! Since Lore 0.10.1 a sync can change an instance's view (`lore sync --view <file>`): Lore
//! deletes the files the new view leaves out, fetches the ones it brings back and stores the
//! view. TOME syncs to the current revision with the new view and reports what the sync did,
//! from Lore's own file events (since Lore 0.11 a sync reports deleted files too).
//! Files with local changes stay; they are reported as kept. (Lore 0.10.0 could not do this, so
//! TOME used to delete and restore the files itself.)
//!
//! The view rules are Lore's own (`lore_revision::filter`), so TOME and Lore agree on every path.

use std::path::Path;

use lore_revision::filter;
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
        let root = Path::new(&self.path);

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

        let mut change = ViewChange::default();
        for file in result.data("revisionSyncFile").filter(|f| f["flagFile"].as_u64() == Some(1)) {
            let path = file["path"].as_str().unwrap_or("").to_string();
            match file["action"].as_str() {
                Some("delete") => change.removed.push(path),
                Some("add") => change.restored.push(path),
                _ => {}
            }
        }
        // Changed files the new view leaves out stay on disk with their changes.
        change.kept = before
            .iter()
            .filter(|f| root.join(&f.path).is_file())
            .filter(|f| RelativePath::new_from_initial_path(&f.path).is_ok_and(|relative| view.excludes(&relative, false)))
            .map(|f| f.path.clone())
            .collect();
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

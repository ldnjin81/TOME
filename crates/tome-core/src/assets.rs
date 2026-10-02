//! The artist view of a working copy: folders and Unreal packages (`.uasset`, `.umap`) on disk,
//! one folder at a time.

use std::path::{Component, Path, PathBuf};

use serde::Serialize;

/// Folders that never hold content to browse: Lore's own and Unreal's generated ones.
const SKIPPED: [&str; 4] = ["Binaries", "Intermediate", "Saved", "DerivedDataCache"];

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Folder {
    pub name: String,
    /// Relative to the working copy, `/`-separated.
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Asset {
    /// The file name without its extension (what the editor shows).
    pub name: String,
    /// Relative to the working copy, `/`-separated (the path Lore uses).
    pub path: String,
    /// `uasset` or `umap`.
    pub kind: String,
    pub size: u64,
    /// Last modified, milliseconds since the Unix epoch.
    pub modified: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Listing {
    pub folder: String,
    pub folders: Vec<Folder>,
    pub assets: Vec<Asset>,
}

/// `folder` (relative, `/`-separated, empty for the root) under `root`; None when it would leave
/// the working copy.
pub fn resolve(root: &Path, folder: &str) -> Option<PathBuf> {
    let relative = Path::new(folder);
    if !relative.components().all(|c| matches!(c, Component::Normal(_))) {
        return None;
    }
    Some(root.join(relative))
}

fn join(folder: &str, name: &str) -> String {
    if folder.is_empty() { name.to_string() } else { format!("{folder}/{name}") }
}

/// The subfolders and packages directly in `folder`, each sorted by name (case-insensitive).
/// Hidden folders and Unreal's generated ones are left out.
pub fn list(root: &Path, folder: &str) -> Result<Listing, String> {
    let folder = folder.trim_matches('/');
    let dir = resolve(root, folder).ok_or_else(|| format!("not a folder in the working copy: {folder}"))?;
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let (mut folders, mut assets) = (Vec::new(), Vec::new());
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            if !name.starts_with('.') && !SKIPPED.contains(&name.as_str()) {
                folders.push(Folder { path: join(folder, &name), name });
            }
            continue;
        }
        let Some((stem, kind)) = name.rsplit_once('.') else { continue };
        let kind = kind.to_ascii_lowercase();
        if kind != "uasset" && kind != "umap" {
            continue;
        }
        let modified = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as u64);
        assets.push(Asset { name: stem.to_string(), path: join(folder, &name), kind, size: meta.len(), modified });
    }
    folders.sort_by_key(|f| f.name.to_lowercase());
    assets.sort_by_key(|a| a.name.to_lowercase());
    Ok(Listing { folder: folder.to_string(), folders, assets })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_packages_and_folders_but_not_generated_ones() {
        let root = std::env::temp_dir().join(format!("tome-assets-{}", std::process::id()));
        for dir in ["Content/Characters", "Content/maps", ".lore", "Intermediate", "Saved", "Content/.hidden"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        for file in ["Content/b_Mesh.uasset", "Content/A_Map.UMAP", "Content/readme.txt", "Content/noext", "Project.uproject"] {
            std::fs::write(root.join(file), b"x").unwrap();
        }
        let top = list(&root, "").unwrap();
        assert_eq!(top.folders.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["Content"]);
        assert!(top.assets.is_empty());
        let content = list(&root, "/Content/").unwrap();
        assert_eq!(content.folder, "Content");
        assert_eq!(content.folders.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["Content/Characters", "Content/maps"]);
        assert_eq!(content.assets.iter().map(|a| (a.name.as_str(), a.path.as_str(), a.kind.as_str())).collect::<Vec<_>>(), [("A_Map", "Content/A_Map.UMAP", "umap"), ("b_Mesh", "Content/b_Mesh.uasset", "uasset")]);
        assert!(content.assets[0].modified > 0 && content.assets[0].size == 1);
        assert!(list(&root, "../etc").is_err());
        assert!(list(&root, "Content/../..").is_err());
        assert!(list(&root, "Missing").is_err());
        std::fs::remove_dir_all(&root).ok();
    }
}

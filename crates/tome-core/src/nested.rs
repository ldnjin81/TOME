//! 작업본 안에 든 다른 Lore 저장소(중첩 저장소). 하위 폴더에 `.lore` 메타 폴더가 있으면 그 폴더가
//! 다른 저장소의 루트다. 작업본 전체를 뒤지지 않고, 주어진 경로와 그 상위 폴더만 확인한다.
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

/// `paths`(작업본 루트 기준 상대 경로) 각각이 놓인 중첩 저장소의 루트를 루트 기준 상대 경로('/'
/// 구분)로 돌려준다. 경로 자신이 루트인 경우도 포함하고, 작업본 루트 자체는 제외한다. 겹치면
/// (중첩 안의 중첩) 가장 바깥 루트만 남긴다. 정렬되어 있고 중복이 없다.
pub fn nested_roots(root: &Path, paths: &[String]) -> Vec<String> {
    let mut checked: HashMap<String, bool> = HashMap::new();
    let mut found = BTreeSet::new();
    for path in paths {
        let parts: Vec<&str> = path.split(['/', '\\']).filter(|p| !p.is_empty() && *p != ".").collect();
        // `..`가 있으면 작업본 밖을 가리킬 수 있으니 보지 않는다.
        if parts.contains(&"..") {
            continue;
        }
        let mut prefix = String::new();
        for part in parts {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            // 작업본 자신의 메타 폴더 안은 중첩 저장소가 아니다.
            if part == ".lore" {
                break;
            }
            let is_repository = *checked.entry(prefix.clone()).or_insert_with(|| root.join(&prefix).join(".lore").is_dir());
            if is_repository {
                found.insert(prefix.clone());
                break;
            }
        }
    }
    found.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_nested_roots_from_paths() {
        let root = std::env::temp_dir().join(format!("tome-nested-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".lore")).unwrap();
        std::fs::create_dir_all(root.join("Plugins/Sub/.lore")).unwrap();
        std::fs::create_dir_all(root.join("Plugins/Sub/Inner/.lore")).unwrap();
        std::fs::create_dir_all(root.join("Content/Maps")).unwrap();
        // `.lore`라는 이름의 파일은 저장소가 아니다.
        std::fs::create_dir_all(root.join("Fake")).unwrap();
        std::fs::write(root.join("Fake/.lore"), "").unwrap();
        let paths = [
            "Plugins/Sub/Source/a.cpp".to_string(),
            "Plugins\\Sub\\Inner\\b.txt".to_string(),
            "Plugins/Sub".to_string(),
            "Content/Maps/m.umap".to_string(),
            "Fake/x".to_string(),
            ".lore/view".to_string(),
            "../outside/.lore".to_string(),
            "top.txt".to_string(),
        ];
        assert_eq!(nested_roots(&root, &paths), vec!["Plugins/Sub".to_string()]);
        assert!(nested_roots(&root, &[]).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}

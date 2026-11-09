//! 브랜치 탐색기의 트리와 가로 시간축. 시간은 호출자가 전달한 로컬 시간대의 날짜로 묶는다.
use crate::{
    CallResult, Repository,
    model::{Branch, Revision},
};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};

const DAY: u64 = 86_400_000;

#[derive(Debug, Clone, Serialize)]
pub struct Lane {
    pub branch: Branch,
    pub parent: Option<String>,
    pub depth: usize,
    pub parent_source: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct Column {
    pub start: u64,
    pub end: u64,
    pub x: f64,
    pub width: f64,
    pub gap: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct Node {
    pub revision: Revision,
    pub lane: usize,
    pub x: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub kind: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct Layout {
    pub lanes: Vec<Lane>,
    pub columns: Vec<Column>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub width: f64,
    pub root: String,
    pub incomplete: Vec<String>,
    /// 이 시각(Unix ms) 이후의 기록만 읽었다. 0이면 전체.
    pub since: u64,
}

/// 경계 밖 부모를 한 번에 너무 많이 읽지 않도록 하는 상한.
const MAX_BOUNDARY_PARENTS: usize = 300;

/// 읽은 리비전의 부모 중 아직 없는 것. 기준 날짜 경계에서 끊긴 첫 부모와, 다른(보관·기준 밖)
/// 브랜치에서 온 병합 부모가 여기에 든다. 처음 나온 순서대로, 중복 없이.
pub fn missing_parents(revisions: &[Revision]) -> Vec<String> {
    let present: HashSet<&str> = revisions.iter().map(|r| r.id.as_str()).collect();
    let mut seen = HashSet::new();
    revisions
        .iter()
        .flat_map(|r| r.parents.iter())
        .filter(|p| !p.is_empty() && !present.contains(p.as_str()) && seen.insert(p.as_str()))
        .cloned()
        .collect()
}

/// `days`일 전의 시각(Unix ms). `None`이면 0(전체).
pub fn since_days(now: u64, days: Option<u32>) -> u64 {
    days.map_or(0, |d| now.saturating_sub(u64::from(d) * DAY).max(1))
}

/// 이벤트의 생성 부모를 읽는다. Lore는 stack[0]에 직접 부모를 저장한다.
pub fn parent_hints(result: &CallResult, archived: bool) -> HashMap<String, String> {
    let available: HashSet<_> = crate::model::branches(result)
        .into_iter()
        .filter(|b| archived || !b.archived)
        .map(|b| b.id)
        .collect();
    result
        .data("branchListEntry")
        .filter_map(|b| {
            let id = b["id"].as_str()?;
            let parent = b["stack"]
                .as_array()?
                .iter()
                .filter_map(|p| p["branch"].as_str())
                .find(|p| *p != id && available.contains(*p))?;
            Some((id.into(), parent.into()))
        })
        .collect()
}

/// 중복 브랜치/리비전을 제거하고, 순환 부모도 루트 아래로 복구한다.
pub fn layout(
    branches: &[Branch],
    revisions: &[Revision],
    hints: &HashMap<String, String>,
    archived: bool,
) -> Layout {
    layout_with_root(branches, revisions, hints, archived, None)
}

/// 로컬 저장소 메타데이터에 기본 브랜치가 있으면 이름 추론보다 우선한다.
pub fn layout_with_root(
    branches: &[Branch],
    revisions: &[Revision],
    hints: &HashMap<String, String>,
    archived: bool,
    default_branch: Option<&str>,
) -> Layout {
    layout_with_timezone(branches, revisions, hints, archived, default_branch, 0)
}

/// UTC 동쪽을 양수로 하는 분 단위 오프셋으로 날짜를 나눈다.
pub fn layout_with_timezone(
    branches: &[Branch],
    revisions: &[Revision],
    hints: &HashMap<String, String>,
    archived: bool,
    default_branch: Option<&str>,
    timezone_offset_minutes: i32,
) -> Layout {
    let offset = i64::from(timezone_offset_minutes.clamp(-1440, 1440)) * 60_000;
    let midnight = |day: i64| (day * DAY as i64 - offset).max(0) as u64;
    // 보관 브랜치를 숨겨도, 보이는 브랜치의 리비전이 부모로 삼는(병합·포크) 보관 브랜치는 레인으로
    // 남겨 선이 끊기지 않게 한다.
    let live: HashSet<&str> = branches.iter().filter(|b| !b.archived).map(|b| b.id.as_str()).collect();
    let rev_branch: HashMap<&str, &str> = revisions.iter().map(|r| (r.id.as_str(), r.branch_id.as_str())).collect();
    let parent_branches: HashSet<&str> = revisions
        .iter()
        .filter(|r| live.contains(r.branch_id.as_str()))
        .flat_map(|r| r.parents.iter())
        .filter_map(|p| rev_branch.get(p.as_str()).copied())
        .collect();
    let mut seen = HashSet::new();
    let mut branches: Vec<_> = branches
        .iter()
        .filter(|b| (archived || !b.archived || parent_branches.contains(b.id.as_str())) && seen.insert(&b.id))
        .cloned()
        .collect();
    branches.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    let root = branches
        .iter()
        .find(|b| Some(b.id.as_str()) == default_branch)
        .or_else(|| branches.iter().find(|b| b.name == "main"))
        .or_else(|| {
            branches
                .iter()
                .filter(|b| !hints.contains_key(&b.id))
                .min_by_key(|b| b.created)
        })
        .or(branches.first())
        .map(|b| b.id.clone())
        .unwrap_or_default();
    let ids: HashSet<_> = branches.iter().map(|b| b.id.clone()).collect();
    let by_rev: HashMap<_, _> = revisions.iter().map(|r| (r.id.as_str(), r)).collect();
    let mut parents = HashMap::new();
    let mut sources = HashMap::new();
    for b in &branches {
        if b.id == root {
            continue;
        }
        let oldest = revisions
            .iter()
            .filter(|r| r.branch_id == b.id)
            .min_by_key(|r| (r.timestamp, r.number, &r.id));
        let inferred = oldest
            .and_then(|r| r.parents.first())
            .and_then(|p| by_rev.get(p.as_str()))
            .or_else(|| {
                if oldest.is_none() {
                    by_rev.get(b.latest.as_str())
                } else {
                    None
                }
            })
            .map(|r| r.branch_id.clone());
        let hint = hints.get(&b.id).filter(|p| **p != b.id && ids.contains(*p));
        let inferred = inferred.filter(|p| *p != b.id && ids.contains(p));
        let source = if hint.is_some() {
            "stack"
        } else if inferred.is_some() {
            "revision"
        } else {
            "root"
        };
        let parent = hint.cloned().or(inferred).unwrap_or_else(|| root.clone());
        sources.insert(b.id.clone(), source.to_string());
        parents.insert(b.id.clone(), parent);
    }
    for b in &branches {
        let mut trail = HashSet::new();
        let mut at = b.id.clone();
        while let Some(p) = parents.get(&at) {
            if !trail.insert(at.clone()) {
                parents.insert(b.id.clone(), root.clone());
                sources.insert(b.id.clone(), "root".into());
                break;
            }
            at = p.clone();
        }
    }
    fn visit(
        id: &str,
        depth: usize,
        branches: &[Branch],
        parents: &HashMap<String, String>,
        sources: &HashMap<String, String>,
        lanes: &mut Vec<Lane>,
    ) {
        if let Some(b) = branches.iter().find(|b| b.id == id) {
            lanes.push(Lane {
                branch: b.clone(),
                parent: parents.get(id).cloned(),
                depth,
                parent_source: sources.get(id).cloned().unwrap_or_else(|| "root".into()),
            });
            for child in branches
                .iter()
                .filter(|b| parents.get(&b.id).is_some_and(|p| p == id))
            {
                visit(&child.id, depth + 1, branches, parents, sources, lanes);
            }
        }
    }
    let mut lanes = Vec::new();
    visit(&root, 0, &branches, &parents, &sources, &mut lanes);
    let lane_ids: HashMap<_, _> = lanes
        .iter()
        .enumerate()
        .map(|(i, l)| (l.branch.id.as_str(), i))
        .collect();
    let mut seen = HashSet::new();
    let mut days: BTreeMap<i64, Vec<&Revision>> = BTreeMap::new();
    for r in revisions
        .iter()
        .filter(|r| lane_ids.contains_key(r.branch_id.as_str()) && seen.insert(&r.id))
    {
        days.entry((r.timestamp as i64 + offset).div_euclid(DAY as i64))
            .or_default()
            .push(r);
    }
    let mut columns = Vec::new();
    let mut nodes = Vec::new();
    let mut x = 0.;
    let mut previous = None;
    for (day, mut rs) in days {
        if let Some(p) = previous {
            if day > p + 1 {
                columns.push(Column {
                    start: midnight(p + 1),
                    end: midnight(day - 1),
                    x,
                    width: 150.,
                    gap: true,
                });
                x += 150.;
            }
        }
        rs.sort_by_key(|r| (r.timestamp, r.number, &r.id));
        let width = (rs.len() as f64 * 36. + 48.).max(160.);
        columns.push(Column {
            start: midnight(day),
            end: midnight(day),
            x,
            width,
            gap: false,
        });
        for (i, r) in rs.into_iter().enumerate() {
            nodes.push(Node {
                revision: r.clone(),
                lane: lane_ids[r.branch_id.as_str()],
                x: x + 30. + i as f64 * 36.,
            });
        }
        x += width;
        previous = Some(day);
    }
    let node_ids: HashMap<_, _> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.revision.id.as_str(), i))
        .collect();
    let mut edges = Vec::new();
    for (to, n) in nodes.iter().enumerate() {
        for (i, p) in n.revision.parents.iter().enumerate() {
            if let Some(&from) = node_ids.get(p.as_str()) {
                edges.push(Edge {
                    from,
                    to,
                    kind: if i > 0 {
                        "merge"
                    } else if nodes[from].lane != n.lane {
                        "fork"
                    } else {
                        "line"
                    }
                    .into(),
                });
            }
        }
    }
    Layout {
        lanes,
        columns,
        nodes,
        edges,
        width: x.max(320.),
        root,
        incomplete: Vec::new(),
        since: 0,
    }
}

impl Repository {
    /// 표시할 브랜치를 모두 읽는다. 오프라인일 때 서버로 자동 재시도하지 않는다.
    pub fn branch_explorer(&self, archived: bool) -> Result<Layout, String> {
        self.branch_explorer_in_timezone(archived, 0)
    }

    pub fn branch_explorer_in_timezone(
        &self,
        archived: bool,
        timezone_offset_minutes: i32,
    ) -> Result<Layout, String> {
        self.branch_explorer_since(archived, timezone_offset_minutes, 0)
    }

    /// `since`(Unix ms, 0이면 전체) 이후의 리비전만 읽는다. Lore가 브랜치마다 그 날짜에서
    /// 기록 읽기를 멈추므로 큰 저장소에서도 최근 기록만큼만 시간이 든다. 경계에서 끊긴 부모와
    /// 보관·기준 밖 브랜치의 병합 부모는 그 리비전 하나씩만 더 읽어 선을 잇는다.
    pub fn branch_explorer_since(
        &self,
        archived: bool,
        timezone_offset_minutes: i32,
        since: u64,
    ) -> Result<Layout, String> {
        // 보관 브랜치를 숨길 때도 병합 부모로 쓰인 보관 브랜치의 레인을 만들려면 그 정보가 필요하다.
        let args = lore::branch::LoreBranchListArgs { archived: 1 };
        let result = crate::call(
            crate::interface::lore_branch_list_async,
            &self.globals(),
            &args,
        );
        if !result.ok() {
            return Err(result.error);
        }
        let branches = crate::model::branches(&result);
        let mut revisions = Vec::new();
        let mut incomplete = Vec::new();
        let mut seen = HashSet::new();
        for b in branches
            .iter()
            .filter(|b| (archived || !b.archived) && seen.insert(&b.id))
        {
            // 보관 브랜치는 이름→ID가 지워져 이름으로는 기록을 못 읽는다(Lore 0.13). id로는 읽힌다.
            let key = if b.archived { &b.id } else { &b.name };
            let history = if since == 0 {
                self.history(key, u32::MAX)
            } else {
                self.history_since(key, since)
            };
            if !history.ok() {
                incomplete.push(format!("{}: {}", b.name, history.error));
            }
            revisions.extend(crate::model::history(&history));
        }
        // 경계 밖 부모는 리비전 하나만 읽는다(그 부모의 부모까지 따라가지 않는다). 보관 브랜치까지
        // 전체 기록을 읽었다면 남은 부모는 지워진 브랜치의 것이라 레인이 없으니 읽지 않는다.
        let boundary = if since == 0 && archived { Vec::new() } else { missing_parents(&revisions) };
        for parent in boundary.into_iter().take(MAX_BOUNDARY_PARENTS) {
            let found = self.history_from(&parent, 1);
            revisions.extend(crate::model::history(&found).into_iter().filter(|r| r.id == parent));
        }
        // local+offline을 모두 고정해 기본 브랜치 조회가 서버에 접속하지 않게 한다.
        let mut globals = self.globals();
        globals.local = 1;
        globals.offline = 1;
        let metadata_args = lore::repository::LoreRepositoryMetadataGetArgs {
            key: crate::interface::LoreString::from_bytes(b"default-branch"),
        };
        let metadata = crate::call(
            crate::interface::lore_repository_metadata_get_async,
            &globals,
            &metadata_args,
        );
        let default_branch = metadata
            .data("metadata")
            .find(|m| m["key"] == "default-branch")
            .and_then(|m| m["value"]["data"].as_str());
        let mut out = layout_with_timezone(
            &branches,
            &revisions,
            &parent_hints(&result, archived),
            archived,
            default_branch,
            timezone_offset_minutes,
        );
        out.incomplete = incomplete;
        out.since = since;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn b(id: &str, latest: &str, archived: bool) -> Branch {
        Branch {
            id: id.into(),
            name: id.into(),
            latest: latest.into(),
            current: false,
            archived,
            creator: String::new(),
            created: 0,
        }
    }
    fn r(id: &str, branch: &str, parents: &[&str], day: u64) -> Revision {
        Revision {
            id: id.into(),
            branch_id: branch.into(),
            parents: parents.iter().map(|s| s.to_string()).collect(),
            timestamp: day * DAY,
            number: day,
            message: String::new(),
            author: String::new(),
            metadata: vec![],
        }
    }
    #[test]
    fn parent_inference_and_empty_branch() {
        let l = layout(
            &[
                b("main", "m", false),
                b("feature", "f", false),
                b("empty", "f", false),
            ],
            &[r("m", "main", &[], 1), r("f", "feature", &["m"], 2)],
            &HashMap::new(),
            false,
        );
        assert_eq!(l.lanes[0].branch.id, "main");
        assert_eq!(
            l.lanes
                .iter()
                .find(|l| l.branch.id == "feature")
                .unwrap()
                .parent
                .as_deref(),
            Some("main")
        );
        assert_eq!(
            l.lanes
                .iter()
                .find(|l| l.branch.id == "empty")
                .unwrap()
                .parent
                .as_deref(),
            Some("feature")
        );
    }
    #[test]
    fn stack_overrides_inference_and_cycles_are_safe() {
        let bs = [
            b("main", "m", false),
            b("a", "a", false),
            b("b", "b", false),
        ];
        let rs = [
            r("m", "main", &[], 1),
            r("a", "a", &["m"], 2),
            r("b", "b", &["m"], 3),
        ];
        let l = layout(&bs, &rs, &HashMap::from([("a".into(), "b".into())]), false);
        assert_eq!(
            l.lanes
                .iter()
                .find(|l| l.branch.id == "a")
                .unwrap()
                .parent
                .as_deref(),
            Some("b")
        );
        let l = layout(
            &bs,
            &rs,
            &HashMap::from([("a".into(), "b".into()), ("b".into(), "a".into())]),
            false,
        );
        assert_eq!(l.lanes.len(), 3);
    }
    #[test]
    fn folds_empty_days_spaces_dense_nodes_and_connects_merges() {
        let l = layout(
            &[b("main", "merge", false), b("f", "f", false)],
            &[
                r("m", "main", &[], 1),
                r("f", "f", &["m"], 5),
                r("merge", "main", &["m", "f"], 5),
            ],
            &HashMap::new(),
            false,
        );
        assert_eq!(l.columns.len(), 3);
        assert!(l.columns[1].gap);
        assert_eq!((l.columns[1].start, l.columns[1].end), (2 * DAY, 4 * DAY));
        assert!(l.nodes[2].x - l.nodes[1].x >= 36.);
        assert_eq!(l.edges.iter().filter(|e| e.kind == "merge").count(), 1);
        assert_eq!(l.edges.iter().filter(|e| e.kind == "fork").count(), 1);
    }
    #[test]
    fn archived_filter_and_duplicates() {
        let bs = [
            b("main", "m", false),
            b("a", "a", true),
            b("main", "m", false),
        ];
        let rs = [
            r("m", "main", &[], 0),
            r("a", "a", &["m"], 1),
            r("m", "main", &[], 0),
        ];
        assert_eq!(layout(&bs, &rs, &HashMap::new(), false).nodes.len(), 1);
        assert_eq!(layout(&bs, &rs, &HashMap::new(), true).lanes.len(), 2);
        assert!(layout(&[], &[], &HashMap::new(), false).lanes.is_empty());
    }
    #[test]
    fn local_default_precedes_main_name() {
        let l = layout_with_root(
            &[b("main", "m", false), b("trunk", "t", false)],
            &[],
            &HashMap::new(),
            false,
            Some("trunk"),
        );
        assert_eq!(l.root, "trunk");
        assert_eq!(l.lanes[0].branch.id, "trunk");
    }
    #[test]
    fn local_midnight_splits_days_and_folds_local_gaps() {
        let bs = [b("main", "late", false)];
        let mut early = r("early", "main", &[], 10);
        early.timestamp += 14 * 3_600_000; // KST 23시
        let mut late = early.clone();
        late.id = "late".into();
        late.timestamp += 2 * 3_600_000; // 다음 날 KST 1시
        let utc = layout_with_timezone(
            &bs,
            &[early.clone(), late.clone()],
            &HashMap::new(),
            false,
            None,
            0,
        );
        assert_eq!(utc.columns.len(), 1);
        let kst = layout_with_timezone(
            &bs,
            &[early.clone(), late.clone()],
            &HashMap::new(),
            false,
            None,
            540,
        );
        assert_eq!(kst.columns.len(), 2);
        assert_eq!(kst.columns[1].start, 11 * DAY - 9 * 3_600_000);
        late.timestamp += 3 * DAY;
        let kst = layout_with_timezone(
            &bs,
            &[early.clone(), late.clone()],
            &HashMap::new(),
            false,
            None,
            540,
        );
        assert!(kst.columns[1].gap);
        assert_eq!(kst.columns[1].start, 11 * DAY - 9 * 3_600_000);
        assert_eq!(kst.columns[1].end, 13 * DAY - 9 * 3_600_000);
        let west = layout_with_timezone(
            &bs,
            &[r("midnight", "main", &[], 10)],
            &HashMap::new(),
            false,
            None,
            -480,
        );
        assert_eq!(west.columns[0].start, 9 * DAY + 8 * 3_600_000);
    }
    #[test]
    fn archived_merge_parent_keeps_its_lane() {
        // old(보관)의 리비전이 main에 병합됐다. 보관 브랜치를 숨겨도 병합 부모의 레인과 선은 남는다.
        let bs = [
            b("main", "merge", false),
            b("old", "o", true),
            b("gone", "g", true),
        ];
        let rs = [
            r("m", "main", &[], 1),
            r("o", "old", &["m"], 2),
            r("g", "gone", &["m"], 2),
            r("merge", "main", &["m", "o"], 3),
        ];
        let l = layout(&bs, &rs, &HashMap::new(), false);
        assert!(l.lanes.iter().any(|l| l.branch.id == "old"));
        // 아무도 부모로 삼지 않은 보관 브랜치는 계속 숨는다.
        assert!(!l.lanes.iter().any(|l| l.branch.id == "gone"));
        assert_eq!(l.edges.iter().filter(|e| e.kind == "merge").count(), 1);
        assert_eq!(l.since, 0);
    }
    #[test]
    fn boundary_parents_connect_lines() {
        // 기준 날짜 안의 리비전만 읽었을 때: f1의 첫 부모(m0, 경계 밖)와 병합 부모(x, 다른 브랜치)가 빠져 있다.
        let loaded = [
            r("m1", "main", &["m0"], 100),
            r("f1", "f", &["m0"], 101),
            r("m2", "main", &["m1", "x"], 102),
        ];
        assert_eq!(missing_parents(&loaded), vec!["m0".to_string(), "x".to_string()]);
        assert!(missing_parents(&[r("a", "main", &[], 1)]).is_empty());
        // 보충한 경계 부모가 레이아웃에서 포크·직선으로 이어진다.
        let mut all = loaded.to_vec();
        all.push(r("m0", "main", &[], 10));
        let l = layout(
            &[b("main", "m2", false), b("f", "f1", false)],
            &all,
            &HashMap::new(),
            false,
        );
        assert_eq!(l.edges.iter().filter(|e| e.kind == "fork").count(), 1);
        assert_eq!(l.edges.iter().filter(|e| e.kind == "line").count(), 2);
        // 경계 부모와 최근 기록 사이의 빈 날짜는 접힌다.
        assert!(l.columns.iter().any(|c| c.gap));
    }
    #[test]
    fn since_days_counts_back_from_now() {
        assert_eq!(since_days(100 * DAY, None), 0);
        assert_eq!(since_days(100 * DAY, Some(90)), 10 * DAY);
        // 아주 이른 시각에도 0(전체)으로 바뀌지 않는다.
        assert_eq!(since_days(DAY, Some(90)), 1);
    }
    #[test]
    fn reads_stack_event() {
        let result = CallResult {
            status: 0,
            error: String::new(),
            events: vec![
                serde_json::json!({"tagName":"branchListEntry","data":{"id":"feature","stack":[{"branch":"main","revision":"m"}]}}),
            ],
        };
        let mut result = result;
        result.events.push(
            serde_json::json!({"tagName":"branchListEntry","data":{"id":"main","name":"main"}}),
        );
        assert_eq!(parent_hints(&result, false)["feature"], "main");
        result.events[0]["data"]["stack"] =
            serde_json::json!([{"branch":"deleted"},{"branch":"main"}]);
        assert_eq!(parent_hints(&result, false)["feature"], "main");
    }
}

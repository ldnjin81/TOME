import type { Branch, Revision } from './types';

export interface ExplorerLane { branch: Branch; parent: string | null; depth: number; parent_source: string }
export interface ExplorerNode { revision: Revision; lane: number; x: number }
export interface ExplorerLayout {
  lanes: ExplorerLane[];
  columns: { start: number; end: number; x: number; width: number; gap: boolean }[];
  nodes: ExplorerNode[];
  edges: { from: number; to: number; kind: 'line' | 'fork' | 'merge' }[];
  width: number;
  root: string;
  incomplete: string[];
  /** 이 시각(Unix ms) 이후의 기록만 읽었다. 0이면 전체. */
  since: number;
}

/** 처음 읽는 기록 범위이자 "이전 기록 더 보기" 한 번에 늘리는 날 수. */
export const HISTORY_STEP_DAYS = 90;

/** 기록 범위를 한 단계 넓힌다(전체이면 그대로). */
export const moreHistory = (days: number | null) => days === null ? null : days + HISTORY_STEP_DAYS;

/** "동작 줄이기"를 켠 사용자에게는 즉시, 아니면 부드럽게 스크롤한다. */
export const scrollBehavior = (reducedMotion: boolean): ScrollBehavior => reducedMotion ? 'auto' : 'smooth';

export function prefersReducedMotion() {
  return typeof window !== 'undefined' && typeof window.matchMedia === 'function' && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
}

/** 검색 중에는 일치하는 브랜치의 조상도 보여 주고 접힘을 임시로 무시한다. */
export function visibleLanes(lanes: ExplorerLane[], collapsed: Set<string>, query: string) {
  const search = query.trim().toLocaleLowerCase();
  const byId = new Map(lanes.map((l) => [l.branch.id, l]));
  const matches = new Set<string>();
  if (search) for (const lane of lanes.filter((l) => l.branch.name.toLocaleLowerCase().includes(search))) {
    let at: ExplorerLane | undefined = lane;
    while (at && !matches.has(at.branch.id)) { matches.add(at.branch.id); at = byId.get(at.parent ?? ''); }
  }
  return lanes.map((_, i) => i).filter((i) => {
    if (search) return matches.has(lanes[i].branch.id);
    let parent = lanes[i].parent;
    const seen = new Set<string>();
    while (parent && !seen.has(parent)) {
      if (collapsed.has(parent)) return false;
      seen.add(parent); parent = byId.get(parent)?.parent ?? null;
    }
    return true;
  });
}

/** 가로는 같은 레인, 세로는 다음 비어 있지 않은 레인의 가장 가까운 시간으로 이동한다. */
export function adjacentNode(nodes: ExplorerNode[], visible: number[], id: string, key: string): ExplorerNode | undefined {
  const current = nodes.find((n) => n.revision.id === id);
  if (!current) return nodes.find((n) => visible.includes(n.lane));
  if (key === 'ArrowLeft' || key === 'ArrowRight') {
    return nodes.filter((n) => n.lane === current.lane && (key === 'ArrowLeft' ? n.x < current.x : n.x > current.x)).sort((a,b) => Math.abs(a.x-current.x)-Math.abs(b.x-current.x))[0];
  }
  const direction = key === 'ArrowUp' ? -1 : 1;
  for (let i=visible.indexOf(current.lane)+direction; i>=0 && i<visible.length; i+=direction) {
    const closest = nodes.filter((n) => n.lane===visible[i]).sort((a,b) => Math.abs(a.x-current.x)-Math.abs(b.x-current.x))[0];
    if (closest) return closest;
  }
}

export const clampZoom = (zoom: number) => Math.max(0.25, Math.min(4, zoom));

/** 최신 리비전의 시간과 레인을 동시에 보여 주는 초기 위치. */
export function recentPosition(nodes: ExplorerNode[], width: number, height: number, zoom = 1) {
  const latest = nodes.reduce<ExplorerNode | undefined>((best, node) => !best || node.revision.timestamp >= best.revision.timestamp ? node : best, undefined);
  return { left: Math.max(0, (latest?.x ?? 0)*zoom-width/2), top: Math.max(0, 40+(latest?.lane ?? 0)*58+29-height/2) };
}

/** 탐색기에서 브랜치를 더블클릭했을 때 할 일. 보관 브랜치는 복원한 뒤 전환한다(병합 중에는 둘 다 안 한다). */
export function switchAction(branch: { name: string; archived: boolean }, current: string, merging: boolean, busy: boolean): 'none' | 'restore' | 'merging' | 'switch' {
  if (busy || branch.name === current) return 'none';
  if (merging) return 'merging';
  if (branch.archived) return 'restore';
  return 'switch';
}

/** 브랜치 목록에서 보관 브랜치 묶음의 접힘 상태를 저장할 때 쓰는 이름(브랜치 이름과 겹치지 않게 ':'로 시작). */
export const ARCHIVED_GROUP = ':archived';

/** 탐색기 레인 중 보관 브랜치만, id마다 한 번, 이름순. */
export function archivedOf(lanes: ExplorerLane[]): Branch[] {
  const seen = new Set<string>();
  return lanes.map((l) => l.branch).filter((b) => b.archived && !seen.has(b.id) && !!seen.add(b.id)).sort((a, b) => a.name.localeCompare(b.name));
}

/** 보관 브랜치의 마지막 리비전: 탐색기가 읽은 리비전 중 그 브랜치 것 가운데 가장 최신. 읽은 범위에 없으면 ''. */
export function lastRevisionOf(branchId: string, nodes: ExplorerNode[]): string {
  const own = nodes.filter((n) => n.revision.branch_id === branchId);
  const latest = own.reduce<ExplorerNode | undefined>((best, n) => !best || n.revision.timestamp > best.revision.timestamp || (n.revision.timestamp === best.revision.timestamp && n.revision.number > best.revision.number) ? n : best, undefined);
  return latest?.revision.id ?? '';
}

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

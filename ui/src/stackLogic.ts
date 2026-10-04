import type { Pick, Revision } from './types';

/** 화면에 보이는 최신순 드래프트를 옮겨, 적용 순서인 오래된 순서로 반환한다. */
export function reorderPicks(drafts: Revision[], dragged: string, target: string, side: 'before' | 'after'): Pick[] | null {
  if (dragged === target) return null;
  const visual = drafts.filter((d) => d.id !== dragged);
  const at = visual.findIndex((d) => d.id === target) + (side === 'after' ? 1 : 0);
  visual.splice(at, 0, drafts.find((d) => d.id === dragged)!);
  if (visual.every((d, i) => d.id === drafts[i].id)) return null;
  return [...visual].reverse().map((r) => ({ id: r.id, message: r.message }));
}

import type { Pick } from './types';

/** 최신순 목록에서 위아래로 옮기고, 적용 순서인 오래된 순서로 돌려준다. */
export function movePick(picks: Pick[], id: string, by: -1 | 1): Pick[] {
  const list = [...picks].reverse();
  const i = list.findIndex((p) => p.id === id);
  const j = i + by;
  if (j < 0 || j >= list.length) return picks;
  [list[i], list[j]] = [list[j], list[i]];
  return list.reverse();
}

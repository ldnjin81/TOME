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

/** 합치기 요청: 아래에서 위로(오래된 순) 이어진 드래프트 묶음과 그 위의 나머지. */
export interface FoldRequest {
  base: string;
  /** 합칠 드래프트, 오래된 순. */
  group: string[];
  /** 합친 리비전의 기본 메시지(묶음의 메시지를 오래된 순으로 이은 것). */
  message: string;
  /** 묶음 위의 드래프트, 오래된 순(다시 얹는다). */
  rest: Pick[];
  /** 대화상자에 보여 줄 묶음 메시지, 오래된 순. */
  messages: string[];
}

/**
 * 최신순 드래프트 목록에서 `upper`를 바로 아래 드래프트와 합치는 요청을 만든다.
 * 아래에 드래프트가 없거나 아래 드래프트의 부모를 모르면 null.
 */
export function foldWithBelow(drafts: Revision[], upper: string): FoldRequest | null {
  const i = drafts.findIndex((d) => d.id === upper);
  if (i < 0 || i + 1 >= drafts.length) return null;
  const lower = drafts[i + 1];
  const base = lower.parents[0];
  if (!base) return null;
  const messages = [lower.message, drafts[i].message];
  return {
    base,
    group: [lower.id, drafts[i].id],
    message: messages.map((m) => m.trim()).filter(Boolean).join('\n\n'),
    rest: drafts.slice(0, i).reverse().map((r) => ({ id: r.id, message: r.message })),
    messages,
  };
}

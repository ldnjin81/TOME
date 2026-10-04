import { describe, expect, it } from 'vitest';
import { movePick } from './restackLogic';
import type { Pick } from './types';

const picks: Pick[] = [
  { id: 'a', message: '첫째' },
  { id: 'b', message: '둘째' },
  { id: 'c', message: '셋째' },
];
const ids = (items: Pick[]) => items.map((pick) => pick.id);

describe('movePick', () => {
  it('가운데 항목을 화면 위로 옮기면 적용 순서는 뒤로 간다', () => {
    expect(ids(movePick(picks, 'b', -1))).toEqual(['a', 'c', 'b']);
  });

  it('가운데 항목을 화면 아래로 옮기면 적용 순서는 앞으로 간다', () => {
    expect(ids(movePick(picks, 'b', 1))).toEqual(['b', 'a', 'c']);
  });

  it('화면 맨 위에서 더 올리거나 맨 아래에서 더 내릴 수 없다', () => {
    expect(movePick(picks, 'c', -1)).toBe(picks);
    expect(movePick(picks, 'a', 1)).toBe(picks);
  });

  it('항목이 하나뿐이면 순서를 바꾸지 않는다', () => {
    const one = [picks[0]];
    expect(movePick(one, 'a', -1)).toBe(one);
    expect(movePick(one, 'a', 1)).toBe(one);
  });

  it('원본 picks 배열은 수정하지 않는다', () => {
    movePick(picks, 'b', -1);
    expect(ids(picks)).toEqual(['a', 'b', 'c']);
  });
});

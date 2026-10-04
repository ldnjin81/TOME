import { describe, expect, it } from 'vitest';
import { orderBranches } from './branchLogic';
import type { Branch } from './types';

const branch = (name: string): Branch => ({ id: name, name, latest: '', current: false, archived: false, creator: '', created: 0 });
const names = (items: Branch[]) => items.map((item) => item.name);

describe('orderBranches', () => {
  it('main을 현재 브랜치보다 앞에 둔다', () => {
    expect(names(orderBranches(['topic', 'main', 'other'].map(branch), 'topic').top)).toEqual(['main', 'topic', 'other']);
  });

  it('현재 브랜치가 접두사에 속해도 최상위에 둔다', () => {
    const ordered = orderBranches(['auto/x', 'main', 'auto/y'].map(branch), 'auto/y');
    expect(names(ordered.top)).toEqual(['main', 'auto/y']);
    expect(ordered.groups.map(([prefix, list]) => [prefix, names(list)])).toEqual([['auto', ['auto/x']]]);
  });

  it('숫자가 든 일반 브랜치를 자연 정렬한다', () => {
    expect(names(orderBranches(['task-10', 'task-2', 'task-1'].map(branch), '').top)).toEqual(['task-1', 'task-2', 'task-10']);
  });

  it('접두사별로 묶고 그룹과 내부 이름을 정렬한다', () => {
    const ordered = orderBranches(['feature/x', 'auto/10', 'auto/2', 'feature/a'].map(branch), '');
    expect(ordered.groups.map(([prefix, list]) => [prefix, names(list)])).toEqual([
      ['auto', ['auto/2', 'auto/10']],
      ['feature', ['feature/a', 'feature/x']],
    ]);
  });

  it('첫 글자가 슬래시인 이름은 그룹으로 묶지 않는다', () => {
    expect(names(orderBranches(['/orphan', 'auto/x'].map(branch), '').top)).toEqual(['/orphan']);
  });
});

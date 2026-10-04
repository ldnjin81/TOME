import type { Branch } from './types';

/** main, 현재 브랜치, 나머지 순으로 두고 접두사별 브랜치를 묶는다. */
export function orderBranches(branches: Branch[], current: string) {
  const rank = (b: Branch) => (b.name === 'main' ? 0 : b.name === current ? 1 : 2);
  const byName = (a: Branch, b: Branch) => rank(a) - rank(b) || a.name.localeCompare(b.name, undefined, { numeric: true });
  const top: Branch[] = [];
  const groups = new Map<string, Branch[]>();
  for (const b of branches) {
    const slash = b.name.indexOf('/');
    if (slash <= 0 || b.name === current) top.push(b);
    else groups.set(b.name.slice(0, slash), [...(groups.get(b.name.slice(0, slash)) ?? []), b]);
  }
  return {
    top: top.sort(byName),
    groups: [...groups.entries()].sort(([a], [b]) => a.localeCompare(b)).map(([g, list]) => [g, list.sort(byName)] as [string, Branch[]]),
  };
}

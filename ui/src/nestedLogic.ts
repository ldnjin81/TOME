/** `path`가 중첩 저장소 루트 중 하나와 같거나 그 안에 있으면 그 루트(경계는 폴더 단위). */
export function nestedRootOf(path: string, roots: string[]): string | undefined {
  const p = path.replace(/\\/g, '/').replace(/^\/+|\/+$/g, '');
  return roots.find((r) => p === r || p.startsWith(`${r}/`));
}

/** 작업본 경로 아래 상대 경로를 그 OS의 구분자로 붙인다. */
export function joinUnder(base: string, relative: string): string {
  const windows = /\\/.test(base) || /^[A-Za-z]:/.test(base);
  const sep = windows ? '\\' : '/';
  return `${base.replace(/[\\/]+$/, '')}${sep}${relative.split('/').join(sep)}`;
}
